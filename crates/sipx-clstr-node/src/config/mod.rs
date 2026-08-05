//! The cluster configuration document, read.
//!
//! [cluster-config](../../../../docs/specs/cluster-config.md) specifies one cluster-scoped document
//! that every node reads identically and projects through an identity supplied from outside. This
//! module is [`load`] — the pure half — and [`project`], the total function that turns a cluster
//! document into one node's view of it.
//!
//! **Why this is not a `#[derive(Deserialize)]`.** §8 V1 requires *every* error, ordered by path:
//! "a document with five mistakes costs five seconds, not five restarts". Serde stops at the first
//! one and reports it as a message rather than as a path plus a rule id. So the document is parsed
//! into a generic value tree and walked by hand, accumulating [`ConfigError`]s. The closed-world
//! rule (V2) needs the same shape — you cannot ask serde which keys it *didn't* recognise.
//!
//! **One reader, three encodings.** §2 D3 asks for one data model behind more than one encoding.
//! JSON is a subset of YAML, so those two share a parser outright. TOML cannot — it is a different
//! grammar — so it is parsed by its own reader and then *converted into the same value tree* before
//! anything looks at it. That is the whole trick: the encoding is chosen in one function and every
//! rule below it sees one shape, so there is no second validation path to disagree with the first.
//!
//! # Scope
//!
//! This loader implements the sections a node needs in order to boot and to find its peers:
//! `apiVersion`, `version`, and under `cluster` — `name`, `environment`, `zones`, `listener[]`,
//! `membership`, `keys[]`, `shardMap`, `locationStore`, `tenant[]`, `security` and `timers`.
//!
//! The remaining sections of §7's registry are **recognised but not descended into**: naming one is
//! not an error, and a typo in its name still is. That boundary is deliberate and is reported by
//! [`Config::unapplied`] rather than left for a reader to infer — a section this loader silently
//! ignored would be configuration nobody is applying and nothing anywhere saying so, which is the
//! exact failure V2 exists to prevent, one level up.
//!
//! **Validated is not the same as applied, and `DP-16` is where the two come apart.**
//! [cluster-membership](../../../../docs/specs/cluster-membership.md) §3–§5 fix `membership[].rpc`,
//! the incarnation source, `keys[]` and `shardMap`, and this module enforces every rule of them that
//! a pure function of the document can enforce. No consumer in this build *acts* on any of it — the
//! owner RPC is `AF-3`/`AF-7`'s, the mint/verify key set reaches no driver field yet, and the shard
//! handoff is `RG-5`'s — so every one of those paths is reported by [`Config::unapplied`] for
//! exactly the reason `FC-2` added that list: a document that declared them and got nothing must be
//! told so, and the warning must not lie in either direction.

pub mod error;

use std::collections::{BTreeMap, BTreeSet};

use error::ordered;
pub use error::{ConfigError, Path, RuleId};

use serde_yaml_ng::Value;

use crate::listen::Advertised;

/// The schema version this loader speaks (§3).
pub const API_VERSION: &str = "sipx.dev/v1alpha1";

/// RFC 3261 §16.6 step 3's value, which §8 V6 refuses to make a knob.
pub const MAX_FORWARDS: u8 = 70;

/// How many proxied transactions a node admits at once when the document does not say (`DP-11`).
///
/// The same number as the kernel's own queue capacity (`sipx_transport::Config::capacity`), and
/// deliberately so: the node's admission bound and the queue bound it inherits are the two limits a
/// request passes through, and two unrelated numbers would make "which one refused this?" a question
/// nobody can answer from configuration. A node holding 1024 in-flight proxied transactions is
/// bounded in memory; a node holding as many as arrive is not.
pub const DEFAULT_MAX_IN_FLIGHT_TRANSACTIONS: usize = 1024;

/// The largest admission bound a document may declare.
///
/// A ceiling in the spirit of §8 V8: past this the value stops describing a limit and starts
/// describing the absence of one, and an operator who wants that has misread what the knob is for.
pub const MAX_IN_FLIGHT_CEILING: usize = 65_536;

/// §8 V8's ceiling on `keys[]`, adopted by
/// [cluster-membership](../../../../docs/specs/cluster-membership.md) KY6 unchanged.
///
/// Generous on purpose: §7's rotation keeps at most two entries verify-valid at once, which is also
/// what `affinity-token` §3's one-byte key id is sized around. A document approaching it is a
/// document whose rotations are not being retired.
pub const MAX_KEYS: usize = 16;

/// `L`, the token lifetime — [affinity-token](../../../../docs/specs/affinity-token.md) §7 M5's
/// default of 86 400 s, and one term of the rotation overlap window `W = max(L, E_max) + S`.
///
/// M5 declares `L` configurable with a floor of 600 s, and this schema has no field for it, so the
/// default is the only value a document can mean today. Named rather than inlined so that the field,
/// when it arrives, has one place to be read into.
pub const TOKEN_LIFETIME_SECONDS: i64 = 86_400;

/// `S`, the skew allowance — [affinity-token](../../../../docs/specs/affinity-token.md) §8 S6's 30 s,
/// and the other fixed term of `W`.
pub const SKEW_ALLOWANCE_SECONDS: i64 = 30;

/// §9.4 DS4's declared default drain timeout: 30 s.
pub const DEFAULT_DRAIN_TIMEOUT_MS: u64 = 30_000;

/// DS4's permitted range, in the unit that spec states it in.
///
/// The floor is not a round number chosen for looks: below the location store's CAS retry budget
/// ([location-service](../../../../docs/specs/location-service.md) §5.1 S10) a drain would expire
/// while an ordinary contended write was still legitimately retrying.
pub const MIN_DRAIN_TIMEOUT_MS: u64 = 5_000;
/// DS4's ceiling.
pub const MAX_DRAIN_TIMEOUT_MS: u64 = 300_000;

// The same rule `DP-12` made unrepresentable for Timer C, applied to the range this schema declares
// beside its own default: a default outside the bounds stated next to it is a build failure rather
// than a refusal at every operator's startup.
const _: () = assert!(DEFAULT_DRAIN_TIMEOUT_MS >= MIN_DRAIN_TIMEOUT_MS);
const _: () = assert!(DEFAULT_DRAIN_TIMEOUT_MS <= MAX_DRAIN_TIMEOUT_MS);

/// RFC 3261 §16.6 step 11's floor for Timer C, which the timer must be **strictly** above.
///
/// "The timer MUST be larger than 3 minutes" — a MUST over a strict inequality, so this value is the
/// largest one that is *not* admissible rather than the smallest one that is.
pub const TIMER_C_FLOOR_MS: u64 = 180_000;

/// §8 V7's declared default for Timer C: four minutes.
///
/// The smallest whole-minute value above [`TIMER_C_FLOOR_MS`], stated in the unit the RFC states the
/// floor in. A default *equal* to an exclusive bound is unsatisfiable by omission, and this one was
/// 180 s until `DP-12`: a document carrying a `timers` section without naming `timerC` was refused
/// for the value this loader had supplied. It is not raised beyond the smallest compliant value
/// because Timer C is the only bound on a branch gone quiet since its last provisional
/// (RFC 3261 §16.7 bullet 2), so every extra minute is a minute a wedged branch holds a proxied
/// transaction and an admission slot ([`AdmissionSpec`]).
pub const DEFAULT_TIMER_C_MS: u64 = 240_000;

// The defect `DP-12` closed, made unrepresentable rather than merely fixed: a default that cannot
// satisfy the rule declared beside it is now a build failure instead of a refusal at every operator's
// startup.
const _: () = assert!(DEFAULT_TIMER_C_MS > TIMER_C_FLOOR_MS);

/// The closed role set of §4 R1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Edge,
    Registrar,
    InboundProxy,
    OutboundProxy,
    E2eTester,
    Echo,
}

impl Role {
    /// Every role, in the order §4 R1 lists them — used to spell the closed set in a refusal.
    pub const ALL: [Role; 6] = [
        Role::Edge,
        Role::Registrar,
        Role::InboundProxy,
        Role::OutboundProxy,
        Role::E2eTester,
        Role::Echo,
    ];

    /// The document's spelling: `kebab-case`, per §2 D4.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Edge => "edge",
            Role::Registrar => "registrar",
            Role::InboundProxy => "inbound-proxy",
            Role::OutboundProxy => "outbound-proxy",
            Role::E2eTester => "e2e-tester",
            Role::Echo => "echo",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Role::ALL.into_iter().find(|role| role.as_str() == text)
    }

    /// The four roles that put a node on the call path (§4 R6).
    fn is_call_path(self) -> bool {
        matches!(
            self,
            Role::Edge | Role::Registrar | Role::InboundProxy | Role::OutboundProxy
        )
    }

    /// The three roles that carry somebody else's request onward (§7's `trunk[]`, `routeRule[]`,
    /// `timers` and `admission` columns).
    ///
    /// `registrar` is on the call path and is **not** one of them: it answers REGISTER out of the
    /// location service and forwards nothing, and §7 gives it `locationStore`, `registrar` and
    /// `tenant[]` and no forwarding section at all.
    #[must_use]
    pub fn is_proxying(self) -> bool {
        matches!(self, Role::Edge | Role::InboundProxy | Role::OutboundProxy)
    }

    fn closed_set() -> String {
        Role::ALL
            .iter()
            .map(|role| role.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Which decision paths a node's roles wire up (§4 R3, `DP-13`).
///
/// R3 fixes what a role may and may not do, in one sentence: it "selects which decision paths are
/// wired; it never selects what a request decides". So a role set is turned into this **once**, when
/// the node is built, and the driver then asks which paths exist rather than which roles it was
/// given. That is what keeps R2's "roles is a set" safe — `inbound-proxy` and `outbound-proxy` on
/// one node stay indistinguishable to an arriving request, because neither is consulted when one is
/// classified.
///
/// Until `DP-13` the projection used the roles to pick listeners and the location store and then
/// dropped them, so the driver dispatched on method alone: a node started as `inbound-proxy`
/// answered `200 OK` to a REGISTER and stored the binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// The registrar path: a REGISTER is admitted, decided against the tenant's policy and stored
    /// (`registrar`).
    pub registrar: bool,
    /// The forwarding path: everything a proxy carries onward (`edge`, `inbound-proxy`,
    /// `outbound-proxy`).
    pub proxy: bool,
}

impl Capabilities {
    /// Both paths — what a [`crate::driver::NodeConfig`] built in code has always meant.
    ///
    /// The document path never uses it: `startup::node_config` derives the wiring from the identity
    /// the node was started with, which is the whole of this story. It is the constructors that take
    /// a socket and nothing else ([`crate::driver::NodeConfig::new`]) that need a stated default,
    /// and theirs is the node they have always produced.
    pub const CALL_PATH: Self = Self {
        registrar: true,
        proxy: true,
    };

    /// The wiring `roles` asks for.
    #[must_use]
    pub fn of(roles: &BTreeSet<Role>) -> Self {
        Self {
            registrar: roles.contains(&Role::Registrar),
            proxy: roles.iter().copied().any(Role::is_proxying),
        }
    }

    /// What this node serves, for the line it logs at startup.
    ///
    /// An operator reading one line should be able to tell a registrar from a proxy from a node that
    /// is both — which is exactly the fact that was unobservable while nothing consumed the roles.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match (self.registrar, self.proxy) {
            (true, true) => "registrar+proxy",
            (true, false) => "registrar",
            (false, true) => "proxy",
            // Unreachable from a document — the empty role set is refused at load (R4) and every
            // remaining role wires one of the two — and stated rather than assumed.
            (false, false) => "nothing",
        }
    }
}

/// What a node is, supplied from outside the document (§5 P1).
///
/// In Kubernetes this comes from the downward API and the workload's role; on a plain host, from
/// flags. It is never a section of the document, because deriving it from the document would mean a
/// per-node document, and then §3's `version` would stop being a fact about the cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeIdentity {
    /// The §6 logical node id. `0` is reserved (I2).
    pub node: u16,
    pub zone: String,
    pub roles: BTreeSet<Role>,
}

/// A listener as the document declares it. Bind and advertise stay strings here: turning them into
/// addresses is `DP-5`'s job and its rules are inherited verbatim rather than restated (§5 P7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerSpec {
    pub roles: BTreeSet<Role>,
    pub transport: String,
    pub bind: String,
    pub advertise: Option<String>,
}

/// Where a node's incarnation counter comes from
/// ([cluster-membership](../../../../docs/specs/cluster-membership.md) MB8).
///
/// A member's field rather than a cluster-wide switch: `affinity-token` §12.2 CT2 makes
/// `boot-second` correct only where the clock cannot step backwards across a restart, and that is a
/// property of a machine rather than of a fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IncarnationSource {
    /// CT2's own mechanism, and MB8's declared default: the boot second, made strictly increasing.
    #[default]
    BootSecond,
    /// For a deployment that cannot rule out a backwards clock step on some of its nodes. Needs an
    /// `incarnationRef`, because the counter lives outside the document.
    PersistedCounter,
}

impl IncarnationSource {
    /// The document's spelling, `kebab-case` per §2 D4.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BootSecond => "boot-second",
            Self::PersistedCounter => "persisted-counter",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [Self::BootSecond, Self::PersistedCounter]
            .into_iter()
            .find(|source| source.as_str() == text)
    }
}

/// One node's entry in the cluster's membership (§5 P3,
/// [cluster-membership](../../../../docs/specs/cluster-membership.md) §3).
///
/// The seven fields MB1 closes the world on. `node`, `name`, `zone` and `roles` are what §5 P3
/// cross-checks and what `shardMap` resolves against; the last three are the ones `AF-6` added and
/// nothing in this build consumes yet (MB7 keeps the *bind* side out of here entirely).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSpec {
    pub node: u16,
    pub name: String,
    pub zone: String,
    pub roles: BTreeSet<Role>,
    /// The **advertised** endpoint a peer dials for the connection-owner RPC (MB5, MB6).
    ///
    /// Required exactly when `roles` puts this member on the call path, and refused otherwise — a
    /// missing endpoint on a flow-owning node makes every request toward a client it owns
    /// undeliverable, and an endpoint on a node that owns nothing is a target nobody should reach.
    pub rpc: Option<String>,
    /// MB8. Absent selects `boot-second`, which is a documented mechanism rather than a silence.
    pub incarnation_source: IncarnationSource,
    /// The driver-resolved handle a `persisted-counter` is read from and written to, by reference
    /// for the same reason a secret is (§8 V9).
    pub incarnation_ref: Option<String>,
}

/// The construction a key selects — `affinity-token` §4, adopted here and declared there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyAlgorithm {
    /// RFC 8439 AEAD, and the deployment default: the whole token body is ciphertext.
    #[default]
    ChaCha20Poly1305,
    /// The explicit opt-out — HMAC-SHA-256 truncated to 96 bits, with a cleartext body.
    HmacSha256_96,
}

impl KeyAlgorithm {
    /// The document's spelling, `kebab-case` per §2 D4.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChaCha20Poly1305 => "chacha20-poly1305",
            Self::HmacSha256_96 => "hmac-sha256-96",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [Self::ChaCha20Poly1305, Self::HmacSha256_96]
            .into_iter()
            .find(|algorithm| algorithm.as_str() == text)
    }
}

/// One `keys[]` entry —
/// [cluster-membership](../../../../docs/specs/cluster-membership.md) §4.
///
/// The six attributes of KY1, and no seventh: they are what `affinity-token` §6 requires and what
/// `AF-4`'s mint/verify library consumes, so changing a name, a type or a meaning here is a new
/// `apiVersion` (§3 D7) rather than an in-place edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpec {
    /// The wire key id `affinity-token` §3 carries in byte 1. **`0` is valid here** — unlike every
    /// other id in this schema (§6 I2), because a token's key-id byte has no "none" value.
    pub id: u8,
    pub algorithm: KeyAlgorithm,
    /// The name the driver resolves into §6's exactly-32-bytes. The only way material enters a node
    /// (KY2, §8 V9); an inline `secret` is refused by KY3 and never reaches this struct.
    pub secret_ref: String,
    /// The verify window's open and close, as UNIX seconds — the form `affinity-token` §8 S2
    /// compares `now` against.
    ///
    /// Absolute instants, never durations (KY4): the loader has no clock (§2 D1), so a relative
    /// spelling would be resolved against whatever moment each node happened to reload, and one
    /// document would mean as many windows as there are nodes.
    pub verify_from: i64,
    pub verify_until: i64,
    /// Whether new records are minted under this key. Exactly one entry per document version
    /// carries `true` (KY5, `affinity-token` §6).
    pub mint: bool,
}

/// One shard of [location-service](../../../../docs/specs/location-service.md) §8's key space, and
/// the member that accepts writes for it (SM2, SM3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardSpec {
    pub id: u16,
    /// A member `name`, never an id: a shard map is a table a human reviews in a diff, and sixty-four
    /// rows of `owner: 7` are unreviewable (SM2).
    pub owner: String,
}

/// The shard map —
/// [cluster-membership](../../../../docs/specs/cluster-membership.md) §5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardMapSpec {
    /// How long a `Draining` shard waits for its last in-flight write before the switch is forced
    /// (§9.4 DS4/DS5). Declared there, adopted here unchanged (§8 V3).
    pub drain_timeout_ms: u64,
    /// The shard space, and it is total: ids `1..=N` with no gap and no repeat (SM1).
    pub shards: Vec<ShardSpec>,
}

/// Where registrations live. The DSN is a *reference* — §8 V9 forbids the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationStoreSpec {
    pub backend: String,
    pub dsn_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantSpec {
    pub name: String,
    pub id: u32,
    /// The domains this tenant serves (location-service §5.1 S1/S5). **Enforced** since `FC-4` for
    /// the address of record and since `RG-18` for the distinct Request-URI: a `REGISTER` naming a
    /// host outside this list in either place is refused. Empty means "any", which is the only
    /// backward-compatible reading of a document that declares none.
    pub domains: Vec<String>,
    /// Per-tenant expiry policy (location-service §5.2) and the binding quota (§5.5).
    pub policy: TenantPolicySpec,
    /// The tenant's digest policy, when the document requires authentication (`FC-3`).
    pub auth: Option<AuthSpec>,
}

/// Per-tenant expiry and quota, as the document states them.
///
/// Defaults are location-service §5.2/§5.5's own — 3600 s granted, 60 s minimum, 86400 s maximum, 10
/// bindings — adopted unchanged rather than restated differently (§8 V3). Before `FC-4` these keys
/// loaded and were dropped, so the registrar ran on the library default no matter what the document
/// said: a `maxBindingsPerAor: 3` was accepted and the effective cap stayed 10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantPolicySpec {
    pub default_expires: u32,
    pub min_expires: u32,
    pub max_expires: u32,
    pub max_bindings_per_aor: usize,
}

impl Default for TenantPolicySpec {
    fn default() -> Self {
        Self {
            default_expires: 3_600,
            min_expires: 60,
            max_expires: 86_400,
            max_bindings_per_aor: 10,
        }
    }
}

/// A tenant that requires digest authentication.
///
/// Only the fields this build can honour. `realm` is the protection space and `secretRef` names the
/// 32-byte nonce key, **by reference** — §8 V9 forbids the value in the document, and resolving it is
/// the driver's job because resolution is IO.
///
/// User credentials are **not** part of this block, deliberately. `RG-7` owns where they come from,
/// and the spec does not fix a source here; inventing a schema for them would be a second, wrong
/// contract beside the one that owns them. A block that carries credentials is refused at load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSpec {
    pub realm: String,
    pub secret_ref: String,
}

/// The `cluster.security` controls §7 declares and this build does not apply, with the decision each
/// one would make if it had a consumer.
///
/// The refusal is **per control** rather than per section, and the table is why: an operator who
/// declared three is told about three (§8 V1), and the day a story specifies one of them, that story
/// removes its own row here and nothing else moves. A single "the security section is unsupported"
/// error would have to be torn out wholesale by the first control to land, which is the shape that
/// makes a successor story rewrite a refusal instead of narrowing it.
///
/// The second field completes "nothing in this build decides …" in the refusal message. It describes
/// the **decision**, never the configured value: `FC-8` owns the rule that a refusal must not echo a
/// secret, and a message that quoted a deny-list or a zone would be a fresh instance of the defect
/// that story is filed for.
const UNAPPLIED_SECURITY_CONTROLS: &[(&str, &str)] = &[
    (
        "unknownSource",
        "whether a request from an unconfigured source reaches a SIP decision path",
    ),
    (
        "sanityCheck",
        "which malformed messages are refused before a decision is taken on them",
    ),
    (
        "userAgentDenyList",
        "which User-Agent values are turned away",
    ),
    (
        "internalZone",
        "which addresses count as internal, and so what the other three are relative to",
    ),
];

/// What `cluster.security` contributes to a node: the fixed Max-Forwards, and nothing else.
///
/// The section's other four keys — [`UNAPPLIED_SECURITY_CONTROLS`] — are **refused** rather than
/// carried, so there is no field for one here and no reader that could quietly not use it. A struct
/// field for an unapplied control is exactly the state `FC-6` removed: `unknownSource`, `sanityCheck`,
/// `userAgentDenyList` and `internalZone` were on the loader's allow-list, validated against nothing,
/// and reached no `NodeConfig` field, so a document asking for them started a node with the opposite
/// posture (**V-06**).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecuritySpec {
    /// Always [`MAX_FORWARDS`]. Present as a field because the value is used, not because it is
    /// configurable — §8 V6.
    pub max_forwards: u8,
}

impl Default for SecuritySpec {
    fn default() -> Self {
        Self {
            max_forwards: MAX_FORWARDS,
        }
    }
}

/// How much work a node will take on at once (`DP-11`).
///
/// **Where this key lives, and why it is not `security`.** The bound is per-node overload control,
/// consumed by every role that sits on the call path — `edge`, `inbound-proxy`, `outbound-proxy`.
/// `security` is the `edge`'s section and its members (`unknownSource`, `sanityCheck`,
/// `userAgentDenyList`, `internalZone`) all answer "who may talk to us", which this does not; and
/// `rateLimit[]` is `RT-3`'s, whose subject is arrival *rate* per source rather than resident
/// concurrency. So it is its own section. It is emphatically **not** `security.maxForwards`, which
/// RFC 3261 §16.6 step 3 fixes at 70 and which §8 V6 refuses to make a knob at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionSpec {
    /// How many proxied transactions may be in flight at once.
    ///
    /// Proxied, not all: a REGISTER is answered from the node's own store and never waits behind
    /// this, because a registration storm *is* the overload and a node that refused REGISTERs under
    /// load would make the storm permanent.
    pub max_in_flight_transactions: usize,
}

impl Default for AdmissionSpec {
    fn default() -> Self {
        Self {
            max_in_flight_transactions: DEFAULT_MAX_IN_FLIGHT_TRANSACTIONS,
        }
    }
}

/// RFC 3261 §17's timers, by their RFC names (§8 V7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimersSpec {
    pub t1_ms: u64,
    pub timer_b_ms: u64,
    pub timer_f_ms: u64,
    pub timer_c_ms: u64,
}

impl Default for TimersSpec {
    fn default() -> Self {
        // §8 V7's declared defaults, adopted unchanged rather than restated differently (V3).
        Self {
            t1_ms: 500,
            timer_b_ms: 64 * 500,
            timer_f_ms: 64 * 500,
            timer_c_ms: DEFAULT_TIMER_C_MS,
        }
    }
}

/// The whole cluster, as one document says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub api_version: String,
    pub version: u32,
    pub name: String,
    pub environment: String,
    pub zones: Vec<String>,
    pub listeners: Vec<ListenerSpec>,
    pub membership: Vec<MemberSpec>,
    /// The cluster's key set (`cluster-membership` §4). Validated here, applied by nobody yet.
    pub keys: Vec<KeySpec>,
    /// The shard map (`cluster-membership` §5), when the document declares one.
    pub shard_map: Option<ShardMapSpec>,
    pub location_store: Option<LocationStoreSpec>,
    pub tenants: Vec<TenantSpec>,
    pub security: SecuritySpec,
    pub admission: AdmissionSpec,
    pub timers: TimersSpec,
    /// Every path this document declared that the build **recognised and did not apply**.
    ///
    /// Paths, not section names, because the keys that matter most are not top level:
    /// `cluster.tenant[0].auth` and `cluster.listener[0].tls` are security-relevant and a set of
    /// top-level sections cannot name either. An earlier version of this field *was* such a set, and
    /// a release shipped four silently-discarded security keys with the detector already in the tree.
    ///
    /// Reported rather than dropped. A warning is not a fix — configuration a node's roles genuinely
    /// do not consume is normal (§4 R5), while authentication and transport want a **refusal** — but
    /// whichever it is, it must not be nobody.
    pub unapplied: Vec<Path>,
    /// Every `cluster` section as the document spelled it, canonically rendered.
    ///
    /// §9's reload rules are judged **between two documents** and RL1 makes the class a property of
    /// the *field*, so [`reload`] has to be able to say "`cluster.profile` changed" about a section
    /// this loader never descends into. Holding the rendering rather than the value tree keeps the
    /// parser's types out of a public struct, and canonicalising it (mappings sorted, numbers
    /// normalised) means a document that only reordered its keys is correctly not a change.
    ///
    /// Private because it is machinery for one function rather than a fact about the cluster: a
    /// consumer that wanted to know what a section said should read the section.
    sections: BTreeMap<String, String>,
}

/// One node's view of the cluster (§5 P2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedConfig {
    pub identity: NodeIdentity,
    pub version: u32,
    /// The listeners whose declared roles intersect this node's (§7, §5 P2).
    pub listeners: Vec<ListenerSpec>,
    /// Present only when this node runs `registrar` — R5 projects away what no role consumes.
    pub location_store: Option<LocationStoreSpec>,
    pub tenants: Vec<TenantSpec>,
    pub security: SecuritySpec,
    /// Carried onto every node rather than projected away: the bound protects the process, and the
    /// process is the same binary whatever roles it was given.
    pub admission: AdmissionSpec,
    pub timers: TimersSpec,
}

/// The §7 sections this loader recognises but does not descend into.
///
/// `keys` and `shardMap` left this list in `DP-16`: `AF-6` specified their fields and nothing
/// implemented them, so a document written to the published spec was refused by V2 — the closed
/// world working exactly as designed, and a schema one story ahead of its loader.
const DEFERRED_SECTIONS: &[&str] = &[
    "profile",
    "management",
    "registrar",
    "normalisation",
    "trunk",
    "domain",
    "destinationSet",
    "routeRule",
    "ingress",
    "rateLimit",
    "nat",
    "mediaPool",
    "observability",
    "probe",
    "echo",
];

const CLUSTER_KEYS: &[&str] = &[
    "name",
    "environment",
    "zones",
    "listener",
    "membership",
    "keys",
    "shardMap",
    "locationStore",
    "tenant",
    "security",
    "admission",
    "timers",
];

/// Which of §9's two classes a change to a section falls in (§9.1 RL1).
///
/// RL1 is the whole reason this is a table and not a diff verdict: "the node and the operator must
/// classify a change identically, or the operator will push a change no node applies".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadClass {
    /// Applying it is a restart, staged by the operator (`KO-8`). A document changing one is
    /// **rejected as a reload** (RL2) and the node keeps running the active version.
    Rollout,
    /// Applied in place, without restarting anything (RL4, and `cluster-membership` §6 RD1).
    Reloadable,
}

/// §7's reload class, per section, in §7's own order.
///
/// `mediaPool` is the one over-approximation and says so: §7 splits it — `mode` is `rollout` and
/// `nodes` is `reloadable` — and this loader does not descend into the section, so it cannot tell
/// the two apart. Classifying the whole section as `rollout` refuses a reload that would in fact
/// have been safe; classifying it the other way would apply a `mode` change nothing performs.
/// `KO-7` owns the section and is where the split lands.
const RELOAD_CLASSES: &[(&str, ReloadClass)] = &[
    ("name", ReloadClass::Rollout),
    ("environment", ReloadClass::Rollout),
    ("zones", ReloadClass::Rollout),
    ("profile", ReloadClass::Rollout),
    ("listener", ReloadClass::Rollout),
    ("management", ReloadClass::Rollout),
    ("membership", ReloadClass::Reloadable),
    ("keys", ReloadClass::Reloadable),
    ("shardMap", ReloadClass::Reloadable),
    ("locationStore", ReloadClass::Rollout),
    ("registrar", ReloadClass::Rollout),
    ("tenant", ReloadClass::Reloadable),
    ("normalisation", ReloadClass::Reloadable),
    ("trunk", ReloadClass::Reloadable),
    ("domain", ReloadClass::Reloadable),
    ("destinationSet", ReloadClass::Reloadable),
    ("routeRule", ReloadClass::Reloadable),
    ("ingress", ReloadClass::Reloadable),
    ("rateLimit", ReloadClass::Reloadable),
    ("timers", ReloadClass::Reloadable),
    ("security", ReloadClass::Reloadable),
    ("admission", ReloadClass::Reloadable),
    ("nat", ReloadClass::Reloadable),
    ("mediaPool", ReloadClass::Rollout),
    ("observability", ReloadClass::Reloadable),
    ("probe", ReloadClass::Reloadable),
    ("echo", ReloadClass::Reloadable),
];

/// The class §7 gives a section.
///
/// A section this table does not name is treated as `rollout`, which is the fail-closed answer: an
/// unclassified section applied in place would be a change nothing performed. The unit test beside
/// this module proves the table covers every recognised section, so the fallback is unreachable
/// from a document rather than merely unlikely.
fn reload_class(section: &str) -> ReloadClass {
    RELOAD_CLASSES
        .iter()
        .find(|(name, _)| *name == section)
        .map_or(ReloadClass::Rollout, |(_, class)| *class)
}

/// Read a cluster document.
///
/// Pure and total in its inputs: `bytes` is the whole document, `identity` is what this node was
/// started as, `env` is the environment `${NAME}` resolves against (§8 V4). No socket, no clock, no
/// filesystem, no second file — §2 D1. Resolving a `dsnRef` into a DSN is *not* done here, because
/// resolution is IO and V9 puts it in the driver.
///
/// Returns every error, ordered by path (§8 V1).
pub fn load(
    bytes: &[u8],
    identity: &NodeIdentity,
    env: &BTreeMap<String, String>,
) -> Result<Config, Vec<ConfigError>> {
    let root = Path::root();
    let mut errors = Vec::new();

    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(utf8) => {
            return Err(vec![ConfigError::new(
                root,
                "CC-D3",
                Some(format!("invalid UTF-8 at byte {}", utf8.valid_up_to())),
                "a UTF-8 encoded YAML or JSON document",
            )]);
        }
    };

    // Substitution happens before typing, so a substituted value is validated exactly like a
    // written one (§8 V4).
    let (substituted, unresolved) = substitute(text, env);

    let document = match parse_document(&substituted) {
        Ok(value) => value,
        Err(why) => {
            report_unresolved(&Value::Null, &root, &unresolved, &mut errors);
            errors.push(ConfigError::new(
                root,
                "CC-D3",
                Some(why),
                "a well-formed YAML, JSON or TOML document",
            ));
            return Err(ordered(errors));
        }
    };
    report_unresolved(&document, &root, &unresolved, &mut errors);

    let config = read_document(&document, identity, &root, &mut errors);

    // V10 makes a refusal total: either a whole config, or none of one. The `None` with no error
    // recorded is unreachable by construction — every early return in the reader pushes first — but
    // it is answered with a refusal rather than a panic, because a loader that panics on a document
    // is a node that crash-loops instead of saying what is wrong with it.
    match config {
        Some(config) if errors.is_empty() => Ok(config),
        _ => {
            if errors.is_empty() {
                errors.push(ConfigError::new(
                    Path::root(),
                    "CC-V10",
                    None,
                    "a document this loader could read to completion",
                ));
            }
            Err(ordered(errors))
        }
    }
}

/// Project a cluster document onto one node (§5 P2).
///
/// Total: everything that could fail has already failed in [`load`]. This only selects.
pub fn project(config: &Config, identity: &NodeIdentity) -> ProjectedConfig {
    let listeners = config
        .listeners
        .iter()
        .filter(|listener| !listener.roles.is_disjoint(&identity.roles))
        .cloned()
        .collect();

    // R5: a section no configured role consumes is projected away rather than carried. The location
    // store is the registrar's (§7), so a node that is not a registrar does not get one — and
    // cannot accidentally act on one.
    let location_store = identity
        .roles
        .contains(&Role::Registrar)
        .then(|| config.location_store.clone())
        .flatten();

    ProjectedConfig {
        identity: identity.clone(),
        version: config.version,
        listeners,
        location_store,
        tenants: config.tenants.clone(),
        security: config.security.clone(),
        admission: config.admission,
        timers: config.timers.clone(),
    }
}

/// What a reload would do, when §9 admits it.
///
/// There is no "restart" variant, and that is RL2: a document changing any `rollout`-class field is
/// **rejected as a reload**, so the alternative to a plan is an error rather than a plan with a flag
/// on it. A `ReloadPlan` in hand means every clause of
/// [cluster-membership](../../../../docs/specs/cluster-membership.md) §6 RD1 holds — no listener
/// rebound, no connection closed, no registration expired, no token or flow reference invalidated,
/// no established dialog or in-flight transaction disturbed — because the sections that could have
/// disturbed any of them are exactly the ones whose change is refused here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadPlan {
    /// The `reloadable` sections whose content differs from the active document, in path order.
    pub changed: Vec<Path>,
}

/// Judge a new document against the active one (§9).
///
/// Two steps, in the order §9.1 states them: the whole new document is validated by [`load`]
/// (RL2 — "validated and then either applied or not"), and only then are the transition rules of
/// §9.2–§9.4 applied, which are judged against the active document and cannot be judged from the new
/// one alone (RL3). At first load there is no predecessor and every transition rule is vacuous,
/// which is why this is a separate entry point rather than an argument to `load`.
///
/// Two rules of §9 are already enforced one layer down and are not repeated here:
/// `cluster-membership` §6 RD3 — a reload that changes *this* node's own `zone` or `roles` — is §5
/// P3's cross-check against the identity, which [`load`] performs on the new document; and RD2's
/// "adding or removing a member is a reload" is the absence of a rule rather than one.
///
/// # Errors
///
/// Every reason the new document cannot replace the active one, ordered by path (§8 V1) — the
/// loader's own refusals, and §9's transition refusals on top of them.
pub fn reload(
    active: &Config,
    bytes: &[u8],
    identity: &NodeIdentity,
    env: &BTreeMap<String, String>,
) -> Result<(Config, ReloadPlan), Vec<ConfigError>> {
    let next = load(bytes, identity, env)?;
    let root = Path::root();
    let cluster = root.field("cluster");
    let mut errors = Vec::new();
    let mut changed = Vec::new();

    // D10. Rolling back is publishing a *new*, higher version whose content is the old one, so that
    // "which configuration is this node running" has one answer.
    if next.version <= active.version {
        errors.push(ConfigError::new(
            root.field("version"),
            "CC-D10",
            Some(next.version.to_string()),
            &format!(
                "a configuration version above the active {}; a rollback is a new, higher version \
                 carrying the old content",
                active.version
            ),
        ));
    }

    // RL1/RL2. The class is read from §7's table, never reached by diffing — the node and the
    // operator must classify a change identically, or the operator pushes a change no node applies.
    let mut sections: BTreeSet<&str> = active.sections.keys().map(String::as_str).collect();
    sections.extend(next.sections.keys().map(String::as_str));
    for section in sections {
        if active.sections.get(section) == next.sections.get(section) {
            continue;
        }
        let at = cluster.field(section);
        match reload_class(section) {
            ReloadClass::Reloadable => changed.push(at),
            ReloadClass::Rollout => {
                for at in rollout_paths(section, active, &next, &at) {
                    errors.push(ConfigError::new(
                        at,
                        "CC-RL2",
                        Some("a rollout-class change".into()),
                        "no change to a rollout-class field; applying one is a restart, staged by \
                         the operator (KO-8), and the node keeps running the active version",
                    ));
                }
            }
        }
    }

    check_id_reassignment(active, &next, &cluster, &mut errors);
    check_key_transition(active, &next, &cluster, &mut errors);

    if errors.is_empty() {
        Ok((next, ReloadPlan { changed }))
    } else {
        Err(ordered(errors))
    }
}

/// Which paths a changed `rollout` section is named by.
///
/// As deep as this loader's own reading goes, and no deeper. `listener[]` and `locationStore` are
/// parsed into typed fields, so a change in one can be named at the field that changed — which is
/// what §12 `CC-D-8` asks for. `profile`, `management`, `registrar` and `mediaPool` are recognised
/// and not descended into, so the section is the most precise true thing that can be said about
/// them; inventing a deeper path would mean a second reading of a schema this module does not own.
fn rollout_paths(section: &str, active: &Config, next: &Config, at: &Path) -> Vec<Path> {
    match section {
        "listener" if active.listeners.len() == next.listeners.len() => {
            let mut paths = Vec::new();
            for (index, (before, after)) in active
                .listeners
                .iter()
                .zip(next.listeners.iter())
                .enumerate()
            {
                let at = at.index(index);
                if before.roles != after.roles {
                    paths.push(at.field("roles"));
                }
                if before.transport != after.transport {
                    paths.push(at.field("transport"));
                }
                if before.bind != after.bind {
                    paths.push(at.field("bind"));
                }
                if before.advertise != after.advertise {
                    paths.push(at.field("advertise"));
                }
            }
            if paths.is_empty() {
                vec![at.clone()]
            } else {
                paths
            }
        }
        "locationStore" => match (&active.location_store, &next.location_store) {
            (Some(before), Some(after)) => {
                let mut paths = Vec::new();
                if before.backend != after.backend {
                    paths.push(at.field("backend"));
                }
                if before.dsn_ref != after.dsn_ref {
                    paths.push(at.field("dsnRef"));
                }
                if paths.is_empty() {
                    vec![at.clone()]
                } else {
                    paths
                }
            }
            _ => vec![at.clone()],
        },
        _ => vec![at.clone()],
    }
}

/// §6 I3, and `cluster-membership` RD4: a `node` id is not re-pointed to a different `name`.
///
/// The loader owns the version-to-version half of I3; §7.2 RB11's `W` wait is the calendar half and
/// is the operator's, because it needs a wall clock the loader does not have (§2 D1). Reusing an id
/// early is indistinguishable, on the wire, from the record it collides with.
fn check_id_reassignment(
    active: &Config,
    next: &Config,
    cluster: &Path,
    errors: &mut Vec<ConfigError>,
) {
    let at = cluster.field("membership");
    for (index, member) in next.membership.iter().enumerate() {
        let Some(previous) = active
            .membership
            .iter()
            .find(|held| held.node == member.node)
        else {
            continue;
        };
        if previous.name != member.name {
            errors.push(ConfigError::new(
                at.index(index).field("node"),
                "CC-I3",
                Some(format!(
                    "id {} re-pointed from \"{}\" to \"{}\"",
                    member.node, previous.name, member.name
                )),
                "an id that still names what it named at the active version; a retired id waits \
                 max(L, E_max) + S before it names something else (cluster-membership §7.2 RB11)",
            ));
        }
    }
}

/// `W`, the overlap window `max(L, E_max) + S` — `cluster-membership` §7.1's table, computed from the
/// document exactly as RB1 says an operator computes it.
///
/// `E_max` is RB1's **largest** `tenant[].expiry` maximum, not one tenant's: a flow reference carries
/// no expiry of its own and leaves circulation only when the binding holding it refreshes, so one
/// tenant raising its ceiling lengthens every rotation for the whole cluster. Taken from the
/// *incoming* document, which is RB5's rule — "the term is the document's largest at the moment of
/// retirement, not at the moment of activation".
///
/// `L` is `affinity-token` §7 M5's 86 400 s. This schema has no field for the token lifetime, so the
/// default is the only value a document can mean; when one is added, this is where it is read, and
/// the `max` is already here waiting for it. `S` is §8 S6's 30 s.
fn overlap_window(next: &Config) -> i64 {
    let e_max = next
        .tenants
        .iter()
        .map(|tenant| i64::from(tenant.policy.max_expires))
        .max()
        .unwrap_or_else(|| i64::from(TenantPolicySpec::default().max_expires));
    TOKEN_LIFETIME_SECONDS.max(e_max) + SKEW_ALLOWANCE_SECONDS
}

/// §9.3 RL10 and RL11 — the two rules that make "no call is disturbed by a key reload" (RL12) true
/// rather than hoped for.
///
/// Both are judged from the *declared* windows, because D1 forbids the loader a clock: RL11's
/// wall-clock half — has `max(L, E_max) + S` actually elapsed — is `cluster-membership` §7.1 RB5's,
/// addressed to an operator.
///
/// RL10 is read literally, including the case where the active document declared no `keys` at all:
/// introducing the section with a minting key is a mint under a key no other node has been given,
/// which is exactly what the rule refuses. So a fleet acquires its first key set by starting on a
/// document that already carries it — the same restart-class posture RB9 takes for emergency
/// retirement, and for the same reason: `load` has no predecessor, so RL3 makes the rule vacuous
/// there.
fn check_key_transition(
    active: &Config,
    next: &Config,
    cluster: &Path,
    errors: &mut Vec<ConfigError>,
) {
    let at = cluster.field("keys");

    // RL10. The error names the key id and both versions, as the rule requires.
    if let Some((index, minting)) = next.keys.iter().enumerate().find(|(_, key)| key.mint)
        && !active.keys.iter().any(|held| held.id == minting.id)
    {
        errors.push(ConfigError::new(
            at.index(index).field("mint"),
            "CC-RL10",
            Some(format!(
                "key id {} minting at version {}, absent from version {}",
                minting.id, next.version, active.version
            )),
            "a mint key already distributed at the active version; flipping mint to a key no peer \
             holds produces records a healthy node cannot verify (affinity-token §6 K1, K3)",
        ));
    }

    // RL11's second half, and the one this loader shipped without: the *incoming* mint key's window
    // must cover the same bound. The first half on its own is satisfiable by a document that retires
    // nothing and still strands every record — a mint key whose window is a minute wide produces
    // tokens that stop verifying long inside `W`, and the next rotation inherits the problem rather
    // than causing it, at which point no reload can be refused for it because the damage is already
    // in circulation.
    //
    // Judged as a *width*, because D1 forbids the loader a clock. `cluster-membership` §7.1 RB2
    // states the rule against the wall — `verifyUntil ≥ t_activate + W` — and the clock-free
    // consequence of it is that the declared window is at least `W` wide, since activation cannot
    // precede publication. That is necessary rather than sufficient, and deliberately so: the
    // sufficient form needs the clock RB5 gives an operator and this function is denied.
    if let Some((index, minting)) = next.keys.iter().enumerate().find(|(_, key)| key.mint) {
        let window = overlap_window(next);
        if minting.verify_until - minting.verify_from < window {
            errors.push(ConfigError::new(
                at.index(index).field("verifyUntil"),
                "CC-RL11",
                Some(format!(
                    "a verify window of {} s for minting key id {}",
                    minting.verify_until - minting.verify_from,
                    minting.id
                )),
                &format!(
                    "a window at least max(L, E_max) + S = {window} s wide; a key mints for as long \
                     as it is the mint key, and every record it mints must still verify that long \
                     afterwards (cluster-membership §7.1 RB2, affinity-token §6 K4)"
                ),
            ));
        }
    }

    // RL11's first half: the outgoing mint key is neither removed nor has its window brought forward
    // while records minted under it can still be presented.
    let Some(outgoing) = active.keys.iter().find(|key| key.mint) else {
        return;
    };
    match next
        .keys
        .iter()
        .enumerate()
        .find(|(_, key)| key.id == outgoing.id)
    {
        None => errors.push(ConfigError::new(
            at,
            "CC-RL11",
            Some(format!(
                "key id {} removed while it was minting",
                outgoing.id
            )),
            "a document that keeps the outgoing mint key verify-valid for max(L, E_max) + S after \
             the flip; retiring it sooner is a rolling restart (cluster-membership §7.1 RB9)",
        )),
        Some((index, incoming)) if incoming.verify_until < outgoing.verify_until => {
            errors.push(ConfigError::new(
                at.index(index).field("verifyUntil"),
                "CC-RL11",
                Some(format!(
                    "the verify window of key id {} brought forward",
                    outgoing.id
                )),
                "a verifyUntil no earlier than the active version's; a key that stops verifying \
                 early strands every record minted under it inside max(L, E_max) + S",
            ));
        }
        Some(_) => {}
    }
}

/// Parse one of the three encodings §2 D3 admits, into the one value tree everything else walks.
///
/// The encoding is **sniffed, not configured and not taken from the file extension**. A document is
/// the same document whatever it is called, and an operator who renames `cluster.yaml` to
/// `cluster.conf` has not changed the configuration. TOML is tried first only when the text cannot be
/// YAML: YAML is the broader grammar and would happily accept a TOML file as a single scalar, which
/// would then fail much later with a confusing message about a mapping.
fn parse_document(text: &str) -> Result<Value, String> {
    // A TOML document's first meaningful line is `key = value` or `[table]`. YAML uses `:` and `-`,
    // and JSON starts `{`. This is a cheap discriminator, and being wrong about it costs only which
    // error message appears — both parsers are tried before anything is refused.
    let looks_like_toml = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .is_some_and(|line| {
            line.starts_with('[') || (line.contains(" = ") && !line.contains(": "))
        });

    if looks_like_toml {
        match toml::from_str::<toml::Value>(text) {
            Ok(value) => return Ok(toml_to_value(value)),
            Err(toml_error) => {
                // Fall through: it only *looked* like TOML. Report the YAML failure if that fails
                // too, since that is the encoding the document more probably meant to be.
                return serde_yaml_ng::from_str(text).map_err(|_| {
                    format!("not valid TOML ({toml_error}) and not valid YAML either")
                });
            }
        }
    }
    serde_yaml_ng::from_str(text).map_err(|yaml_error| match toml::from_str::<toml::Value>(text) {
        Ok(_) => "valid TOML that did not sniff as TOML; please report this".to_owned(),
        Err(_) => yaml_error.to_string(),
    })
}

/// Convert a TOML value into the tree the rules walk.
///
/// Total, and deliberately lossy in exactly one direction: TOML's datetimes become strings, because
/// this schema has no datetime-typed field and inventing one here would be a second place where the
/// document's types are decided.
fn toml_to_value(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => Value::Number(f.into()),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => {
            Value::Sequence(items.into_iter().map(toml_to_value).collect())
        }
        toml::Value::Table(table) => {
            let mut mapping = serde_yaml_ng::Mapping::new();
            for (key, item) in table {
                mapping.insert(Value::String(key), toml_to_value(item));
            }
            Value::Mapping(mapping)
        }
    }
}

// ----------------------------------------------------------------------------- substitution ----

/// Replace every `${NAME}` from `env` (§8 V4), and report back the ones that did not resolve.
///
/// The only substitution there is: no nesting, no defaulting, no arithmetic, no command
/// substitution. An undefined name is **left standing in the text**, deliberately, rather than
/// replaced with the empty string — which would turn `advertise: "${NODE_IP}:5060"` into an
/// unparsable address and report the wrong problem one layer down.
///
/// It is left standing rather than reported here because a textual pass has no path to report *at*,
/// and §12 `CC-D-4` asks for the declaring path rather than the document root: the reference survives
/// into the parsed tree, and [`report_unresolved`] names the field it is sitting in. The two
/// spellings the rule refuses — an undefined name and a name outside `[A-Z_][A-Z0-9_]*` — travel the
/// same way, because both are "this is not a value" and both are read at the same place.
fn substitute(text: &str, env: &BTreeMap<String, String>) -> (String, BTreeSet<String>) {
    let mut out = String::with_capacity(text.len());
    let mut unresolved = BTreeSet::new();
    let mut rest = text;

    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            // An unterminated `${` is not a variable; leave it and let the parser complain about
            // the document it actually is.
            out.push_str(&rest[start..]);
            return (out, unresolved);
        };
        let name = &after[..end];
        match env.get(name) {
            Some(value) if is_var_name(name) => out.push_str(value),
            _ => {
                unresolved.insert(name.to_owned());
                out.push_str("${");
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    (out, unresolved)
}

/// Report every `${NAME}` that did not resolve, at the path it was written at (§8 V4).
///
/// Walking the parsed tree is what buys the path: `advertise: "${NODE_IP}:5060"` with no `NODE_IP`
/// in the environment is reported at `cluster.listener[0].advertise`, so an operator is told which
/// field to look at rather than that something, somewhere, named a variable. A name that survives
/// into no scalar — one written as a mapping *key*, or one in a document that did not parse — is
/// still reported, at the document root, because a rule that only fires where a walk can reach is a
/// rule with a hole in it.
fn report_unresolved(
    document: &Value,
    root: &Path,
    unresolved: &BTreeSet<String>,
    errors: &mut Vec<ConfigError>,
) {
    if unresolved.is_empty() {
        return;
    }
    let mut located: BTreeSet<&str> = BTreeSet::new();
    locate_references(document, root, unresolved, &mut located, errors);
    for name in unresolved {
        if !located.contains(name.as_str()) {
            errors.push(ConfigError::new(
                root.clone(),
                "CC-V4",
                Some(format!("${{{name}}}")),
                &expected_variable(name),
            ));
        }
    }
}

fn expected_variable(name: &str) -> String {
    if is_var_name(name) {
        "a variable defined in the environment passed to load".to_owned()
    } else {
        "a name matching [A-Z_][A-Z0-9_]*".to_owned()
    }
}

fn locate_references<'a>(
    value: &Value,
    path: &Path,
    unresolved: &'a BTreeSet<String>,
    located: &mut BTreeSet<&'a str>,
    errors: &mut Vec<ConfigError>,
) {
    match value {
        Value::String(text) => {
            for name in unresolved {
                if text.contains(&format!("${{{name}}}")) {
                    located.insert(name.as_str());
                    errors.push(ConfigError::new(
                        path.clone(),
                        "CC-V4",
                        Some(format!("${{{name}}}")),
                        &expected_variable(name),
                    ));
                }
            }
        }
        Value::Sequence(items) => {
            for (index, item) in items.iter().enumerate() {
                locate_references(item, &path.index(index), unresolved, located, errors);
            }
        }
        Value::Mapping(map) => {
            for (key, item) in map {
                let at = key
                    .as_str()
                    .map_or_else(|| path.clone(), |key| path.field(key));
                locate_references(item, &at, unresolved, located, errors);
            }
        }
        Value::Tagged(tagged) => {
            locate_references(&tagged.value, path, unresolved, located, errors);
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn is_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

// ------------------------------------------------------------------------------- the reader ----

/// Reject any key at `path` that is not in `known` (§8 V2).
fn closed_world(
    map: &serde_yaml_ng::Mapping,
    known: &[&str],
    path: &Path,
    errors: &mut Vec<ConfigError>,
) {
    for key in map.keys() {
        let Some(name) = key.as_str() else {
            errors.push(ConfigError::new(
                path.clone(),
                "CC-V2",
                Some(format!("{key:?}")),
                "a string key",
            ));
            continue;
        };
        if !known.contains(&name) {
            errors.push(ConfigError::new(
                path.field(name),
                "CC-V2",
                Some(name.to_owned()),
                &format!("one of: {}", known.join(", ")),
            ));
        }
    }
}

fn as_mapping<'a>(
    value: &'a Value,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Option<&'a serde_yaml_ng::Mapping> {
    if let Some(map) = value.as_mapping() {
        return Some(map);
    }
    errors.push(ConfigError::new(
        path.clone(),
        "CC-D3",
        Some(type_of(value).to_owned()),
        "a mapping",
    ));
    None
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a sequence",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}

fn required_str(
    map: &serde_yaml_ng::Mapping,
    key: &str,
    path: &Path,
    rule: &str,
    errors: &mut Vec<ConfigError>,
) -> Option<String> {
    let at = path.field(key);
    match map.get(Value::from(key)) {
        None => {
            errors.push(ConfigError::new(at, rule, None, "a string; it is required"));
            None
        }
        Some(value) => match value.as_str() {
            Some(text) if !text.is_empty() => Some(text.to_owned()),
            Some(_) => {
                errors.push(ConfigError::new(
                    at,
                    rule,
                    Some("\"\"".into()),
                    "a non-empty string",
                ));
                None
            }
            None => {
                errors.push(ConfigError::new(
                    at,
                    rule,
                    Some(type_of(value).to_owned()),
                    "a string",
                ));
                None
            }
        },
    }
}

fn required_uint(
    map: &serde_yaml_ng::Mapping,
    key: &str,
    path: &Path,
    rule: &str,
    max: u64,
    errors: &mut Vec<ConfigError>,
) -> Option<u64> {
    let at = path.field(key);
    match map.get(Value::from(key)) {
        None => {
            errors.push(ConfigError::new(
                at,
                rule,
                None,
                "an integer; it is required",
            ));
            None
        }
        Some(value) => match value.as_u64() {
            Some(number) if number <= max => Some(number),
            Some(number) => {
                errors.push(ConfigError::new(
                    at,
                    rule,
                    Some(number.to_string()),
                    &format!("an integer in 0..={max}"),
                ));
                None
            }
            None => {
                errors.push(ConfigError::new(
                    at,
                    rule,
                    Some(type_of(value).to_owned()),
                    "a non-negative integer",
                ));
                None
            }
        },
    }
}

fn read_roles(value: Option<&Value>, path: &Path, errors: &mut Vec<ConfigError>) -> BTreeSet<Role> {
    let mut roles = BTreeSet::new();
    let Some(value) = value else {
        errors.push(ConfigError::new(
            path.clone(),
            "CC-R2",
            None,
            "a sequence of roles; roles is a set, not a value",
        ));
        return roles;
    };
    let Some(items) = value.as_sequence() else {
        errors.push(ConfigError::new(
            path.clone(),
            "CC-R2",
            Some(type_of(value).to_owned()),
            "a sequence of roles; roles is a set, not a value",
        ));
        return roles;
    };
    for (index, item) in items.iter().enumerate() {
        let at = path.index(index);
        match item.as_str().and_then(Role::parse) {
            Some(role) => {
                roles.insert(role);
            }
            None => errors.push(ConfigError::new(
                at,
                "CC-R1",
                item.as_str()
                    .map(str::to_owned)
                    .or_else(|| Some(type_of(item).to_owned())),
                &format!("one of the closed role set: {}", Role::closed_set()),
            )),
        }
    }
    roles
}

/// R4 and R6: the empty set is refused, and `echo` may not sit beside a call-path role.
fn check_role_combination(roles: &BTreeSet<Role>, path: &Path, errors: &mut Vec<ConfigError>) {
    if roles.is_empty() {
        errors.push(ConfigError::new(
            path.clone(),
            "CC-R4",
            Some("[]".into()),
            "at least one role; a node that runs nothing should not have been started",
        ));
        return;
    }
    for offender in [Role::Echo, Role::E2eTester] {
        if roles.contains(&offender) && roles.iter().any(|role| role.is_call_path()) {
            errors.push(ConfigError::new(
                path.clone(),
                "CC-R6",
                Some(
                    roles
                        .iter()
                        .map(|role| role.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                &format!(
                    "`{}` not combined with edge, registrar, inbound-proxy or outbound-proxy",
                    offender.as_str()
                ),
            ));
        }
    }
}

fn read_document(
    document: &Value,
    identity: &NodeIdentity,
    root: &Path,
    errors: &mut Vec<ConfigError>,
) -> Option<Config> {
    let top = as_mapping(document, root, errors)?;

    // §3 D6, and it runs **before** anything else is looked at: "a node MUST refuse a document
    // naming any other [schema version], naming the versions it does implement. It MUST NOT parse a
    // document it does not fully implement — not on a best-effort basis, not by ignoring what it
    // does not recognise."
    //
    // So a foreign `apiVersion` is not one error among several: everything already accumulated is
    // dropped and this is the only thing reported. What would otherwise come back is a list of
    // closed-world complaints about a schema this build has no opinion on, which reads as "these
    // keys are wrong" when the true statement is "this whole document is somebody else's". A
    // half-understood security posture is worse than a node that will not start.
    let api_version = required_str(top, "apiVersion", root, "CC-D6", errors);
    if let Some(found) = &api_version
        && found != API_VERSION
    {
        errors.clear();
        errors.push(ConfigError::new(
            root.field("apiVersion"),
            "CC-D6",
            Some(found.clone()),
            API_VERSION,
        ));
        return None;
    }

    closed_world(top, &["apiVersion", "version", "cluster"], root, errors);
    let version = required_uint(top, "version", root, "CC-D9", u64::from(u32::MAX), errors);

    let cluster_path = root.field("cluster");
    let cluster_value = top.get(Value::from("cluster"));
    let Some(cluster_value) = cluster_value else {
        errors.push(ConfigError::new(
            cluster_path,
            "CC-2",
            None,
            "the cluster section; it is required",
        ));
        return None;
    };
    let cluster = as_mapping(cluster_value, &cluster_path, errors)?;

    let mut known: Vec<&str> = CLUSTER_KEYS.to_vec();
    known.extend_from_slice(DEFERRED_SECTIONS);
    closed_world(cluster, &known, &cluster_path, errors);

    // Recorded where they are recognised, so the list cannot drift from what the walk actually did.
    let mut unapplied: Vec<Path> = DEFERRED_SECTIONS
        .iter()
        .filter(|section| cluster.contains_key(Value::from(**section)))
        .map(|section| cluster_path.field(section))
        .collect();

    // `management` is intentionally still a deferred section, but V9 is a property of the
    // document and its errors rather than of the eventual TLS consumer. Enforce the one secret
    // invariant we can already prove without pretending to validate the rest of that future block.
    if let Some(management) = cluster
        .get(Value::from("management"))
        .and_then(Value::as_mapping)
        && let Some(tls) = management.get(Value::from("tls"))
    {
        refuse_inline_tls_key(tls, &cluster_path.field("management").field("tls"), errors);
    }

    let name = required_str(cluster, "name", &cluster_path, "CC-7", errors);
    let environment = required_str(cluster, "environment", &cluster_path, "CC-7", errors);
    let zones = read_zones(cluster, &cluster_path, errors);
    let listeners = read_listeners(cluster, &cluster_path, errors, &mut unapplied);
    let membership = read_membership(cluster, &cluster_path, errors, &mut unapplied);
    let keys = read_keys(cluster, &cluster_path, errors, &mut unapplied);
    let shard_map = read_shard_map(cluster, &cluster_path, errors, &mut unapplied);
    let location_store = read_location_store(cluster, &cluster_path, errors, &mut unapplied);
    let tenants = read_tenants(cluster, &cluster_path, errors);
    let security = read_security(cluster, &cluster_path, errors);
    let admission = read_admission(cluster, &cluster_path, errors);
    let timers = read_timers(cluster, &cluster_path, errors, &mut unapplied);

    check_role_combination(
        &identity.roles,
        &Path::root().field("<identity>.roles"),
        errors,
    );
    check_membership_agrees(&membership, identity, &cluster_path, errors);
    check_projection_has_a_listener(&listeners, identity, &cluster_path, errors);
    check_shard_owners(shard_map.as_ref(), &membership, &cluster_path, errors);

    // §8 V1's ordering is over paths, and `unapplied` is read by an operator beside those errors.
    // Sorting it here rather than at each push keeps the two lists in one order without asking every
    // reader to remember to.
    unapplied.sort();
    unapplied.dedup();

    Some(Config {
        api_version: api_version?,
        version: u32::try_from(version?).ok()?,
        name: name?,
        environment: environment?,
        zones,
        listeners,
        membership,
        keys,
        shard_map,
        location_store,
        tenants,
        security,
        admission,
        timers,
        unapplied,
        sections: canonical_sections(cluster),
    })
}

/// Every `cluster` section, canonically rendered, for [`reload`] to diff (§9.1 RL1).
fn canonical_sections(cluster: &serde_yaml_ng::Mapping) -> BTreeMap<String, String> {
    cluster
        .iter()
        .filter_map(|(key, value)| Some((key.as_str()?.to_owned(), canonical(value))))
        .collect()
}

/// One value, rendered so that two documents meaning the same thing render the same bytes.
///
/// Mappings are emitted in key order and numbers are normalised, so a reload that only reordered a
/// section's keys — or spelled `42` in TOML rather than in YAML — is correctly **not** a change.
/// Sequence order is preserved, because in this schema a sequence is a list and `listener[0]` is not
/// `listener[1]`.
fn canonical(value: &Value) -> String {
    match value {
        Value::Null => "~".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number
            .as_u64()
            .map(|n| n.to_string())
            .or_else(|| number.as_i64().map(|n| n.to_string()))
            .or_else(|| number.as_f64().map(|n| n.to_string()))
            .unwrap_or_else(|| number.to_string()),
        Value::String(text) => format!("{text:?}"),
        Value::Sequence(items) => {
            let rendered: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", rendered.join(","))
        }
        Value::Mapping(map) => {
            let mut rendered: Vec<String> = map
                .iter()
                .map(|(key, item)| format!("{}:{}", canonical(key), canonical(item)))
                .collect();
            rendered.sort();
            format!("{{{}}}", rendered.join(","))
        }
        Value::Tagged(tagged) => format!("!{} {}", tagged.tag, canonical(&tagged.value)),
    }
}

fn read_zones(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Vec<String> {
    let at = path.field("zones");
    let Some(value) = cluster.get(Value::from("zones")) else {
        errors.push(ConfigError::new(
            at,
            "CC-7",
            None,
            "a sequence of zone names",
        ));
        return Vec::new();
    };
    let Some(items) = value.as_sequence() else {
        errors.push(ConfigError::new(
            at,
            "CC-7",
            Some(type_of(value).to_owned()),
            "a sequence of zone names",
        ));
        return Vec::new();
    };
    // V8's declared ceiling. Raising it is a change to the spec, never a configuration flag.
    if items.len() > 64 {
        errors.push(ConfigError::new(
            at.clone(),
            "CC-V8",
            Some(items.len().to_string()),
            "at most 64 zones",
        ));
    }
    let mut zones = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item.as_str() {
            // I4: byte-for-byte. No folding, no trimming.
            Some(text) if !text.is_empty() => zones.push(text.to_owned()),
            _ => errors.push(ConfigError::new(
                at.index(index),
                "CC-7",
                Some(type_of(item).to_owned()),
                "a non-empty zone name",
            )),
        }
    }
    zones
}

fn read_listeners(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
    unapplied: &mut Vec<Path>,
) -> Vec<ListenerSpec> {
    let at = path.field("listener");
    let Some(value) = cluster.get(Value::from("listener")) else {
        errors.push(ConfigError::new(
            at,
            "CC-7",
            None,
            "a sequence of listeners; a cluster with none can serve nothing",
        ));
        return Vec::new();
    };
    let Some(items) = value.as_sequence() else {
        errors.push(ConfigError::new(
            at,
            "CC-7",
            Some(type_of(value).to_owned()),
            "a sequence of listeners",
        ));
        return Vec::new();
    };
    let mut listeners = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = at.index(index);
        let Some(map) = as_mapping(item, &at, errors) else {
            continue;
        };
        closed_world(
            map,
            &[
                "roles",
                "transport",
                "bind",
                "advertise",
                "connectionLifetime",
                "maxConnections",
                "tls",
            ],
            &at,
            errors,
        );
        // `tls` is the one that matters here: a listener declaring it and getting cleartext is the
        // defect `FC-1` closed for the *transport* field, and the same key exists one level down.
        for ignored in ["connectionLifetime", "maxConnections", "tls"] {
            if map.contains_key(Value::from(ignored)) {
                unapplied.push(at.field(ignored));
            }
        }
        if let Some(tls) = map.get(Value::from("tls")) {
            // `FC-1` still owns whether this whole block is applied or refused. Redacting a private
            // key written beside `keyRef` does not need to wait for that decision: V9 applies to
            // every document and this targeted check neither accepts nor resolves the TLS block.
            refuse_inline_tls_key(tls, &at.field("tls"), errors);
        }
        let roles = read_roles(map.get(Value::from("roles")), &at.field("roles"), errors);
        let transport = required_str(map, "transport", &at, "CC-7", errors)
            .and_then(|declared| check_transport(&declared, &at.field("transport"), errors));
        let bind = required_str(map, "bind", &at, "CC-7", errors);
        let advertise = map
            .get(Value::from("advertise"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let (Some(transport), Some(bind)) = (transport, bind) {
            listeners.push(ListenerSpec {
                roles,
                transport,
                bind,
                advertise,
            });
        }
    }
    listeners
}

/// Refuse an inline TLS private key without echoing it.
///
/// TLS blocks are not otherwise consumed by this build (`FC-1` owns that work), so this deliberately
/// validates only V9's cross-cutting secret rule. A malformed block remains that story's refusal;
/// a mapping containing the plausible inline neighbour of `keyRef` is already unsafe to print and
/// can be rejected precisely today.
fn refuse_inline_tls_key(value: &Value, at: &Path, errors: &mut Vec<ConfigError>) {
    let Some(tls) = value.as_mapping() else {
        return;
    };
    if tls.contains_key(Value::from("key")) {
        errors.push(ConfigError::new(
            at.field("key"),
            "CC-V9",
            Some("an inline TLS private key".into()),
            "keyRef naming a private key the driver resolves; no secret value appears in the document",
        ));
    }
}

/// Accept only a transport this build can actually serve, and refuse the rest **loudly**.
///
/// This existed as `_ => Udp` for one commit, and it was a fail-open on a security-relevant field: a
/// document declaring `transport: tls` bound plaintext UDP and answered `200 OK`, so a deployment
/// asking for encrypted signalling got none and nothing said so. That is the shape of defect §8 V10
/// exists to prevent — refusing to start is the only failure mode — and it is worse here than
/// elsewhere, because the operator's *intent* was recorded in the document and then discarded.
///
/// `tls`, `ws` and `wss` are named in the refusal rather than treated as unknown, because "this
/// build cannot serve it yet" and "there is no such transport" are different problems and lead an
/// operator to different actions. This follows media-relay §13.2 MP12's precedent: refuse a policy
/// the implementation cannot honour, rather than honour a different one.
fn check_transport(declared: &str, path: &Path, errors: &mut Vec<ConfigError>) -> Option<String> {
    match declared {
        "udp" | "tcp" => Some(declared.to_owned()),
        "tls" | "ws" | "wss" => {
            errors.push(ConfigError::new(
                path.clone(),
                "CC-V10",
                Some(declared.to_owned()),
                "udp or tcp; this build cannot serve tls, ws or wss, and will not silently \
                 substitute cleartext for a transport that was asked for",
            ));
            None
        }
        _ => {
            errors.push(ConfigError::new(
                path.clone(),
                "CC-V2",
                Some(declared.to_owned()),
                "one of: udp, tcp, tls, ws, wss",
            ));
            None
        }
    }
}

/// Read `membership[]` — [cluster-membership](../../../../docs/specs/cluster-membership.md) §3.
///
/// MB1 closes the world on exactly seven fields. Three of them — `rpc`, `incarnationSource` and
/// `incarnationRef` — reach no consumer in this build, so each one a document declares is reported
/// through `unapplied`: `AF-3`/`AF-7` own the endpoint a peer dials and `affinity-token` §12.2 CT2's
/// incarnation is produced by machinery that does not exist here yet. Validating a field is not
/// applying it, and `FC-2` exists because the difference was once invisible.
fn read_membership(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
    unapplied: &mut Vec<Path>,
) -> Vec<MemberSpec> {
    let at = path.field("membership");
    let Some(value) = cluster.get(Value::from("membership")) else {
        // Absent membership is legitimate: §5 P3 says a node with no entry still starts.
        return Vec::new();
    };
    let Some(items) = value.as_sequence() else {
        errors.push(ConfigError::new(
            at,
            "CC-7",
            Some(type_of(value).to_owned()),
            "a sequence of membership entries",
        ));
        return Vec::new();
    };
    let mut members: Vec<MemberSpec> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = at.index(index);
        let Some(map) = as_mapping(item, &at, errors) else {
            continue;
        };
        closed_world(
            map,
            &[
                "node",
                "name",
                "zone",
                "roles",
                "rpc",
                "incarnationSource",
                "incarnationRef",
            ],
            &at,
            errors,
        );
        for declared in ["rpc", "incarnationSource", "incarnationRef"] {
            if map.contains_key(Value::from(declared)) {
                unapplied.push(at.field(declared));
            }
        }
        let node = required_uint(map, "node", &at, "CC-I1", u64::from(u16::MAX), errors);
        let name = required_str(map, "name", &at, "CC-I1", errors);
        let zone = required_str(map, "zone", &at, "CC-I1", errors);
        let roles = read_roles(map.get(Value::from("roles")), &at.field("roles"), errors);
        check_role_combination(&roles, &at.field("roles"), errors);
        let rpc = read_rpc(map, &roles, &at, errors);
        let (incarnation_source, incarnation_ref) = read_incarnation(map, &at, errors);

        let Some(node) = node else { continue };
        // I2: `0` is reserved — affinity-token §3 spells it "none".
        if node == 0 {
            errors.push(ConfigError::new(
                at.field("node"),
                "CC-I2",
                Some("0".into()),
                "a node id of 1 or greater; 0 is reserved for \"none\"",
            ));
        }
        // Already bounded by `required_uint`'s max, so this cannot fail; expressed as a
        // conversion rather than a cast so the bound is checked by the compiler and not by a
        // comment that could drift from it.
        let Ok(node) = u16::try_from(node) else {
            continue;
        };
        // I2: a duplicate is a load error naming both holders. Two nodes sharing an id give two
        // different connections one flow identity — affinity-token §12.2 CT1.
        if let Some(existing) = members.iter().find(|member| member.node == node) {
            errors.push(ConfigError::new(
                at.field("node"),
                "CC-I2",
                Some(node.to_string()),
                &format!("an id not already held by \"{}\"", existing.name),
            ));
        }
        check_member_is_unique(&members, name.as_deref(), rpc.as_deref(), &at, errors);
        if let (Some(name), Some(zone)) = (name, zone) {
            members.push(MemberSpec {
                node,
                name,
                zone,
                roles,
                rpc,
                incarnation_source,
                incarnation_ref,
            });
        }
    }
    members
}

/// MB4 and MB6 — the two uniqueness rules a member carries beyond its id.
///
/// Both are refused naming the other holder, because "this name is taken" without saying by what is
/// a message that sends an operator back to the file to search for it.
fn check_member_is_unique(
    members: &[MemberSpec],
    name: Option<&str>,
    rpc: Option<&str>,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) {
    // MB4: `name` is unique in the document, byte-compared (§6 I4). `shardMap[].owner` resolves by
    // name (SM2), so two members answering to one name would make an ownership assignment ambiguous
    // — which is DS2's "a shard accepting at two nodes" reached through the front door.
    if let Some(declared) = name
        && let Some(existing) = members.iter().find(|member| member.name == declared)
    {
        errors.push(ConfigError::new(
            path.field("name"),
            "CC-MB4",
            Some(declared.to_owned()),
            &format!("a name not already held by node {}", existing.node),
        ));
    }
    // MB6: two members advertising one endpoint is a load error naming both, because affinity-token
    // §13.1 D5 dials the owner a reference names and nothing re-checks that the answer came from it.
    if let Some(declared) = rpc
        && let Some(existing) = members
            .iter()
            .find(|member| member.rpc.as_deref() == Some(declared))
    {
        errors.push(ConfigError::new(
            path.field("rpc"),
            "CC-MB6",
            Some(declared.to_owned()),
            &format!(
                "an endpoint not already advertised by \"{}\"",
                existing.name
            ),
        ));
    }
}

/// Read a member's `rpc` endpoint (MB5, MB6, MB7).
///
/// The form is §5 P7's, **inherited rather than restated**: this calls the same
/// [`Advertised::parse`] the listener's advertised address goes through, so "empty, unspecified or
/// port `0`" has one implementation and cannot drift into two. What MB6 adds on top is that the port
/// is *required* here — an omitted listener port means "the one I bound", and an RPC endpoint has no
/// bound port to fall back on, so a defaulted one would be a port the far side has to guess.
fn read_rpc(
    map: &serde_yaml_ng::Mapping,
    roles: &BTreeSet<Role>,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Option<String> {
    let at = path.field("rpc");
    // MB5's over-approximation, and it is deliberate: the precise property is "this node may accept
    // a connection-oriented transport", which is a listener fact rather than a role fact. Erring
    // safe is priced by affinity-token §11.4 FM6 — a UDP-only edge owns no flows, so the endpoint it
    // is made to declare is one nobody dials.
    let owns_flows = roles.iter().copied().any(Role::is_call_path);
    let Some(value) = map.get(Value::from("rpc")) else {
        if owns_flows {
            errors.push(ConfigError::new(
                at,
                "CC-MB5",
                None,
                "an advertised host:port; a member on the call path owns flows, and a peer with no \
                 endpoint to dial cannot deliver a request toward a client this node owns",
            ));
        }
        return None;
    };
    // Fail-closed in both directions (MB5): an endpoint on a node that owns nothing is a target
    // nobody should reach. `echo` and `e2e-tester` are off the call path (§4 R6).
    if !owns_flows {
        errors.push(ConfigError::new(
            at.clone(),
            "CC-MB5",
            Some("an advertised endpoint".into()),
            "no rpc key: this member's roles are off the call path, so it owns no flow and no peer \
             dials it",
        ));
        return None;
    }
    let Some(text) = value.as_str() else {
        errors.push(ConfigError::new(
            at,
            "CC-MB6",
            Some(type_of(value).to_owned()),
            "a host:port string",
        ));
        return None;
    };
    match Advertised::parse(text) {
        Ok(endpoint) if endpoint.port().is_some() => Some(text.to_owned()),
        Ok(_) => {
            errors.push(ConfigError::new(
                at,
                "CC-MB6",
                Some(text.to_owned()),
                "a host:port; the RPC port is a deployment fact with no default, and a defaulted \
                 one is a port the far side has to guess",
            ));
            None
        }
        Err(why) => {
            errors.push(ConfigError::new(
                at,
                "CC-MB6",
                Some(text.to_owned()),
                &why.to_string(),
            ));
            None
        }
    }
}

/// Read `incarnationSource` and `incarnationRef` (MB8).
///
/// Absent selects `boot-second`, which is `affinity-token` §12.2 CT2's own mechanism and needs no
/// storage. That is not a silent default: omitting the field selects a documented mechanism whose
/// stated limit — a clock that steps backwards across a restart — is written down beside it, and the
/// deployment that cannot rule that out sets `persisted-counter` on the nodes where it matters.
fn read_incarnation(
    map: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> (IncarnationSource, Option<String>) {
    let at = path.field("incarnationSource");
    let mut source = IncarnationSource::default();
    if let Some(value) = map.get(Value::from("incarnationSource")) {
        let Some(declared) = value.as_str().and_then(IncarnationSource::parse) else {
            errors.push(ConfigError::new(
                at,
                "CC-MB8",
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| Some(type_of(value).to_owned())),
                &format!(
                    "one of: {}, {}",
                    IncarnationSource::BootSecond.as_str(),
                    IncarnationSource::PersistedCounter.as_str()
                ),
            ));
            return (IncarnationSource::default(), None);
        };
        source = declared;
    }

    let at = path.field("incarnationRef");
    let reference = map
        .get(Value::from("incarnationRef"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if source == IncarnationSource::PersistedCounter && reference.is_none() {
        errors.push(ConfigError::new(
            at,
            "CC-MB8",
            None,
            "a reference the driver resolves the counter from; a persisted counter with nowhere to \
             persist is a boot-second with extra words",
        ));
    }
    (source, reference)
}

/// §5 P3: the document's membership entry for this node is cross-checked, not obeyed.
fn check_membership_agrees(
    membership: &[MemberSpec],
    identity: &NodeIdentity,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) {
    let Some(entry) = membership
        .iter()
        .find(|member| member.node == identity.node)
    else {
        // Not an error. A node whose pod the operator has not yet published would otherwise be
        // unable to start, and the failure would arrive as a crash loop rather than a mismatch.
        return;
    };
    let at = path.field("membership");
    if entry.zone != identity.zone {
        errors.push(ConfigError::new(
            at.clone(),
            "CC-P3",
            Some(format!("zone \"{}\" in the document", entry.zone)),
            &format!(
                "zone \"{}\", which this node was started with",
                identity.zone
            ),
        ));
    }
    if entry.roles != identity.roles {
        let spell = |roles: &BTreeSet<Role>| {
            roles
                .iter()
                .map(|role| role.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        errors.push(ConfigError::new(
            at,
            "CC-P3",
            Some(format!("roles [{}] in the document", spell(&entry.roles))),
            &format!(
                "roles [{}], which this node was started with",
                spell(&identity.roles)
            ),
        ));
    }
}

/// §5 P4: a projected node must hold at least one listener.
fn check_projection_has_a_listener(
    listeners: &[ListenerSpec],
    identity: &NodeIdentity,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) {
    if identity.roles.is_empty() {
        return; // R4 already reported the real problem; this would be a second voice on it.
    }
    let mine = listeners
        .iter()
        .filter(|listener| !listener.roles.is_disjoint(&identity.roles))
        .count();
    if mine == 0 {
        errors.push(ConfigError::new(
            path.field("listener"),
            "CC-P4",
            Some("0 listeners for this node's roles".into()),
            "at least one listener whose roles intersect this node's",
        ));
    }
}

// ------------------------------------------------ keys and the shard map (cluster-membership) ----

/// Read `keys[]` — [cluster-membership](../../../../docs/specs/cluster-membership.md) §4.
///
/// Absent is valid: a cluster that mints no affinity records is a cluster with no `keys` section,
/// and KY5's "a document in which no entry carries `mint: true` is refused" is a statement about a
/// section that *is* declared. Refusing an absent section would make every document in the tree
/// invalid for a subsystem none of them uses yet.
///
/// Nothing here resolves a `secretRef`: KY2 puts that in the driver because resolution is IO, and
/// the two rules that need the resolved bytes — KY2's partial-key-set refusal and KY8's refusal of
/// the §10 test keys — are start-up rules with no consumer in this build to enforce them for.
fn read_keys(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
    unapplied: &mut Vec<Path>,
) -> Vec<KeySpec> {
    let at = path.field("keys");
    let Some(value) = cluster.get(Value::from("keys")) else {
        return Vec::new();
    };
    // Loaded, validated, and applied by nobody: `AF-4`'s mint/verify library reaches no field of the
    // driver's configuration yet, so a document declaring keys gets no minted token. Reported for
    // the reason `FC-2` exists — the alternative is a deployment that believes it rotated a key.
    unapplied.push(at.clone());

    let Some(items) = value.as_sequence() else {
        errors.push(ConfigError::new(
            at,
            "CC-7",
            Some(type_of(value).to_owned()),
            "a sequence of key entries",
        ));
        return Vec::new();
    };
    // KY6, which is §8 V8's ceiling adopted unchanged.
    if items.len() > MAX_KEYS {
        errors.push(ConfigError::new(
            at.clone(),
            "CC-V8",
            Some(items.len().to_string()),
            &format!(
                "at most {MAX_KEYS} key entries; a document approaching the ceiling is a document \
                 whose rotations are not being retired"
            ),
        ));
    }

    let mut keys: Vec<KeySpec> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = at.index(index);
        let Some(entry) = read_key_entry(item, &at, errors) else {
            continue;
        };
        // affinity-token §6: no two entries share an `id` while both verify windows are open. Ids
        // may wrap over the years; two open windows on one id make key selection ambiguous for
        // exactly the tokens rotation exists to keep verifying.
        if let Some(existing) = keys.iter().find(|key| {
            key.id == entry.id
                && key.verify_from <= entry.verify_until
                && entry.verify_from <= key.verify_until
        }) {
            errors.push(ConfigError::new(
                at.field("id"),
                "CC-KY1",
                Some(entry.id.to_string()),
                &format!(
                    "an id whose verify window does not overlap the one already declared for id {} \
                     (affinity-token §6)",
                    existing.id
                ),
            ));
        }
        keys.push(entry);
    }

    // KY5. `affinity-token` §6 fixes "exactly one at any configuration version"; this spec supplies
    // the default and the fail-closed half, because a cluster that mints nothing Record-Routes
    // nothing and would fail on its first dialog-forming request rather than at load.
    let minting: Vec<u8> = keys
        .iter()
        .filter(|key| key.mint)
        .map(|key| key.id)
        .collect();
    if minting.len() != 1 && !keys.is_empty() {
        let found = if minting.is_empty() {
            "no key marked mint: true".to_owned()
        } else {
            format!("{} keys marked mint: true", minting.len())
        };
        errors.push(ConfigError::new(
            at,
            "CC-KY5",
            Some(found),
            "exactly one minting key at any configuration version (affinity-token §6)",
        ));
    }
    keys
}

/// One `keys[]` entry, or `None` when it is too broken to carry forward (§8 V10's totality applies
/// to the document, not to a single entry: the walk keeps going so V1 can report every fault).
fn read_key_entry(item: &Value, at: &Path, errors: &mut Vec<ConfigError>) -> Option<KeySpec> {
    let map = as_mapping(item, at, errors)?;
    // `secret` sits on the allow-list beside KY1's six, and that is KY3 rather than an oversight: it
    // is *recognised* so that the refusal below is the one an author reads, and not V2's
    // "unrecognised key" arriving first and telling them the opposite. The same shape
    // `read_security` uses for the four controls §7 declares and this build refuses.
    closed_world(
        map,
        &[
            "id",
            "algorithm",
            "secretRef",
            "verifyFrom",
            "verifyUntil",
            "mint",
            "secret",
        ],
        at,
        errors,
    );
    // KY3. `secret` is recognised so that writing it is refused for the *right* reason: V9's "key
    // material is named by reference, never written", not V2's "unrecognised key", which would read
    // as "this schema has no notion of a secret" — the opposite of true.
    //
    // The refusal describes and never echoes. This rule fires exactly when a real secret is sitting
    // in the field, so a message that printed it would be the defect enforcing itself.
    if map.contains_key(Value::from("secret")) {
        errors.push(ConfigError::new(
            at.field("secret"),
            "CC-V9",
            Some("an inline key secret".into()),
            "secretRef naming a secret the driver resolves; no secret value appears in the document",
        ));
        return None;
    }

    // KY1's first attribute, and the one id in this schema for which `0` is legal: a token's key-id
    // byte has no "none" value, so §6 I2's reserved zero does not reach here.
    let id = required_uint(map, "id", at, "CC-KY1", u64::from(u8::MAX), errors);
    let secret_ref = required_str(map, "secretRef", at, "CC-V9", errors);
    let algorithm = read_algorithm(map, at, errors);
    let verify_from = read_instant(map, "verifyFrom", at, errors);
    let verify_until = read_instant(map, "verifyUntil", at, errors);
    let mint = read_bool(map, "mint", at, errors).unwrap_or(false);

    // KY4: both are required, and the window must be non-empty. A window that never opens is an
    // entry that can never verify anything, and naming it at load beats finding it at 03:00.
    if let (Some(from), Some(until)) = (verify_from, verify_until)
        && from >= until
    {
        errors.push(ConfigError::new(
            at.field("verifyUntil"),
            "CC-KY4",
            Some("a window that never opens".into()),
            "a verifyUntil strictly after verifyFrom",
        ));
    }

    let id = u8::try_from(id?).ok()?;
    Some(KeySpec {
        id,
        algorithm,
        secret_ref: secret_ref?,
        verify_from: verify_from?,
        verify_until: verify_until?,
        mint,
    })
}

fn read_algorithm(
    map: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> KeyAlgorithm {
    let at = path.field("algorithm");
    let Some(value) = map.get(Value::from("algorithm")) else {
        // §8 V3: the default is `affinity-token` §4's, adopted here and declared there.
        return KeyAlgorithm::default();
    };
    if let Some(algorithm) = value.as_str().and_then(KeyAlgorithm::parse) {
        return algorithm;
    }
    errors.push(ConfigError::new(
        at,
        "CC-KY1",
        value
            .as_str()
            .map(str::to_owned)
            .or_else(|| Some(type_of(value).to_owned())),
        &format!(
            "one of: {}, {}",
            KeyAlgorithm::ChaCha20Poly1305.as_str(),
            KeyAlgorithm::HmacSha256_96.as_str()
        ),
    ));
    KeyAlgorithm::default()
}

fn read_bool(
    map: &serde_yaml_ng::Mapping,
    key: &str,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Option<bool> {
    let value = map.get(Value::from(key))?;
    if let Some(flag) = value.as_bool() {
        return Some(flag);
    }
    errors.push(ConfigError::new(
        path.field(key),
        "CC-KY1",
        Some(type_of(value).to_owned()),
        "a boolean",
    ));
    None
}

/// Read one of KY4's absolute instants into UNIX seconds.
///
/// **Only the `Z` form** of RFC 3339 §5.6 is accepted. An offset spelling names the same instant and
/// is a second way to write it, and §2 D5's whole argument is that a document is reviewed as a diff:
/// two spellings of one moment make a rotation window harder to read at exactly the moment it
/// matters. Narrower is also fail-closed — a document this refuses is one an operator rewrites, not
/// one a node misreads.
fn read_instant(
    map: &serde_yaml_ng::Mapping,
    key: &str,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Option<i64> {
    let at = path.field(key);
    let Some(value) = map.get(Value::from(key)) else {
        errors.push(ConfigError::new(
            at,
            "CC-KY4",
            None,
            "an RFC 3339 UTC instant such as 2026-07-28T12:00:00Z; it is required, and a relative \
             window is unrepresentable because the loader has no clock",
        ));
        return None;
    };
    let Some(text) = value.as_str() else {
        errors.push(ConfigError::new(
            at,
            "CC-KY4",
            Some(type_of(value).to_owned()),
            "an RFC 3339 UTC instant such as 2026-07-28T12:00:00Z",
        ));
        return None;
    };
    let Some(seconds) = parse_rfc3339_utc(text) else {
        errors.push(ConfigError::new(
            at,
            "CC-KY4",
            Some(text.to_owned()),
            "an RFC 3339 UTC instant such as 2026-07-28T12:00:00Z; UTC is spelled `Z`, because one \
             instant with two spellings is a window nobody can review in a diff",
        ));
        return None;
    };
    Some(seconds)
}

/// Exactly `width` ASCII digits — RFC 3339 §5.6's `4DIGIT` and `2DIGIT`, read as it writes them.
///
/// `str::parse::<i64>` is deliberately not used for a field of this grammar. It accepts a leading
/// `+` or `-` and any number of digits, which is how `+2026-07-28T12:00:00Z`, `2026-7-8T1:2:3Z` and
/// `2026-07-28T-1:-5:-9Z` all became instants this loader accepted — the last of them a **different**
/// instant from the one written, which is a verify window an operator cannot review in a diff
/// because the document does not say what the node read.
fn fixed_digits(text: &str, width: usize) -> Option<i64> {
    if text.len() != width || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `YYYY-MM-DDTHH:MM:SS[.frac]Z` → UNIX seconds, or `None` for anything else.
///
/// Written out rather than taken from a date library because the whole of what this schema needs
/// from one is this function, and a dependency is a thing a release has to carry. The civil-days
/// arithmetic is the standard proleptic-Gregorian one; the fractional part is checked and discarded,
/// since `affinity-token` §8 S2 compares whole seconds.
///
/// **Total, and checked twice over.** `load` documents itself pure and total in its inputs, §8 V10
/// makes refusing the document the only failure mode there is, and this function reads a field an
/// operator writes. The grammar is what bounds the arithmetic — a `4DIGIT` year cannot reach
/// `i64`'s range — and the arithmetic is checked anyway, because a totality resting on an argument
/// about the grammar is one that a later widening of the grammar removes without anyone noticing.
/// Under `-O` the alternative to a checked multiply is not a panic but a **wrapped** instant, and a
/// rotation rule judged against one is a safety rule that has been switched off silently.
fn parse_rfc3339_utc(text: &str) -> Option<i64> {
    let body = text.strip_suffix('Z').or_else(|| text.strip_suffix('z'))?;
    let (date, time) = body.split_once('T').or_else(|| body.split_once('t'))?;

    let mut date_parts = date.split('-');
    let year = fixed_digits(date_parts.next()?, 4)?;
    let month = fixed_digits(date_parts.next()?, 2)?;
    let day = fixed_digits(date_parts.next()?, 2)?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    if day < 1 || day > days_in_month(year, month) {
        return None;
    }

    // `time-secfrac` is `"." 1*DIGIT`. Present means at least one digit and nothing else: a fraction
    // that is not a fraction makes the whole instant unreadable, not a readable instant with noise
    // after it. `.abc`, `.` and `.1.2` were all discarded silently before.
    let (time, fraction) = time.split_once('.').unwrap_or((time, "0"));
    if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut time_parts = time.split(':');
    let hour = fixed_digits(time_parts.next()?, 2)?;
    let minute = fixed_digits(time_parts.next()?, 2)?;
    let second = fixed_digits(time_parts.next()?, 2)?;
    // The leap second is `60`, which RFC 3339 §5.6 admits and this arithmetic maps onto the next
    // second — a whole-second comparison is all §8 S2 asks for.
    if time_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    days_from_civil(year, month, day)
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + minute * 60 + second)
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days from 1970-01-01 to `year-month-day` in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Read `shardMap` — [cluster-membership](../../../../docs/specs/cluster-membership.md) §5.
fn read_shard_map(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
    unapplied: &mut Vec<Path>,
) -> Option<ShardMapSpec> {
    let at = path.field("shardMap");
    let value = cluster.get(Value::from("shardMap"))?;
    // Validated here; the handoff that would *use* it — the two maps held at once, the drain, the
    // switch — is `RG-5`'s (§9.4 DS7), so no node acts on an assignment yet.
    unapplied.push(at.clone());

    let map = as_mapping(value, &at, errors)?;
    closed_world(map, &["drainTimeout", "shards"], &at, errors);

    let drain_timeout_ms = read_drain_timeout(map, &at, errors);
    let shards = read_shards(map, &at, errors);
    Some(ShardMapSpec {
        drain_timeout_ms,
        shards,
    })
}

/// `drainTimeout`, in DS4's own range and DS4's own default.
///
/// A duration with its unit, not a bare number: every other duration in this schema is milliseconds
/// (§8 V7's timers) and this one is stated in seconds, so an unsuffixed `30` would be two plausible
/// values a thousand-fold apart. Refusing it names the form instead of guessing.
fn read_drain_timeout(
    map: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> u64 {
    let at = path.field("drainTimeout");
    let Some(value) = map.get(Value::from("drainTimeout")) else {
        return DEFAULT_DRAIN_TIMEOUT_MS;
    };
    let declared = value
        .as_str()
        .and_then(|text| text.strip_suffix('s'))
        .and_then(|seconds| seconds.parse::<u64>().ok())
        .and_then(|seconds| seconds.checked_mul(1_000));
    let Some(declared) = declared else {
        errors.push(ConfigError::new(
            at,
            "CC-DS4",
            value
                .as_str()
                .map(str::to_owned)
                .or_else(|| Some(type_of(value).to_owned())),
            "a duration in whole seconds, spelled with its unit — `30s`",
        ));
        return DEFAULT_DRAIN_TIMEOUT_MS;
    };
    if !(MIN_DRAIN_TIMEOUT_MS..=MAX_DRAIN_TIMEOUT_MS).contains(&declared) {
        errors.push(ConfigError::new(
            at,
            "CC-DS4",
            Some(format!("{}s", declared / 1_000)),
            &format!(
                "a drain timeout between {}s and {}s; below the floor a drain would expire while an \
                 ordinary contended write was still legitimately retrying (location-service §5.1 \
                 S10)",
                MIN_DRAIN_TIMEOUT_MS / 1_000,
                MAX_DRAIN_TIMEOUT_MS / 1_000
            ),
        ));
        return DEFAULT_DRAIN_TIMEOUT_MS;
    }
    declared
}

/// The shard list, and SM1's totality.
fn read_shards(
    map: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Vec<ShardSpec> {
    let at = path.field("shards");
    let Some(value) = map.get(Value::from("shards")) else {
        errors.push(ConfigError::new(
            at,
            "CC-SM1",
            None,
            "a shard list; the list is the shard space, and a shardMap without one assigns nothing",
        ));
        return Vec::new();
    };
    let Some(items) = value.as_sequence() else {
        errors.push(ConfigError::new(
            at,
            "CC-SM1",
            Some(type_of(value).to_owned()),
            "a sequence of shards",
        ));
        return Vec::new();
    };

    let mut shards: Vec<ShardSpec> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = at.index(index);
        let Some(entry) = as_mapping(item, &at, errors) else {
            continue;
        };
        closed_world(entry, &["id", "owner"], &at, errors);
        let id = required_uint(entry, "id", &at, "CC-I1", u64::from(u16::MAX), errors);
        let owner = required_str(entry, "owner", &at, "CC-SM2", errors);
        let (Some(id), Some(owner)) = (id, owner) else {
            continue;
        };
        // §6 I2: `0` is reserved — affinity-token §3 spells a shard's zero "none".
        if id == 0 {
            errors.push(ConfigError::new(
                at.field("id"),
                "CC-I2",
                Some("0".into()),
                "a shard id of 1 or greater; 0 is reserved for \"none\"",
            ));
            continue;
        }
        let Ok(id) = u16::try_from(id) else { continue };
        shards.push(ShardSpec { id, owner });
    }

    // SM1: the ids are `1..=N` with no gap and no repeat, and `N` is the length of the list — there
    // is no separate count, because a count and a list are two spellings that can disagree. A
    // missing id is a slice of the registration key space no REGISTER can be accepted for, and it
    // would surface as a tenant's phones going quiet rather than as a configuration error.
    if !shards.is_empty() {
        let declared: BTreeSet<u16> = shards.iter().map(|shard| shard.id).collect();
        let total = u16::try_from(shards.len()).unwrap_or(u16::MAX);
        let missing: Vec<String> = (1..=total)
            .filter(|id| !declared.contains(id))
            .map(|id| id.to_string())
            .collect();
        if !missing.is_empty() {
            errors.push(ConfigError::new(
                at,
                "CC-SM1",
                Some(format!("no owner for shard {}", missing.join(", "))),
                "a total map: ids 1..=N with no gap and no repeat, where N is the length of the list",
            ));
        }
    }
    shards
}

/// SM2 and SM3, which are §8 V5 cross-section rules: they need `membership` and `shardMap` at once.
fn check_shard_owners(
    shard_map: Option<&ShardMapSpec>,
    membership: &[MemberSpec],
    path: &Path,
    errors: &mut Vec<ConfigError>,
) {
    let Some(shard_map) = shard_map else { return };
    let at = path.field("shardMap").field("shards");
    for (index, shard) in shard_map.shards.iter().enumerate() {
        let at = at.index(index).field("owner");
        let Some(owner) = membership.iter().find(|member| member.name == shard.owner) else {
            // SM2. §8 V5's own example, and the rule id is this schema's because the cross-section
            // check is this schema's.
            errors.push(ConfigError::new(
                at,
                "CC-V5",
                Some(shard.owner.clone()),
                "a name declared in cluster.membership; a shard owned by nobody is a slice of the \
                 registration key space with nowhere to write",
            ));
            continue;
        };
        // SM3: a shard owns registration state, so assigning one to a node that runs no registrar
        // leaves its writes with nowhere to land.
        if !owner.roles.contains(&Role::Registrar) {
            errors.push(ConfigError::new(
                at,
                "CC-SM3",
                Some(shard.owner.clone()),
                "a member whose roles include registrar",
            ));
        }
    }
}

fn read_location_store(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
    unapplied: &mut Vec<Path>,
) -> Option<LocationStoreSpec> {
    let at = path.field("locationStore");
    let value = cluster.get(Value::from("locationStore"))?;
    let map = as_mapping(value, &at, errors)?;
    closed_world(map, &["backend", "dsnRef", "ha"], &at, errors);
    // Recognised by §7, held by no field of `LocationStoreSpec` and consulted by no driver, so it is
    // reported rather than dropped — the same reason `listener[].tls` is, one section over.
    if map.contains_key(Value::from("ha")) {
        unapplied.push(at.field("ha"));
    }

    // V9: the value never appears in the document, only a reference to it.
    if map.contains_key(Value::from("dsn")) {
        errors.push(ConfigError::new(
            at.field("dsn"),
            "CC-V9",
            Some("an inline DSN".into()),
            "dsnRef naming a secret the driver resolves; no secret value appears in the document",
        ));
    }
    let backend = required_str(map, "backend", &at, "CC-7", errors)?;
    let dsn_ref = map
        .get(Value::from("dsnRef"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if backend != "memory" && dsn_ref.is_none() {
        errors.push(ConfigError::new(
            at.field("dsnRef"),
            "CC-V9",
            None,
            "a dsnRef; a store that is not in-process needs one",
        ));
    }
    Some(LocationStoreSpec { backend, dsn_ref })
}

fn read_tenants(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Vec<TenantSpec> {
    let at = path.field("tenant");
    let Some(value) = cluster.get(Value::from("tenant")) else {
        return Vec::new();
    };
    let Some(items) = value.as_sequence() else {
        errors.push(ConfigError::new(
            at,
            "CC-7",
            Some(type_of(value).to_owned()),
            "a sequence of tenants",
        ));
        return Vec::new();
    };
    let mut tenants: Vec<TenantSpec> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = at.index(index);
        let Some(map) = as_mapping(item, &at, errors) else {
            continue;
        };
        closed_world(
            map,
            &[
                "name",
                "id",
                "domains",
                "auth",
                "expiry",
                "maxBindingsPerAor",
            ],
            &at,
            errors,
        );
        // Every key of `tenant[]` is applied since `FC-4`, so none is reported here. `FC-2`'s
        // warning must not lie in either direction: a key it names is genuinely ignored, and a key it
        // omits is genuinely applied. Parsing a value into a struct field is *not* applying it —
        // `domains` sat in `TenantSpec` unread for a release, which is exactly that mistake.
        let name = required_str(map, "name", &at, "CC-I1", errors);
        let id = required_uint(map, "id", &at, "CC-I1", u64::from(u32::MAX), errors);
        let auth = read_auth(map, &at, errors);
        let policy = read_tenant_policy(map, &at, errors);
        let domains = map
            .get(Value::from("domains"))
            .and_then(Value::as_sequence)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        let Some(id) = id else { continue };
        if id == 0 {
            errors.push(ConfigError::new(
                at.field("id"),
                "CC-I2",
                Some("0".into()),
                "a tenant id of 1 or greater; 0 is reserved for \"none/system\"",
            ));
        }
        let Ok(id) = u32::try_from(id) else { continue };
        if let Some(existing) = tenants.iter().find(|tenant| tenant.id == id) {
            errors.push(ConfigError::new(
                at.field("id"),
                "CC-I2",
                Some(id.to_string()),
                &format!("an id not already held by \"{}\"", existing.name),
            ));
        }
        if let Some(name) = name {
            if let Some(existing) = tenants.iter().find(|tenant| tenant.name == name) {
                errors.push(ConfigError::new(
                    at.field("name"),
                    "CC-I2",
                    Some(name.clone()),
                    &format!("a name not already held by id {}", existing.id),
                ));
            }
            tenants.push(TenantSpec {
                name,
                id,
                domains,
                policy,
                auth,
            });
        }
    }
    tenants
}

/// Parse a tenant's `auth` block.
///
/// A block that carries user credentials is **refused**: `RG-7` owns where credentials come from,
/// and a document that says "authenticate, against these" cannot be honoured until that lands. Saying
/// so is better than accepting it and authenticating nobody. The minimal block — `realm` and
/// `secretRef` — is what this build can apply.
fn read_auth(
    map: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> Option<AuthSpec> {
    let at = path.field("auth");
    let value = map.get(Value::from("auth"))?;
    let block = as_mapping(value, &at, errors)?;
    closed_world(block, &["realm", "secretRef", "algorithm"], &at, errors);

    if block.contains_key(Value::from("credentials")) || block.contains_key(Value::from("users")) {
        errors.push(ConfigError::new(
            at.field("credentials"),
            "CC-V9",
            Some("credentials declared in the document".into()),
            "no credentials here — where they come from is RG-7's, and a document that tries to \
             supply them cannot be honoured yet",
        ));
        return None;
    }

    let realm = required_str(block, "realm", &at, "CC-RA-A3", errors);
    let secret_ref = required_str(block, "secretRef", &at, "CC-V9", errors);

    // The nonce secret is the one credential the document is allowed to *name*. An inline value is
    // refused by V9, which forbids any secret in the document — a field named `secret` here would
    // otherwise be that exact mistake with a plausible spelling.
    if block.contains_key(Value::from("secret")) {
        errors.push(ConfigError::new(
            at.field("secret"),
            "CC-V9",
            Some("an inline nonce secret".into()),
            "secretRef naming a secret the driver resolves; no secret value appears in the document",
        ));
        return None;
    }

    if let (Some(realm), Some(secret_ref)) = (realm, secret_ref) {
        Some(AuthSpec { realm, secret_ref })
    } else {
        None
    }
}

/// Parse `tenant[].expiry` and `tenant[].maxBindingsPerAor` (`FC-4`).
///
/// Absent keys keep location-service's own defaults; §8 V3 forbids restating a different one. A
/// minimum above the maximum is refused rather than silently reordered — an operator who wrote it
/// meant something, and guessing which half is a policy this schema does not get to invent.
fn read_tenant_policy(
    map: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> TenantPolicySpec {
    let mut policy = TenantPolicySpec::default();

    if let Some(quota) = map.get(Value::from("maxBindingsPerAor")) {
        match quota.as_u64() {
            Some(0) => errors.push(ConfigError::new(
                path.field("maxBindingsPerAor"),
                "CC-S2",
                Some("0".into()),
                "at least 1; a quota of zero is a tenant that can register nothing, which is a \
                 disabled tenant spelled as a limit",
            )),
            Some(value) => {
                policy.max_bindings_per_aor = usize::try_from(value).unwrap_or(usize::MAX);
            }
            None => errors.push(ConfigError::new(
                path.field("maxBindingsPerAor"),
                "CC-S2",
                Some(type_of(quota).to_owned()),
                "a positive integer",
            )),
        }
    }

    let at = path.field("expiry");
    if let Some(value) = map.get(Value::from("expiry"))
        && let Some(block) = as_mapping(value, &at, errors)
    {
        closed_world(block, &["default", "min", "max"], &at, errors);
        for (key, slot) in [
            ("default", &mut policy.default_expires),
            ("min", &mut policy.min_expires),
            ("max", &mut policy.max_expires),
        ] {
            if let Some(found) = block.get(Value::from(key)) {
                match found.as_u64().and_then(|n| u32::try_from(n).ok()) {
                    Some(seconds) => *slot = seconds,
                    None => errors.push(ConfigError::new(
                        at.field(key),
                        "CC-S2",
                        Some(type_of(found).to_owned()),
                        "a duration in seconds",
                    )),
                }
            }
        }
        if policy.min_expires > policy.max_expires {
            errors.push(ConfigError::new(
                at.clone(),
                "CC-S2",
                Some(format!(
                    "min {} above max {}",
                    policy.min_expires, policy.max_expires
                )),
                "a minimum at or below the maximum; which of the two was meant is not this \
                     schema's to guess",
            ));
        }
    }
    policy
}

/// Read `cluster.security` (`FC-6`).
///
/// Absent or empty is valid and means the fixed Max-Forwards of §8 V6, which is the whole of what this
/// build applies from the section. Every other key §7 declares here is refused: see
/// [`UNAPPLIED_SECURITY_CONTROLS`] for why the refusal is per control, and [`SecuritySpec`] for what
/// the old shape did instead.
///
/// The four names stay on the closed-world allow-list rather than being dropped from it, and that is a
/// deliberate difference from "there is no such key". They *are* §7's declared keys, so a document
/// spelling `unknownSorce` should be told the recognised set (V2) and a document spelling
/// `unknownSource` should be told that nothing applies it — two different problems leading an operator
/// to two different actions, exactly as `check_transport` separates `tls` from a transport that does
/// not exist. What the allow-list must not do any more is *end* there: the mechanism the epic's design
/// calls the third state — accepted, on a list, consumed by nobody — is what made V2's own error
/// message evidence that these keys were applied.
fn read_security(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> SecuritySpec {
    let at = path.field("security");
    let Some(value) = cluster.get(Value::from("security")) else {
        return SecuritySpec::default();
    };
    let Some(map) = as_mapping(value, &at, errors) else {
        return SecuritySpec::default();
    };
    closed_world(
        map,
        &[
            "unknownSource",
            "sanityCheck",
            "userAgentDenyList",
            "internalZone",
        ],
        &at,
        errors,
    );
    // V6: where an RFC fixes a value, the schema does not offer a knob. `maxForwards` is not in the
    // recognised set above, so a document setting it is already a closed-world error — but the
    // generic "unknown key" message would teach the wrong lesson, so it gets its own refusal.
    if map.contains_key(Value::from("maxForwards")) {
        errors.push(ConfigError::new(
            at.field("maxForwards"),
            "CC-V6",
            Some("a maxForwards setting".into()),
            &format!(
                "no maxForwards key; RFC 3261 §16.6 step 3 fixes it at {MAX_FORWARDS} and it is not \
                 a hop budget to be tuned downward"
            ),
        ));
    }
    // One error per declared control, so an operator who declared three is told about three (§8 V1).
    // The value is **described, never quoted**: `FC-8` owns writing that rule down, and a refusal that
    // echoed what was written would be a new instance of the defect it is filed for.
    for (key, decides) in UNAPPLIED_SECURITY_CONTROLS {
        if map.contains_key(Value::from(*key)) {
            errors.push(ConfigError::new(
                at.field(key),
                "CC-V10",
                Some(format!("a declared {key} policy")),
                &format!(
                    "no {key} key until a consumer applies it: nothing in this build decides {decides}, \
                     so a node started with this declared would serve the posture the key was written \
                     to narrow. Remove it, or wait for the story that specifies the control"
                ),
            ));
        }
    }
    SecuritySpec::default()
}

/// Read the admission bound (`DP-11`).
///
/// Absent is the declared default, not zero: a node whose document says nothing about overload is a
/// node with the default bound, and a bound of zero would be a node that answers `503` to every call.
/// Which is why zero is refused rather than accepted — the value that turns the feature off is not a
/// smaller limit, it is an outage, and §8 V10's posture is to refuse rather than to honour it.
fn read_admission(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> AdmissionSpec {
    let at = path.field("admission");
    let mut admission = AdmissionSpec::default();
    let Some(value) = cluster.get(Value::from("admission")) else {
        return admission;
    };
    let Some(map) = as_mapping(value, &at, errors) else {
        return admission;
    };
    closed_world(map, &["maxInFlightTransactions"], &at, errors);

    let key = "maxInFlightTransactions";
    if let Some(value) = map.get(Value::from(key)) {
        let ceiling = u64::try_from(MAX_IN_FLIGHT_CEILING).unwrap_or(u64::MAX);
        match value.as_u64() {
            Some(declared) if declared >= 1 && declared <= ceiling => {
                // Bounded by the check above, so the conversion cannot fail on any target this
                // builds for; expressed as a conversion rather than a cast so the bound is the
                // compiler's business and not a comment's.
                if let Ok(declared) = usize::try_from(declared) {
                    admission.max_in_flight_transactions = declared;
                }
            }
            Some(declared) => errors.push(ConfigError::new(
                at.field(key),
                "CC-V8",
                Some(declared.to_string()),
                &format!(
                    "an integer in 1..={MAX_IN_FLIGHT_CEILING}; 0 is a node that answers 503 to \
                     every call, and there is no way to spell \"no bound\""
                ),
            )),
            None => errors.push(ConfigError::new(
                at.field(key),
                "CC-V8",
                Some(type_of(value).to_owned()),
                "a count of concurrent transactions",
            )),
        }
    }
    admission
}

fn read_timers(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
    unapplied: &mut Vec<Path>,
) -> TimersSpec {
    let at = path.field("timers");
    let mut timers = TimersSpec::default();
    if let Some(value) = cluster.get(Value::from("timers"))
        && let Some(map) = as_mapping(value, &at, errors)
    {
        closed_world(
            map,
            &["t1", "timerB", "timerC", "timerF", "maxCallDuration"],
            &at,
            errors,
        );
        // Of the five keys §7 declares, exactly one reaches a driver: `timerC`, which `PX-10` wired
        // through `NodeConfig::timer_c` onto the proxy engine's `ProxyConfig`. The other four are
        // reported rather than dropped, because accepted-and-silently-discarded is the class `FC-2`
        // added `unapplied` to eliminate — and `DP-12` found `maxCallDuration` alone on this list
        // while the whole section was in fact unread, which understated it by four keys.
        //
        // `t1`, `timerB` and `timerF` are the **kernel's** transaction timer constants (RFC 3261
        // §17.1.1.2, §17.1.2.2). Applying them means handing them to `sipx_transport::Config::timers`
        // when the endpoint is built, and this build never sets that field, so a document naming
        // them gets the kernel's own constants. `maxCallDuration` is a session cap, has no field on
        // `TimersSpec` at all, and is not Timer C — conflating the two produces a Timer C set to
        // hours in the belief that it protects long calls, and then the wrong knob is the one tuned.
        for ignored in ["t1", "timerB", "timerF", "maxCallDuration"] {
            if map.contains_key(Value::from(ignored)) {
                unapplied.push(at.field(ignored));
            }
        }

        let mut read_ms = |key: &str, slot: &mut u64| {
            if let Some(value) = map.get(Value::from(key)) {
                match value.as_u64() {
                    Some(ms) => *slot = ms,
                    None => errors.push(ConfigError::new(
                        at.field(key),
                        "CC-V7",
                        Some(type_of(value).to_owned()),
                        "a duration in milliseconds",
                    )),
                }
            }
        };
        read_ms("t1", &mut timers.t1_ms);
        read_ms("timerB", &mut timers.timer_b_ms);
        read_ms("timerF", &mut timers.timer_f_ms);
        read_ms("timerC", &mut timers.timer_c_ms);
    }

    // V7: Timer C MUST be strictly greater than 3 minutes (RFC 3261 §16.6 step 11). Checked against
    // whatever value stands — written or defaulted — and not only on the path where the document
    // said something, because skipping the defaulted path is what let §8 V7 declare a default of
    // exactly 180 s for a release: the contradiction was invisible until a document happened to
    // carry a `timers` section, and then it refused the loader's own value (`DP-12`).
    //
    // Note this is *not* `maxCallDuration`, which is a session cap; conflating them produces a
    // Timer C set to hours in the belief that it protects long calls, and then the wrong knob is the
    // one that gets tuned.
    if timers.timer_c_ms <= TIMER_C_FLOOR_MS {
        errors.push(ConfigError::new(
            at.field("timerC"),
            "CC-V7",
            Some(format!("{} ms", timers.timer_c_ms)),
            &format!("greater than {TIMER_C_FLOOR_MS} ms (3 minutes), per RFC 3261 §16.6 step 11"),
        ));
    }
    timers
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
