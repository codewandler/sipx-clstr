//! From a cluster document to a running node (`DP-10`).
//!
//! [`crate::config`] reads a document and projects it onto one node; [`crate::driver`] runs a
//! [`NodeConfig`]. This is the seam between them, and it is the layer where IO is allowed: reading
//! the file, reading the environment, and resolving the references the document deliberately does
//! not contain.
//!
//! `cluster-config` §8 V9 is the reason this is a separate step rather than part of the loader. The
//! document names secrets by reference — `dsnRef`, `secretRef`, `keyRef` — and resolving one is IO,
//! so the loader stays pure and the resolution happens here.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use sipx_transport::TransportKind;

use crate::config::{self, Config, NodeIdentity, ProjectedConfig, Role};
use crate::driver::{NodeConfig, StoreChoice};
use crate::listen::{Advertised, Listener, Listeners};

/// What stops a node from starting before the driver ever runs.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// The document could not be read from disk.
    #[error("cannot read {path}: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The document was read and refused. Every error, ordered by path (§8 V1).
    ///
    /// Carried as rendered lines rather than as `ConfigError`s so the binary can print them without
    /// knowing the schema, and so the ordering the loader established survives being reported.
    #[error("{} problem(s) in the configuration", .0.len())]
    Rejected(Vec<String>),
    /// A listener the document declares cannot be served as declared.
    #[error("cluster.listener: {0}")]
    Listener(#[from] crate::listen::ListenerError),
    /// A reference in the document does not resolve to anything (§8 V9).
    #[error("{path} names `{reference}`, which is not set in the environment as `{variable}`")]
    UnresolvedReference {
        path: String,
        reference: String,
        variable: String,
    },
    /// The identity a node must be given from outside was not supplied (§5 P1).
    #[error("{0}")]
    MissingIdentity(String),
    /// A role in the identity names a path this build does not have (`DP-13`).
    ///
    /// The `FC-1`/`FC-3` shape, applied to §4: accepted means applied, or refused. A role that
    /// nothing wires cannot be honoured, and a node that came up serving *something else* instead is
    /// the failure this epic exists to remove — it was not hypothetical, either: a node started as
    /// `echo` came up as a full proxy **and** registrar, which is the one arrangement
    /// [e2e-probe](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md) §9
    /// forbids absolutely.
    #[error("this node was started with the `{role}` role, which this build cannot serve: {why}")]
    RoleNotServed {
        role: &'static str,
        why: &'static str,
    },
}

/// Where the node's identity comes from, since it is never in the document (§5 P1).
///
/// In Kubernetes these come from the downward API and the workload's role; on a plain host, from the
/// command line. Reading them out of the document would mean a per-node document, and then §3's
/// `version` would stop being a fact about the cluster.
#[derive(Debug, Clone, Default)]
pub struct IdentityArgs {
    pub node: Option<u16>,
    pub zone: Option<String>,
    pub roles: Option<String>,
}

impl IdentityArgs {
    /// Fill anything unset from the environment.
    ///
    /// `SIPX_CLSTR_NODE`, `SIPX_CLSTR_ZONE`, `SIPX_CLSTR_ROLES` — so a Kubernetes manifest can supply
    /// them from the downward API without a shell wrapper building a command line.
    #[must_use]
    pub fn or_env(mut self, env: &BTreeMap<String, String>) -> Self {
        if self.node.is_none() {
            self.node = env.get("SIPX_CLSTR_NODE").and_then(|v| v.parse().ok());
        }
        if self.zone.is_none() {
            self.zone = env.get("SIPX_CLSTR_ZONE").cloned();
        }
        if self.roles.is_none() {
            self.roles = env.get("SIPX_CLSTR_ROLES").cloned();
        }
        self
    }

    /// # Errors
    ///
    /// [`StartupError::MissingIdentity`] naming what was missing, and how to supply it.
    pub fn resolve(self) -> Result<NodeIdentity, StartupError> {
        let node = self.node.ok_or_else(|| {
            StartupError::MissingIdentity(
                "this node has no id: pass --node <1..65535> or set SIPX_CLSTR_NODE".to_owned(),
            )
        })?;
        let zone = self.zone.ok_or_else(|| {
            StartupError::MissingIdentity(
                "this node has no zone: pass --zone <name> or set SIPX_CLSTR_ZONE".to_owned(),
            )
        })?;
        let spelled = self.roles.ok_or_else(|| {
            StartupError::MissingIdentity(
                "this node has no roles: pass --roles <a,b> or set SIPX_CLSTR_ROLES".to_owned(),
            )
        })?;

        let mut roles = std::collections::BTreeSet::new();
        for name in spelled.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let role = Role::ALL
                .into_iter()
                .find(|role| role.as_str() == name)
                .ok_or_else(|| {
                    StartupError::MissingIdentity(format!(
                        "`{name}` is not a role; the set is {}",
                        Role::ALL
                            .iter()
                            .map(|r| r.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;
            roles.insert(role);
        }
        Ok(NodeIdentity { node, zone, roles })
    }
}

/// The environment, as a map the loader can be given (§8 V4 takes it as an argument).
#[must_use]
pub fn environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

/// Read a document from `path` and build the node configuration it describes for `identity`.
///
/// # Errors
///
/// Every way a document can fail to describe a startable node — see [`StartupError`]. The loader's
/// errors arrive together and in path order, because reporting only the first would waste the one
/// property that makes a five-mistake document cost one restart instead of five.
pub fn from_document(
    path: &str,
    identity: &NodeIdentity,
    env: &BTreeMap<String, String>,
) -> Result<NodeConfig, StartupError> {
    let bytes = std::fs::read(path).map_err(|source| StartupError::Unreadable {
        path: path.to_owned(),
        source,
    })?;

    let cluster = config::load(&bytes, identity, env).map_err(|errors| {
        StartupError::Rejected(errors.iter().map(ToString::to_string).collect())
    })?;

    let projected = config::project(&cluster, identity);
    node_config(&cluster, &projected, env)
}

/// Turn a projected document into the driver's configuration.
fn node_config(
    cluster: &Config,
    projected: &ProjectedConfig,
    env: &BTreeMap<String, String>,
) -> Result<NodeConfig, StartupError> {
    // `DP-13`, and the first thing this function does: a role this build has no path for stops the
    // node here, before a socket is bound and before anything else in the document is turned into
    // behaviour. Refused **by name**, and the reason says which section would have configured it, so
    // an operator can tell "not yet built" from "you spelled it wrong" — the loader has already
    // proved the spelling by this point (§4 R1).
    for role in &projected.identity.roles {
        // An exhaustive match rather than a set of "unserved" roles, and for the same reason the
        // transport arm below is written out: adding a role to §4's closed set without deciding what
        // runs it becomes a compile error here instead of a node that quietly serves something else.
        let unserved = match role {
            Role::Edge | Role::Registrar | Role::InboundProxy | Role::OutboundProxy => continue,
            Role::Echo => Some((
                role.as_str(),
                "the echo endpoint is a UAS (e2e-probe §9) that this driver does not run, and \
                 `cluster.echo` is not a section this loader reads. Until it does, a node given this \
                 role can only answer as something it is not",
            )),
            Role::E2eTester => Some((
                role.as_str(),
                "the probe engine is not driven by this binary, and `cluster.probe` is not a section \
                 this loader reads. Until it is, a node given this role would probe nothing",
            )),
        };
        if let Some((role, why)) = unserved {
            return Err(StartupError::RoleNotServed { role, why });
        }
    }

    let mut listeners = Vec::new();
    for declared in &projected.listeners {
        let bind: SocketAddr = declared
            .bind
            .parse()
            .map_err(|_| StartupError::Listener(crate::listen::ListenerError::EmptyHost))?;
        // The loader has already refused anything but these two — `tls`/`ws`/`wss` are rejected by
        // name rather than downgraded, so this cannot silently pick cleartext for a transport that
        // was asked for. Kept as an explicit refusal rather than a `_ =>` default so that adding a
        // transport to the loader without wiring it here is a startup error, not a substitution.
        let transport = match declared.transport.as_str() {
            "udp" => TransportKind::Udp,
            "tcp" => TransportKind::Tcp,
            other => {
                return Err(StartupError::MissingIdentity(format!(
                    "cluster.listener declares transport `{other}`, which this build cannot serve"
                )));
            }
        };
        let listener = match &declared.advertise {
            Some(advertise) => Listener::new(transport, bind, Advertised::parse(advertise)?)?,
            None => Listener::bound(transport, bind)?,
        };
        listeners.push(listener);
    }

    let mut config = NodeConfig::listening(Listeners::new(listeners)?);

    // `DP-13` — the roles reach dispatch. This is the transfer the finding was about: everything
    // below this line was already carried across, and the one thing that decides *which* of it a node
    // may act on was not, so `serve` matched on method alone.
    config.capabilities = config::Capabilities::of(&projected.identity.roles);

    // One tenant per node is still the driver's shape (`RG-12`'s note); the document may declare
    // several, and this takes the first rather than inventing a selection rule the schema does not
    // have. When the driver grows multi-tenancy this is the line that changes.
    if let Some(tenant) = projected.tenants.first() {
        config.tenant.clone_from(&tenant.name);
    }
    config.auth = auth_config(&config.tenant, projected, env)?;

    // `FC-4` — the rest of `tenant[]` reaches the registrar. `tenants.first()` keeps the driver's
    // one-tenant-per-node shape (`RG-12`'s note); a document declaring more than one is refused
    // rather than silently served by its first, because honouring one of several is a partial apply.
    if projected.tenants.len() > 1 {
        return Err(StartupError::MissingIdentity(format!(
            "this build serves one tenant per node and the document declares {}. Honouring the first \
             and ignoring the rest would be a silent partial apply; split them across nodes, or wait \
             for multi-tenancy.",
            projected.tenants.len()
        )));
    }
    if let Some(tenant) = projected.tenants.first() {
        config.policy = sipx_clstr_registrar::TenantPolicy {
            default_expires: tenant.policy.default_expires,
            min_expires: tenant.policy.min_expires,
            max_expires: tenant.policy.max_expires,
            max_bindings_per_aor: tenant.policy.max_bindings_per_aor,
        };
        config.domains.clone_from(&tenant.domains);
    }

    config.store = store_choice(projected, env)?;

    // `DP-11`. Applied here and therefore **absent from `unapplied`**: the loader records that list
    // where it recognises a section, so a key that reaches the driver must not appear on it or the
    // startup warning below would say the node ignores a bound it is enforcing.
    config.max_in_flight_transactions = projected.admission.max_in_flight_transactions;

    // `PX-10`, and the same rule as the line above: applied here, therefore **absent from
    // `unapplied`**. The loader refuses `timerC <= 180 s` (§8 V7), so whatever arrives here already
    // satisfies F11's floor and this conversion cannot smuggle an illegal value past it.
    //
    // The rest of `cluster.timers` — `t1`, `timerB`, `timerF` — is *not* applied and says so, in
    // `read_timers`. Those three are the kernel's transaction timer constants, and reaching them
    // means `sipx_transport::Config::timers`, which this build never sets; Timer C is the proxy TU's
    // own and is the one this story wired. Accepted-and-silently-discarded is the third outcome, and
    // it is the one that is not available.
    config.timer_c = Duration::from_millis(projected.timers.timer_c_ms);

    // Said out loud at startup rather than discovered as behaviour that never happens — and said by
    // **path**, because the keys that matter are not top level. A set of section names could not have
    // named `cluster.tenant[0].auth`, which is the one an operator most needs to hear about.
    //
    // A warning is not a fix. Configuration a node's roles genuinely do not consume is normal
    // (§4 R5); authentication and transport want a refusal, and `FC-1`/`FC-3` are where that lands.
    // What must not happen is neither.
    if !cluster.unapplied.is_empty() {
        let paths = cluster
            .unapplied
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        tracing::warn!(
            unapplied = %paths,
            count = cluster.unapplied.len(),
            "the document declares configuration this build accepts and does NOT apply"
        );
        // Security-relevant keys get their own line, because "not applied" reads very differently
        // for `observability` than it does for the thing standing between a stranger and your
        // registrar.
        for path in &cluster.unapplied {
            if matches!(path.leaf(), "auth" | "tls") {
                tracing::warn!(
                    path = %path,
                    "SECURITY: this key is accepted and ignored — the node does not do what it says"
                );
            }
        }
    }

    Ok(config)
}

/// Resolve `locationStore` into something the driver can open (§8 V9).
fn store_choice(
    projected: &ProjectedConfig,
    env: &BTreeMap<String, String>,
) -> Result<StoreChoice, StartupError> {
    let Some(declared) = &projected.location_store else {
        // R5 projected it away, or the document declared none. Either way this node keeps its own
        // bindings, which is correct for a node that is not a registrar.
        return Ok(StoreChoice::InMemory);
    };
    if declared.backend == "memory" {
        return Ok(StoreChoice::InMemory);
    }
    let Some(reference) = &declared.dsn_ref else {
        // The loader already refused this; belt and braces, because a `None` here would otherwise
        // become a silent in-memory store.
        return Err(StartupError::UnresolvedReference {
            path: "cluster.locationStore.dsnRef".to_owned(),
            reference: String::new(),
            variable: String::new(),
        });
    };
    let variable = variable_for(reference);
    let dsn = env
        .get(&variable)
        .ok_or_else(|| StartupError::UnresolvedReference {
            path: "cluster.locationStore.dsnRef".to_owned(),
            reference: reference.clone(),
            variable: variable.clone(),
        })?;
    Ok(StoreChoice::Postgres { dsn: dsn.clone() })
}

/// The environment variable a reference names: uppercased, with `-` and `.` as `_`.
///
/// The document carries `dsnRef: location-dsn`; the operator renders that reference into a Secret and
/// the Secret into an environment variable. On a plain host the same mapping lets an operator export
/// `LOCATION_DSN` and be done. The rule is stated here because the spec deliberately leaves the
/// resolution mechanism to the driver.
/// Build the digest policy the document asks for, and refuse if it cannot be honoured (`FC-3`).
///
/// The credential the document *names* is the nonce secret, resolved from the environment exactly
/// like a `dsnRef` (§8 V9). Resolution is IO, so it happens here and not in the loader.
///
/// **The decision that cannot be defaulted:** a document that asks for authentication and whose
/// secret *does* resolve is **refused**, because there are no user credentials yet — `RG-7` owns
/// where they come from — and running `required` against an empty store would reject every
/// `REGISTER` at `401`. That is a registrar that takes no registrations, and it is worse than open,
/// because it also *looks* protected. So the operator is told to remove the block (declaring an open
/// tenant deliberately) or wait for the credential store.
fn auth_config(
    tenant: &str,
    projected: &ProjectedConfig,
    env: &BTreeMap<String, String>,
) -> Result<Option<crate::driver::AuthConfig>, StartupError> {
    let spec = projected.tenants.iter().find_map(|t| t.auth.as_ref());
    let Some(auth) = spec else {
        // No auth block: an open tenant, deliberately chosen by the document's author. Nothing to
        // build and nothing to warn about beyond what `FC-2` already reports.
        return Ok(None);
    };

    let variable = variable_for(&auth.secret_ref);
    let resolved = env.get(&variable);

    match resolved {
        Some(secret_text) => {
            // A real secret, and no credentials to check anything against. Refuse rather than run a
            // registrar that says 401 to everyone while looking authenticated.
            let _ = secret_text; // resolved, and that is what forces the refusal
            Err(StartupError::MissingIdentity(format!(
                "tenant `{tenant}` requires authentication, but this build has no credential store \
                 to check users against (RG-7 owns that). A `secretRef` that resolves would produce a \
                 registrar that answers 401 to every REGISTER while appearing protected. Remove \
                 `tenant[].auth` to run an open tenant deliberately, or supply credentials once RG-7 \
                 lands. Secret `{0}` was named by reference, never read into the document.",
                auth.secret_ref
            )))
        }
        None => {
            // The secret does not resolve, so nothing can ever be challenged. An open tenant is the
            // only thing this node can be, and building it with a zeroed key is a safe placeholder:
            // with no users, no credential will ever validate against any key. The nonce secret never
            // crosses the process boundary, so a fixed value leaks nothing.
            Ok(Some(crate::driver::AuthConfig {
                realm: auth.realm.clone(),
                secret: [0u8; 32],
                credentials: sipx_clstr_registrar::InMemoryCredentials::default(),
            }))
        }
    }
}

fn variable_for(reference: &str) -> String {
    reference
        .chars()
        .map(|c| match c {
            '-' | '.' | '/' => '_',
            other => other.to_ascii_uppercase(),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn a_reference_names_an_environment_variable() {
        assert_eq!(variable_for("location-dsn"), "LOCATION_DSN");
        assert_eq!(variable_for("acme.store"), "ACME_STORE");
    }

    #[test]
    fn identity_comes_from_flags_or_the_environment() {
        let from_env = IdentityArgs::default()
            .or_env(&env(&[
                ("SIPX_CLSTR_NODE", "7"),
                ("SIPX_CLSTR_ZONE", "b"),
                ("SIPX_CLSTR_ROLES", "edge,registrar"),
            ]))
            .resolve()
            .expect("resolves");
        assert_eq!(from_env.node, 7);
        assert_eq!(from_env.zone, "b");
        assert!(from_env.roles.contains(&Role::Edge));
        assert!(from_env.roles.contains(&Role::Registrar));

        // A flag wins over the environment: the command line is the more specific statement.
        let overridden = IdentityArgs {
            node: Some(3),
            ..IdentityArgs::default()
        }
        .or_env(&env(&[
            ("SIPX_CLSTR_NODE", "7"),
            ("SIPX_CLSTR_ZONE", "b"),
            ("SIPX_CLSTR_ROLES", "edge"),
        ]))
        .resolve()
        .expect("resolves");
        assert_eq!(overridden.node, 3);
    }

    #[test]
    fn a_missing_identity_says_what_to_pass() {
        let error = IdentityArgs::default().resolve().expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains("--node"), "{message}");
        assert!(message.contains("SIPX_CLSTR_NODE"), "{message}");
    }

    #[test]
    fn an_unknown_role_on_the_command_line_spells_the_closed_set() {
        let error = IdentityArgs {
            node: Some(1),
            zone: Some("a".to_owned()),
            roles: Some("edge,sbc".to_owned()),
        }
        .resolve()
        .expect_err("must refuse");
        assert!(error.to_string().contains("outbound-proxy"), "{error}");
    }
}
