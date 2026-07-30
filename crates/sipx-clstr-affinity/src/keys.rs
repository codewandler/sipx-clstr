//! The key model and rotation rules — [`affinity-token`] §6.
//!
//! Keys arrive exclusively from deployment configuration (the schema is `DP-1`'s); there is no
//! key-exchange protocol, no discovery, and no key material in any message. What lives here is
//! the shape that configuration produces and the two questions verification asks of it: *is this
//! key id verify-valid right now*, and *which key mints*.
//!
//! [`affinity-token`]: https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md

use core::fmt;

use thiserror::Error;

/// The two constructions of version 1 (§4).
///
/// The algorithm is a property of the **key**, not of the token: a key id lookup yields
/// `(secret, algorithm)`, and the algorithm is what fixes the tag length and therefore the valid
/// total-length range (§8, S3). Nothing on the wire names it, so an attacker cannot ask for the
/// weaker one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    /// RFC 8439 AEAD. Nonce = the token nonce, AAD = the header, plaintext = the body.
    ///
    /// The default for a deployment: the whole body is ciphertext, which is what keeps tenant ids
    /// and internal topology off the wire (§4, "Confidential fields").
    ChaCha20Poly1305,
    /// HMAC-SHA-256 over `header ‖ body`, truncated to its first 12 bytes (RFC 2104 §5).
    ///
    /// The explicit opt-out: the body is cleartext, so every field is visible to both endpoints
    /// and every on-path element.
    HmacSha256_96,
}

impl Algorithm {
    /// Tag length in bytes — 16 for the AEAD, 12 for the truncated HMAC (§3).
    #[must_use]
    pub const fn tag_len(self) -> usize {
        match self {
            Self::ChaCha20Poly1305 => 16,
            Self::HmacSha256_96 => 12,
        }
    }

    /// The smallest valid total token length for this algorithm — the empty facts region (§8, S3).
    #[must_use]
    pub const fn min_token_len(self) -> usize {
        crate::token::HEADER_LEN + crate::token::BODY_FIXED_LEN + self.tag_len()
    }

    /// The largest valid total token length for this algorithm — the 64-byte facts ceiling.
    ///
    /// S3 checks this *before* S5 reads the framing byte, which is why an over-budget facts region
    /// is a length failure rather than a framing one (`AT-18`).
    #[must_use]
    pub const fn max_token_len(self) -> usize {
        self.min_token_len() + crate::token::MAX_FACTS
    }
}

/// A key: the wire id, the construction it selects, and 32 bytes of secret.
///
/// `Debug` is redacted. A key that can be printed is a key that ends up in a log, and this one
/// authenticates every mid-dialog routing decision the platform makes.
#[derive(Clone)]
pub struct MintKey {
    id: u8,
    algorithm: Algorithm,
    secret: [u8; 32],
}

impl MintKey {
    /// Build a key from its configured attributes (§6).
    #[must_use]
    pub const fn new(id: u8, algorithm: Algorithm, secret: [u8; 32]) -> Self {
        Self {
            id,
            algorithm,
            secret,
        }
    }

    /// The wire key id — byte 1 of every record minted under it (§3).
    #[must_use]
    pub const fn id(&self) -> u8 {
        self.id
    }

    /// The construction this key selects (§4).
    #[must_use]
    pub const fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// The 32 secret bytes. Crate-internal: nothing outside the two call sites needs them.
    pub(crate) const fn secret(&self) -> &[u8; 32] {
        &self.secret
    }
}

impl fmt::Debug for MintKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MintKey")
            .field("id", &self.id)
            .field("algorithm", &self.algorithm)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// One configured key entry: the key plus the window in which it verifies, plus whether it mints.
///
/// The window is what makes rotation work with **overlapping** validity (§6 K1–K4): key B is
/// distributed with `mint: false` and an already-open window, so every node can verify a B-token
/// before any node mints one, and key A keeps verifying until its own window closes.
#[derive(Debug, Clone)]
pub struct KeyEntry {
    key: MintKey,
    verify_from: u32,
    verify_until: u32,
    mint: bool,
}

impl KeyEntry {
    /// Build an entry from its configured attributes (§6).
    ///
    /// `verify_from`/`verify_until` are absolute UNIX seconds and the window is **inclusive at both
    /// ends** — S2 reads `verify_from ≤ now ≤ verify_until`.
    #[must_use]
    pub const fn new(key: MintKey, verify_from: u32, verify_until: u32, mint: bool) -> Self {
        Self {
            key,
            verify_from,
            verify_until,
            mint,
        }
    }

    /// The key itself.
    #[must_use]
    pub const fn key(&self) -> &MintKey {
        &self.key
    }

    /// Whether this entry's window is open at `now` (§8, S2).
    #[must_use]
    pub const fn verifies_at(&self, now: u32) -> bool {
        self.verify_from <= now && now <= self.verify_until
    }

    /// Whether new tokens are minted under this key.
    #[must_use]
    pub const fn mints(&self) -> bool {
        self.mint
    }
}

/// The ways a configured key set violates §6, which the config loader must refuse to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum KeySetError {
    /// §6: "Exactly one `mint: true` key at any config version" — there were none.
    #[error("no key is marked mint: true")]
    NoMintKey,
    /// §6: "Exactly one `mint: true` key at any config version" — there was more than one.
    #[error("more than one key is marked mint: true")]
    MultipleMintKeys,
    /// §6: "No two entries share an `id` while both verify windows are open."
    ///
    /// Ids may wrap over the years; two open windows on one id would make key selection ambiguous
    /// for exactly the tokens rotation exists to keep verifying.
    #[error("key id {0} is used by two entries whose verify windows overlap")]
    OverlappingId(u8),
    /// A window that never opens. Not a §6 rule in so many words, but an entry that can never
    /// verify anything is a configuration mistake worth naming at load rather than at 03:00.
    #[error("key id {0} has verify_from after verify_until")]
    EmptyWindow(u8),
}

/// The verify-valid key set a node holds, as configuration produced it.
///
/// [`KeySet::new`] enforces the §6 rules the config loader must enforce, so everything downstream —
/// [`verify`](crate::verify) in particular — can treat the set as well-formed.
#[derive(Debug, Clone)]
pub struct KeySet {
    entries: Vec<KeyEntry>,
}

impl KeySet {
    /// Build a key set, enforcing §6's structural rules.
    ///
    /// # Errors
    ///
    /// [`KeySetError`], one variant per rule.
    pub fn new(entries: Vec<KeyEntry>) -> Result<Self, KeySetError> {
        for entry in &entries {
            if entry.verify_from > entry.verify_until {
                return Err(KeySetError::EmptyWindow(entry.key.id));
            }
        }

        let mints = entries.iter().filter(|entry| entry.mint).count();
        if mints == 0 {
            return Err(KeySetError::NoMintKey);
        }
        if mints > 1 {
            return Err(KeySetError::MultipleMintKeys);
        }

        for (index, entry) in entries.iter().enumerate() {
            for other in entries.iter().skip(index + 1) {
                if entry.key.id == other.key.id
                    && entry.verify_from <= other.verify_until
                    && other.verify_from <= entry.verify_until
                {
                    return Err(KeySetError::OverlappingId(entry.key.id));
                }
            }
        }

        Ok(Self { entries })
    }

    /// S2: the key this id selects, if its window is open at `now`.
    ///
    /// A key id absent from the set, or present with a closed window, is an **unknown key** —
    /// there is no distinction, because there is nothing useful the two could tell a verifier
    /// apart from each other, and telemetry gets the reason either way.
    #[must_use]
    pub fn verify_key(&self, id: u8, now: u32) -> Option<&MintKey> {
        self.entries
            .iter()
            .find(|entry| entry.key.id == id && entry.verifies_at(now))
            .map(KeyEntry::key)
    }

    /// The key new tokens are minted under (§6).
    ///
    /// [`KeySet::new`] guarantees exactly one, so this is `Some` for every set that loaded.
    #[must_use]
    pub fn mint_key(&self) -> Option<&MintKey> {
        self.entries
            .iter()
            .find(|entry| entry.mint)
            .map(KeyEntry::key)
    }

    /// The configured entries, in configuration order.
    #[must_use]
    pub fn entries(&self) -> &[KeyEntry] {
        &self.entries
    }
}

/// §6 K4: the earliest moment the outgoing key may be removed after the mint flip.
///
/// `t_switch + max(L, E_max) + S` — the last possible mint under the outgoing key, plus the longest
/// a record minted under it can still be presented, plus skew. Two terms because two record
/// families ride these keys: `L` (the token lifetime) bounds tokens, which expire; `E_max` (the
/// tenant's maximum registration expiry) bounds flow references, which do not (§11.4 FM7) and leave
/// circulation only when the binding carrying them refreshes.
///
/// Saturating, so a configuration with absurd values yields a deadline at the end of the `u32`
/// epoch rather than a wrapped one that retires a live key immediately.
#[must_use]
pub const fn retirement_deadline(
    t_switch: u32,
    lifetime: u32,
    max_registration_expiry: u32,
    skew: u32,
) -> u32 {
    let longest = if lifetime > max_registration_expiry {
        lifetime
    } else {
        max_registration_expiry
    };
    t_switch.saturating_add(longest).saturating_add(skew)
}
