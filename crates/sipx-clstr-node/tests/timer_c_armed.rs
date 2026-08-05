//! `PX-10` — the Timer C a forking INVITE arms is the one the **document** asked for.
//!
//! `cluster.timers` was parsed, validated and projected onto `ProjectedConfig`, and then read by
//! nobody: `grep -n timer_c crates/sipx-clstr-node/src/driver.rs` returned no matches. So
//! `proxy_config_keyed` handed the engine a `ProxyConfig` straight out of `ProxyConfig::new`, and
//! every INVITE branch was guarded by the proxy crate's own default — 180 s, which is also the one
//! value RFC 3261 §16.6 step 11 forbids ("the timer MUST be larger than 3 minutes"). An operator who
//! wrote `timers.timerC` got that 180 s, silently, and no error anywhere said so.
//!
//! **What "the armed Timer C" is, and why the assertion is where it is.** Arming is
//! `Effect::SetTimer { timer: ProxyTimer::C, after }`, emitted by the engine immediately after each
//! `Forward` (proxy-behavior §7 F11). That effect *is* the arming instruction — its `after` is the
//! deadline the branch gets — so it is what this asserts on, per branch, for a request that forks.
//!
//! It cannot be asserted by letting the timer **fire** through a socket. Every value the schema
//! admits is `> 180 s` (cluster-config §8 V7), so the shortest legal Timer C is over three minutes
//! of real time, and this crate is the one layer with a real clock. The deterministic harness is
//! where a fired Timer C is observed in virtual time
//! (`crates/sipx-clstr-sim/tests/proxy_cancel.rs`), and it has no dependency on this crate, so it
//! cannot see `driver.rs` at all. This test is therefore the only place the *document→driver→engine*
//! path is observable, and it observes it at the last point before a clock would be needed.
//!
//! Everything below the assertion is real: the real loader reads a real document off disk, the real
//! `driver::proxy_config` builds the `ProxyConfig` exactly as the driver builds it for an arrival,
//! and the real `ResponseContext` forks and arms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_proxy::{
    Effect as ProxyEffect, Input as ProxyInput, Kind, ProxyTimer, ResponseContext, Target,
};
use sipx_sip::{HeaderName, Method, Request, RequestBuilder, Uri};

use sipx_clstr_node::config::{DEFAULT_TIMER_C_MS, NodeIdentity, Role};
use sipx_clstr_node::driver;
use sipx_clstr_node::startup;

/// The Timer C this document asks for.
///
/// Distinct from **every** default in the tree, so a pass cannot be an accident: not the proxy
/// crate's 180 s, not the schema's 240 s (`DP-12`), and not the chart's 600 s. It is also legal —
/// `> 180 s` — because a document the loader refuses would prove nothing about what gets armed.
// Seconds, like every other Timer C reading in this tree (`CF-9`): F11 states the default and the
// floor in seconds, and a lone `from_mins` here would stop looking like the same quantity.
#[allow(clippy::duration_suboptimal_units)]
const ASKED_FOR: Duration = Duration::from_secs(300);

/// The node's port. Never bound — the document only has to *parse* — but it is its own so that a
/// stray bind in a future edit does not collide with the other driver suites.
const NODE_PORT: u16 = 15083;

/// A cluster document for one edge/registrar node that names its Timer C.
fn document(node_port: u16, timer_c: Duration) -> String {
    format!(
        "
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: px10
  environment: dev
  zones: [a]
  timers:
    timerC: {}
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 127.0.0.1:{node_port}
      advertise: 127.0.0.1:{node_port}
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
      # cluster-membership MB5: required of a member on the call path, dialled by nobody in this
      # build, and therefore reported as unapplied configuration.
      rpc: 127.0.0.1:7223
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [example.test]
",
        timer_c.as_millis(),
    )
}

fn write_document(text: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("px10-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cluster.yaml");
    let mut file = std::fs::File::create(&path).expect("create document");
    file.write_all(text.as_bytes()).expect("write document");
    drop(file);
    path.to_str().expect("utf-8 path").to_owned()
}

/// Load a document and build the `ProxyConfig` the driver would build for an arrival on it.
///
/// `driver::proxy_config` is the function under test: it is the one place a `NodeConfig` becomes the
/// engine's configuration, and it is where the document's Timer C either arrives or does not.
fn proxy_config_from(document: &str, tag: &str) -> sipx_clstr_proxy::ProxyConfig {
    let path = write_document(document, tag);
    let identity = NodeIdentity {
        node: 1,
        zone: "a".to_owned(),
        roles: [Role::Edge, Role::Registrar].into_iter().collect(),
    };
    let env = std::collections::BTreeMap::new();
    let node = startup::from_document(&path, &identity, &env)
        .expect("the document describes a node this build can run");
    driver::proxy_config(&node, None)
}

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

/// A dialog-forming INVITE for an AoR with more than one contact.
fn invite() -> Request {
    RequestBuilder::new(Method::Invite, uri("sip:bob@example.test"))
        .header(HeaderName::CallId, "px10-the-call")
        .unwrap()
        .cseq(1, &Method::Invite)
        .unwrap()
        .header(HeaderName::From, "<sip:alice@example.test>;tag=px10alice")
        .unwrap()
        .header(HeaderName::To, "<sip:bob@example.test>")
        .unwrap()
        .header(
            HeaderName::Via,
            "SIP/2.0/UDP 127.0.0.1:15084;branch=z9hG4bK-px10-invite",
        )
        .unwrap()
        .build()
}

/// Two contacts at equal `q`, which is what makes the request fork in parallel (§7 L4).
fn two_contacts() -> Vec<Target> {
    ["sip:bob@127.0.0.1:15085", "sip:bob@127.0.0.1:15086"]
        .into_iter()
        .map(|contact| Target {
            uri: Bytes::copy_from_slice(contact.as_bytes()),
            route_set: Vec::new(),
            q: 1_000,
        })
        .collect()
}

/// Drive the INVITE through a real engine and return the deadline armed on each branch.
fn armed_timer_c(config: sipx_clstr_proxy::ProxyConfig) -> Vec<Duration> {
    let mut context = ResponseContext::new(config);
    let mut effects = context.on_input(ProxyInput::Upstream(Box::new(invite())));
    assert!(
        effects.iter().any(|e| e.kind() == Kind::ResolveTargets),
        "the engine must ask for targets before anything can fork"
    );
    effects.extend(context.on_input(ProxyInput::TargetsResolved(two_contacts())));

    let forwarded = effects.iter().filter(|e| e.kind() == Kind::Forward).count();
    assert_eq!(
        forwarded, 2,
        "the INVITE must fork, or there is no branch to arm a timer on"
    );

    effects
        .iter()
        .filter_map(|effect| match effect {
            ProxyEffect::SetTimer {
                timer: ProxyTimer::C,
                after,
                ..
            } => Some(*after),
            _ => None,
        })
        .collect()
}

#[test]
fn the_armed_timer_c_is_the_one_the_document_asked_for() {
    let armed = armed_timer_c(proxy_config_from(
        &document(NODE_PORT, ASKED_FOR),
        "asked-for",
    ));

    assert_eq!(
        armed.len(),
        2,
        "F11 arms one Timer C per INVITE branch; got {armed:?}"
    );
    for (index, after) in armed.iter().enumerate() {
        assert_eq!(
            *after, ASKED_FOR,
            "branch {index}: the document asked for {ASKED_FOR:?} and the engine armed {after:?} — \
             cluster.timers.timerC reached the driver or it did not"
        );
    }
}

/// The other honest outcome for a document that says nothing: the schema's default, not the proxy
/// crate's private one. Pinned here because a wiring that only worked when the document spoke would
/// leave the silent path exactly as `DP-12` found it.
#[test]
fn a_document_that_names_no_timer_c_arms_the_schema_default() {
    let mut text = document(NODE_PORT, ASKED_FOR);
    text = text.replace(
        &format!("  timers:\n    timerC: {}\n", ASKED_FOR.as_millis()),
        "",
    );
    assert!(!text.contains("timerC"), "the timers section must be gone");

    let armed = armed_timer_c(proxy_config_from(&text, "defaulted"));
    let expected = Duration::from_millis(DEFAULT_TIMER_C_MS);
    for (index, after) in armed.iter().enumerate() {
        assert_eq!(
            *after, expected,
            "branch {index}: omission is a legal way to accept the schema's default (CC-V-12)"
        );
    }
}
