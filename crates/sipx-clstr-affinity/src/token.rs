//! Mint and verify — [`affinity-token`] §2–§10.
//!
//! Both are pure functions. Time enters as `now`, randomness as the caller's nonce, keys as data;
//! neither reads a clock, touches a socket, or consults a store. Verification in particular is
//! **stateless by design** (§9): there is no nonce ledger and no seen-token store, because
//! re-presenting the same token on every mid-dialog request is the mechanism rather than an
//! attack, and a replay store would reintroduce exactly the hot-path global state the token exists
//! to remove.
//!
//! [`affinity-token`]: https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md

use bytes::Bytes;
use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use thiserror::Error;

use crate::codec::encode_param_value;
use crate::keys::{Algorithm, KeySet, MintKey};

/// Layout version 1 (§3). Version `0x81` is the flow-reference family (§11.3 DS1).
pub const VERSION: u8 = 0x01;

/// Bytes 0–13: version, key id, nonce. Always cleartext — the parser needs version and key id
/// before any cryptography, and the AEAD needs the nonce.
pub(crate) const HEADER_LEN: usize = 14;

/// The body's fixed part: the seven claim fields plus the `facts len` byte (§3, "Totals").
pub(crate) const BODY_FIXED_LEN: usize = 20;

/// The module-fact sub-budget (§3). `mint` refuses more regardless of what a hook framework
/// assembled — the layout ceiling is not the hook framework's to relax (§7, M8).
pub const MAX_FACTS: usize = 64;

/// The largest a body can be: the fixed part plus a full facts region.
const MAX_BODY_LEN: usize = BODY_FIXED_LEN + MAX_FACTS;

/// The largest a token can be — encrypted mode at the facts ceiling (§3, `AT-6`).
pub const MAX_TOKEN_LEN: usize = HEADER_LEN + MAX_BODY_LEN + 16;

/// S1's structural floor: the smallest valid token is authenticated-only with an empty region.
pub const MIN_TOKEN_LEN: usize = HEADER_LEN + BODY_FIXED_LEN + 12;

/// The default clock-skew allowance `S` (§8, S6).
pub const DEFAULT_SKEW: u32 = 30;

/// The default token lifetime `L` (§7, M5).
pub const DEFAULT_LIFETIME: u32 = 86_400;

/// The configurable floor under `L` (§7, M5).
pub const LIFETIME_FLOOR: u32 = 600;

// Body-relative offsets. The spec tabulates absolute ones; subtracting `HEADER_LEN` once here is
// what lets the AEAD and HMAC paths share one reader, since only one of them has the body in
// place.
const OFF_TENANT: usize = 0;
const OFF_HOME_SHARD: usize = 4;
const OFF_EDGE: usize = 6;
const OFF_DIRECTION: usize = 8;
const OFF_MEDIA_NODE: usize = 9;
const OFF_POLICY_VERSION: usize = 11;
const OFF_EXPIRY: usize = 15;
const OFF_FACTS_LEN: usize = 19;

/// The dialog side that presents this token (§3, §7 M2).
///
/// ORIG names the side that sent the dialog-forming request, TERM the side it was forwarded to.
/// Each token's direction names the side that will *present* it, so route-set learning
/// (RFC 3261 §12.1.1 in order for the UAS, §12.1.2 reversed for the UAC) hands each endpoint its
/// own side's entry first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// `0x01` — the originating side.
    Originating,
    /// `0x02` — the terminating side.
    Terminating,
}

impl Direction {
    /// The wire byte (§3).
    #[must_use]
    pub const fn to_wire(self) -> u8 {
        match self {
            Self::Originating => 0x01,
            Self::Terminating => 0x02,
        }
    }

    /// S7: `0x01` and `0x02` are the only accepted values; all 254 others are corruption.
    #[must_use]
    pub const fn from_wire(byte: u8) -> Option<Self> {
        match byte {
            0x01 => Some(Self::Originating),
            0x02 => Some(Self::Terminating),
            _ => None,
        }
    }

    /// The other side. S9's pair check is "complementary direction", which is this.
    #[must_use]
    pub const fn complement(self) -> Self {
        match self {
            Self::Originating => Self::Terminating,
            Self::Terminating => Self::Originating,
        }
    }
}

/// A dialog's routing state — everything a mid-dialog request needs in order to be forwarded with
/// no lookup anywhere (§3).
///
/// `module_facts` is opaque at this layer: contributed by extension modules at hook phase H9,
/// carried verbatim, and returned verbatim in the verdict. Mint and verify never interpret it;
/// its inner framing is [`hook-framework`] §5's.
///
/// [`hook-framework`]: https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/hook-framework.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claims {
    /// Logical tenant id; `0` is reserved (none/system).
    pub tenant: u32,
    /// Rendezvous shard owning the connection-bound side's registration state; `0` = none.
    pub home_shard: u16,
    /// Logical id of the minting edge — the connection-owner hint; `0` = none.
    pub edge: u16,
    /// The side that presents this token.
    pub direction: Direction,
    /// Logical id of the media relay holding the dialog's media session; `0` = no media session.
    pub media_node: u16,
    /// Tenant policy/config version at mint, so an edge can detect stale cached policy.
    pub policy_version: u32,
    /// Absolute UNIX seconds. `u32` is valid until 2106.
    pub expiry: u32,
    /// The module-facts region, `0 ..= 64` bytes, opaque here.
    pub module_facts: Bytes,
}

/// Why verification failed — **telemetry only, never on the wire** (§8).
///
/// Every failure collapses to one `403` for the sender. Distinguishing tampered from expired from
/// unknown-key on the wire would hand an attacker a debugging oracle; the operator gets the
/// reason, the network does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// S1: too short to be any token, or the version byte is not `0x01`.
    Structure,
    /// S2: the key id is not in the key set, or its verify window is closed at `now`.
    UnknownKey,
    /// S3: the length is outside the range the key's algorithm allows.
    Length,
    /// S4: the AEAD did not open, or the truncated HMAC did not match.
    Tag,
    /// S5: the `facts len` byte disagrees with the body length.
    Framing,
    /// S6: `now > expiry + S`.
    Expired,
    /// S7: the direction byte is neither `0x01` nor `0x02`.
    Field,
    /// S8: the ingress pins a tenant and the token names a different one.
    Scope,
    /// S9: the partner token of a popped pair is missing, invalid, or inconsistent with this one.
    Pair,
}

impl Reason {
    /// The §8 step that produced this reason, for telemetry that has to say *where*.
    #[must_use]
    pub const fn step(self) -> &'static str {
        match self {
            Self::Structure => "S1",
            Self::UnknownKey => "S2",
            Self::Length => "S3",
            Self::Tag => "S4",
            Self::Framing => "S5",
            Self::Expired => "S6",
            Self::Field => "S7",
            Self::Scope => "S8",
            Self::Pair => "S9",
        }
    }
}

/// The outcome of verification (§2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The token verified; these are its claims, `module_facts` verbatim.
    Valid(Claims),
    /// The token did not verify. The reason never reaches the wire.
    Invalid(Reason),
}

/// The ingress context the proxy already has when it verifies (§2).
///
/// `skew` is here rather than in the §2 sketch because §8 S6 makes `S` configurable and §2 makes
/// `verify` a pure function: a configurable value a pure function reads has to arrive as an input.
#[derive(Debug, Clone, Copy)]
pub struct Expect<'a> {
    /// A listener or interconnect bound to one tenant pins it here; S8 then requires equality.
    pub tenant: Option<u32>,
    /// When a second consecutive platform `Route` was popped, its token — S9's pair check.
    pub partner: Option<&'a [u8]>,
    /// The clock-skew allowance `S` (§8, S6). [`DEFAULT_SKEW`] unless configured otherwise.
    pub skew: u32,
}

impl Default for Expect<'_> {
    fn default() -> Self {
        Self {
            tenant: None,
            partner: None,
            skew: DEFAULT_SKEW,
        }
    }
}

impl<'a> Expect<'a> {
    /// The unpinned, unpaired context at the default skew — a single platform `Route`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pin the tenant this ingress is bound to (S8).
    #[must_use]
    pub const fn with_tenant(mut self, tenant: u32) -> Self {
        self.tenant = Some(tenant);
        self
    }

    /// Supply the partner token of a popped pair (S9).
    #[must_use]
    pub const fn with_partner(mut self, partner: &'a [u8]) -> Self {
        self.partner = Some(partner);
        self
    }

    /// Override the skew allowance `S`.
    #[must_use]
    pub const fn with_skew(mut self, skew: u32) -> Self {
        self.skew = skew;
        self
    }
}

/// A minted token: at most [`MAX_TOKEN_LEN`] bytes, held inline.
///
/// No allocation on the mint path, deliberately — §3's layout is fixed-offset arithmetic with one
/// bounded variable region, so a heap allocation per Record-Route entry would be pure overhead on
/// the one path M2 exists to keep cheap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    bytes: [u8; MAX_TOKEN_LEN],
    len: usize,
}

impl Token {
    /// The token's wire bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or_default()
    }

    /// The token's length in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the token is empty. Never true for a minted token; present because clippy asks.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The `aft` parameter **value**: base64url, unpadded (§5).
    ///
    /// The parameter name and its 200-byte budget belong to the forwarding core, which is what
    /// assembles the URI; this is only the value.
    #[must_use]
    pub fn to_param_value(&self) -> String {
        encode_param_value(self.as_bytes())
    }
}

/// Why a set of claims cannot be minted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MintError {
    /// §7 M8: the facts region is over the 64-byte sub-budget.
    #[error("the module-facts region is {len} bytes, over the {MAX_FACTS} byte sub-budget")]
    FactsTooLong {
        /// How long it was.
        len: usize,
    },
    /// The construction refused the key or the buffer. Unreachable for a 32-byte secret and a body
    /// this size; present because a total function may not swallow an error it did not prove
    /// impossible, and `unwrap` is a lint error in library code here.
    #[error("the key's construction refused the input")]
    Crypto,
}

/// Where a nonce comes from — the injected randomness seam (§7 M4, AGENTS.md rule 2).
///
/// This crate deliberately implements it for nothing. A production source belongs in the driver,
/// where the operating-system CSPRNG lives; the deterministic harness supplies a replayable one;
/// and the §10 vectors supply fixed nonces directly to [`mint`]. A sans-IO crate that could reach
/// entropy on its own would be one `rand::random()` away from a token nobody can replay.
///
/// RFC 4086 governs the production source. §7 M4 governs its use: fresh per token, never shared
/// between the two entries of a pair, and a `(key id, nonce)` pair must never repeat — nonce reuse
/// under an AEAD key is catastrophic (RFC 8439 §4).
pub trait NonceSource {
    /// The next nonce. 12 bytes, matching the RFC 8439 nonce exactly, so the mint nonce *is* the
    /// AEAD nonce with no derivation step.
    fn next_nonce(&mut self) -> [u8; 12];
}

/// Mint a token from an injected nonce source — [`mint`], with §7 M4's freshness made structural.
///
/// # Errors
///
/// [`MintError`], as [`mint`].
pub fn mint_with<S: NonceSource + ?Sized>(
    claims: &Claims,
    key: &MintKey,
    source: &mut S,
) -> Result<Token, MintError> {
    mint(claims, key, source.next_nonce())
}

/// Mint a token (§3, §7).
///
/// The nonce is the caller's: this function has no way to obtain one, which is the sans-IO rule
/// made structural rather than remembered. See [`NonceSource`].
///
/// # Errors
///
/// [`MintError::FactsTooLong`] when the module-facts region is over the 64-byte sub-budget — M8's
/// rule, enforced here regardless of what assembled the region.
pub fn mint(claims: &Claims, key: &MintKey, nonce: [u8; 12]) -> Result<Token, MintError> {
    let facts = claims.module_facts.as_ref();
    if facts.len() > MAX_FACTS {
        return Err(MintError::FactsTooLong { len: facts.len() });
    }
    let body_len = BODY_FIXED_LEN + facts.len();

    let mut bytes = [0u8; MAX_TOKEN_LEN];
    let mut header = [0u8; HEADER_LEN];
    let mut body = [0u8; MAX_BODY_LEN];

    write_bytes(&mut header, 0, &[VERSION, key.id()]);
    write_bytes(&mut header, 2, &nonce);

    write_bytes(&mut body, OFF_TENANT, &claims.tenant.to_be_bytes());
    write_bytes(&mut body, OFF_HOME_SHARD, &claims.home_shard.to_be_bytes());
    write_bytes(&mut body, OFF_EDGE, &claims.edge.to_be_bytes());
    write_bytes(&mut body, OFF_DIRECTION, &[claims.direction.to_wire()]);
    write_bytes(&mut body, OFF_MEDIA_NODE, &claims.media_node.to_be_bytes());
    write_bytes(
        &mut body,
        OFF_POLICY_VERSION,
        &claims.policy_version.to_be_bytes(),
    );
    write_bytes(&mut body, OFF_EXPIRY, &claims.expiry.to_be_bytes());
    // §3: the `facts len` byte is the region's own framing, and S5 checks it against the body
    // length so that truncation cannot masquerade as a shorter region. The conversion cannot fail
    // after the check above; a length that does not fit in a byte is over the sub-budget by
    // definition, so the same error is the honest answer.
    let Ok(facts_len) = u8::try_from(facts.len()) else {
        return Err(MintError::FactsTooLong { len: facts.len() });
    };
    write_bytes(&mut body, OFF_FACTS_LEN, &[facts_len]);
    write_bytes(&mut body, BODY_FIXED_LEN, facts);

    let tag_len = key.algorithm().tag_len();
    let mut tag = [0u8; 16];
    match key.algorithm() {
        Algorithm::ChaCha20Poly1305 => {
            // In place: ChaCha20 is length-preserving, so the ciphertext occupies exactly the
            // offsets the plaintext did and §3's layout is identical in both modes.
            let cipher = ChaCha20Poly1305::new(key.secret().into());
            let Some(plaintext) = body.get_mut(..body_len) else {
                return Err(MintError::Crypto);
            };
            let Ok(computed) =
                cipher.encrypt_in_place_detached((&nonce).into(), &header, plaintext)
            else {
                return Err(MintError::Crypto);
            };
            write_bytes(&mut tag, 0, computed.as_slice());
        }
        Algorithm::HmacSha256_96 => {
            let Some(mut mac) = new_hmac(key) else {
                return Err(MintError::Crypto);
            };
            mac.update(&header);
            mac.update(body.get(..body_len).unwrap_or_default());
            let full: [u8; 32] = mac.finalize().into_bytes().into();
            write_bytes(&mut tag, 0, full.get(..tag_len).unwrap_or_default());
        }
    }

    write_bytes(&mut bytes, 0, &header);
    write_bytes(
        &mut bytes,
        HEADER_LEN,
        body.get(..body_len).unwrap_or_default(),
    );
    write_bytes(
        &mut bytes,
        HEADER_LEN + body_len,
        tag.get(..tag_len).unwrap_or_default(),
    );

    Ok(Token {
        bytes,
        len: HEADER_LEN + body_len + tag_len,
    })
}

/// Verify a token (§8).
///
/// The steps run in order and the **first failing step wins** — which is normative, not an
/// implementation detail: it is what keeps `AT-14`'s bad version from ever reaching a key lookup
/// and `AT-10`'s retired key from ever reaching the cryptography.
///
/// Stateless (§9). Nothing is remembered between calls, so the legitimate re-presentation of one
/// token on every mid-dialog request of a dialog verifies every time.
#[must_use]
pub fn verify(bytes: &[u8], keys: &KeySet, now: u32, expect: &Expect<'_>) -> Verdict {
    let parsed = match verify_through_s7(bytes, keys, now, expect.skew) {
        Ok(parsed) => parsed,
        Err(reason) => return Verdict::Invalid(reason),
    };

    // S8 — context scope.
    if let Some(pinned) = expect.tenant
        && pinned != parsed.claims.tenant
    {
        return Verdict::Invalid(Reason::Scope);
    }

    // S9 — pair consistency. The partner passes S1–S7 under the same rules; S8's pin is this
    // ingress's, applied once, and a nested pair check would not terminate.
    if let Some(partner) = expect.partner {
        let Ok(other) = verify_through_s7(partner, keys, now, expect.skew) else {
            return Verdict::Invalid(Reason::Pair);
        };
        if !pair_consistent(&parsed, &other) {
            return Verdict::Invalid(Reason::Pair);
        }
    }

    // The first-popped token governs: its direction names the presenting side.
    Verdict::Valid(parsed.claims)
}

/// A token that has passed S1–S7, with the nonce S9 needs to compare.
struct Parsed {
    claims: Claims,
    nonce: [u8; 12],
}

/// S1–S7 — everything that is a property of one token in isolation.
fn verify_through_s7(bytes: &[u8], keys: &KeySet, now: u32, skew: u32) -> Result<Parsed, Reason> {
    // S1 — structure. Length and version before anything else, so a flow reference (version
    // `0x81`, §11.3 DS3) or a zeroed buffer is refused before a key is even looked up.
    if bytes.len() < MIN_TOKEN_LEN || bytes.first() != Some(&VERSION) {
        return Err(Reason::Structure);
    }

    // S2 — key lookup. An unknown or out-of-window key id is rejected before any cryptography.
    let key_id = *bytes.get(1).ok_or(Reason::Structure)?;
    let key = keys.verify_key(key_id, now).ok_or(Reason::UnknownKey)?;
    let algorithm = key.algorithm();

    // S3 — the length range the key's algorithm allows. The tag length is fixed by the algorithm,
    // so the rest is body; this is what makes an over-budget facts region (`AT-18`) a length
    // failure rather than a framing one.
    if bytes.len() < algorithm.min_token_len() || bytes.len() > algorithm.max_token_len() {
        return Err(Reason::Length);
    }
    let body_len = bytes.len() - HEADER_LEN - algorithm.tag_len();

    let header = bytes.get(..HEADER_LEN).ok_or(Reason::Structure)?;
    let nonce: [u8; 12] = header
        .get(2..HEADER_LEN)
        .ok_or(Reason::Structure)?
        .try_into()
        .map_err(|_| Reason::Structure)?;

    // S4 — authenticate. Only now is the body readable, and nothing below this line has looked at
    // an unauthenticated byte (§4).
    let mut body = [0u8; MAX_BODY_LEN];
    let cipher_or_plain = bytes
        .get(HEADER_LEN..HEADER_LEN + body_len)
        .ok_or(Reason::Structure)?;
    write_bytes(&mut body, 0, cipher_or_plain);
    let body = body.get_mut(..body_len).ok_or(Reason::Structure)?;
    let tag = bytes
        .get(HEADER_LEN + body_len..)
        .ok_or(Reason::Structure)?;

    match algorithm {
        Algorithm::ChaCha20Poly1305 => {
            let cipher = ChaCha20Poly1305::new(key.secret().into());
            let tag: [u8; 16] = tag.try_into().map_err(|_| Reason::Length)?;
            cipher
                .decrypt_in_place_detached((&nonce).into(), header, body, (&tag).into())
                .map_err(|_| Reason::Tag)?;
        }
        Algorithm::HmacSha256_96 => {
            // Constant-time, and truncation-aware: `verify_truncated_left` compares the tag
            // against the leftmost bytes of the full HMAC, which is §4's construction exactly.
            let mut mac = new_hmac(key).ok_or(Reason::Tag)?;
            mac.update(header);
            mac.update(body);
            mac.verify_truncated_left(tag).map_err(|_| Reason::Tag)?;
        }
    }

    // S5 — facts framing. The declared length must equal the body length minus the fixed part, so
    // a truncated region cannot masquerade as a shorter one.
    let declared_facts = usize::from(*body.get(OFF_FACTS_LEN).ok_or(Reason::Framing)?);
    if declared_facts != body_len - BODY_FIXED_LEN {
        return Err(Reason::Framing);
    }

    let expiry = u32_at(body, OFF_EXPIRY).ok_or(Reason::Framing)?;
    // S6 — expiry, against the `now` the caller passed in. Saturating, so a token minted near the
    // end of the u32 epoch does not wrap into permanent validity.
    if now > expiry.saturating_add(skew) {
        return Err(Reason::Expired);
    }

    // S7 — field validity.
    let direction = Direction::from_wire(*body.get(OFF_DIRECTION).ok_or(Reason::Field)?)
        .ok_or(Reason::Field)?;

    let facts = body.get(BODY_FIXED_LEN..).ok_or(Reason::Framing)?;
    Ok(Parsed {
        claims: Claims {
            tenant: u32_at(body, OFF_TENANT).ok_or(Reason::Framing)?,
            home_shard: u16_at(body, OFF_HOME_SHARD).ok_or(Reason::Framing)?,
            edge: u16_at(body, OFF_EDGE).ok_or(Reason::Framing)?,
            direction,
            media_node: u16_at(body, OFF_MEDIA_NODE).ok_or(Reason::Framing)?,
            policy_version: u32_at(body, OFF_POLICY_VERSION).ok_or(Reason::Framing)?,
            expiry,
            module_facts: Bytes::copy_from_slice(facts),
        },
        nonce,
    })
}

/// S9's comparison: equal claims apart from direction, complementary directions, distinct nonces.
fn pair_consistent(first: &Parsed, second: &Parsed) -> bool {
    let claims = &first.claims;
    let other = &second.claims;
    claims.direction == other.direction.complement()
        && first.nonce != second.nonce
        && claims.tenant == other.tenant
        && claims.home_shard == other.home_shard
        && claims.edge == other.edge
        && claims.media_node == other.media_node
        && claims.policy_version == other.policy_version
        && claims.expiry == other.expiry
        // Byte-identical, per M3 — the two entries of a pair carry one facts region, not two that
        // happen to be the same length.
        && claims.module_facts == other.module_facts
}

/// HMAC-SHA-256 accepts a key of any length, so this is `None` for no input the type system
/// allows; returning `Option` rather than asserting that is what keeps both call sites total.
fn new_hmac(key: &MintKey) -> Option<Hmac<Sha256>> {
    // Fully qualified: `KeyInit` (the AEAD's) and `Mac` (the HMAC's) both offer `new_from_slice`,
    // and both traits are in scope here because this module owns both constructions.
    <Hmac<Sha256> as Mac>::new_from_slice(key.secret()).ok()
}

/// Copy `source` into `target` at `offset`, doing nothing if it would not fit.
///
/// Every call site has already bounded the length; this exists so that none of them needs raw
/// indexing, which is a lint error in library code here for the reason non-negotiable #3 gives.
fn write_bytes(target: &mut [u8], offset: usize, source: &[u8]) {
    if let Some(slot) = target.get_mut(offset..offset + source.len()) {
        slot.copy_from_slice(source);
    }
}

fn u16_at(buffer: &[u8], offset: usize) -> Option<u16> {
    buffer
        .get(offset..offset + 2)?
        .try_into()
        .ok()
        .map(u16::from_be_bytes)
}

fn u32_at(buffer: &[u8], offset: usize) -> Option<u32> {
    buffer
        .get(offset..offset + 4)?
        .try_into()
        .ok()
        .map(u32::from_be_bytes)
}
