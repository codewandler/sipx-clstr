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
//! `membership`, `locationStore`, `tenant[]`, `security` and `timers`.
//!
//! The remaining sections of §7's registry are **recognised but not descended into**: naming one is
//! not an error, and a typo in its name still is. That boundary is deliberate and is reported by
//! [`Config::unapplied`] rather than left for a reader to infer — a section this loader silently
//! ignored would be configuration nobody is applying and nothing anywhere saying so, which is the
//! exact failure V2 exists to prevent, one level up.

pub mod error;

use std::collections::{BTreeMap, BTreeSet};

use error::ordered;
pub use error::{ConfigError, Path, RuleId};

use serde_yaml_ng::Value;

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

/// One node's entry in the cluster's membership (§5 P3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSpec {
    pub node: u16,
    pub name: String,
    pub zone: String,
    pub roles: BTreeSet<Role>,
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
    /// The domains this tenant serves (location-service §5.1 S1). **Enforced** since `FC-4`: a
    /// `REGISTER` for an address-of-record outside this list is refused. Empty means "any", which is
    /// the only backward-compatible reading of a document that declares none.
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
const DEFERRED_SECTIONS: &[&str] = &[
    "profile",
    "management",
    "keys",
    "shardMap",
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
    "locationStore",
    "tenant",
    "security",
    "admission",
    "timers",
];

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
    let substituted = substitute(text, env, &root, &mut errors);

    let document = match parse_document(&substituted) {
        Ok(value) => value,
        Err(why) => {
            errors.push(ConfigError::new(
                root,
                "CC-D3",
                Some(why),
                "a well-formed YAML, JSON or TOML document",
            ));
            return Err(ordered(errors));
        }
    };

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

/// Replace every `${NAME}` from `env` (§8 V4).
///
/// The only substitution there is: no nesting, no defaulting, no arithmetic, no command
/// substitution. An undefined name is an error naming the variable and the path — deliberately not
/// the empty string, which would turn `advertise: "${NODE_IP}:5060"` into an unparsable address and
/// report the wrong problem one layer down.
fn substitute(
    text: &str,
    env: &BTreeMap<String, String>,
    path: &Path,
    errors: &mut Vec<ConfigError>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            // An unterminated `${` is not a variable; leave it and let the parser complain about
            // the document it actually is.
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        if is_var_name(name) {
            match env.get(name) {
                Some(value) => out.push_str(value),
                None => errors.push(ConfigError::new(
                    path.clone(),
                    "CC-V4",
                    Some(format!("${{{name}}}")),
                    "a variable defined in the environment passed to load",
                )),
            }
        } else {
            errors.push(ConfigError::new(
                path.clone(),
                "CC-V4",
                Some(format!("${{{name}}}")),
                "a name matching [A-Z_][A-Z0-9_]*",
            ));
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
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
    closed_world(top, &["apiVersion", "version", "cluster"], root, errors);

    let api_version = required_str(top, "apiVersion", root, "CC-3", errors);
    if let Some(found) = &api_version
        && found != API_VERSION
    {
        errors.push(ConfigError::new(
            root.field("apiVersion"),
            "CC-3",
            Some(found.clone()),
            API_VERSION,
        ));
    }
    let version = required_uint(top, "version", root, "CC-3", u64::from(u32::MAX), errors);

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

    let name = required_str(cluster, "name", &cluster_path, "CC-7", errors);
    let environment = required_str(cluster, "environment", &cluster_path, "CC-7", errors);
    let zones = read_zones(cluster, &cluster_path, errors);
    let listeners = read_listeners(cluster, &cluster_path, errors, &mut unapplied);
    let membership = read_membership(cluster, &cluster_path, errors);
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

    Some(Config {
        api_version: api_version?,
        version: u32::try_from(version?).ok()?,
        name: name?,
        environment: environment?,
        zones,
        listeners,
        membership,
        location_store,
        tenants,
        security,
        admission,
        timers,
        unapplied,
    })
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

fn read_membership(
    cluster: &serde_yaml_ng::Mapping,
    path: &Path,
    errors: &mut Vec<ConfigError>,
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
        closed_world(map, &["node", "name", "zone", "roles"], &at, errors);
        let node = required_uint(map, "node", &at, "CC-I1", u64::from(u16::MAX), errors);
        let name = required_str(map, "name", &at, "CC-I1", errors);
        let zone = required_str(map, "zone", &at, "CC-I1", errors);
        let roles = read_roles(map.get(Value::from("roles")), &at.field("roles"), errors);
        check_role_combination(&roles, &at.field("roles"), errors);

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
        if let (Some(name), Some(zone)) = (name, zone) {
            members.push(MemberSpec {
                node,
                name,
                zone,
                roles,
            });
        }
    }
    members
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
