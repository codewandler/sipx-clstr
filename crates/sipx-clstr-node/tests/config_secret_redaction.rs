//! `FC-8` — refusing an inline secret never publishes the value it refused.
//!
//! The loader's typed-error surface is covered beside `CC-V-25` in `config::tests`. This file
//! exercises the two outward surfaces the process builds from it: `StartupError::Rejected`'s stable
//! refusal messages and the real binary's stderr. Both must name the offending path without carrying
//! the bytes at that path. The operator admission consumer does not exist yet; `CC-V-26` assigns
//! that separate response surface to `KO-3` rather than crediting an invented caller here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::{BTreeMap, BTreeSet};

use sipx_clstr_node::config::{NodeIdentity, Role};
use sipx_clstr_node::startup::{StartupError, from_document};

fn document(extra: &str) -> String {
    format!(
        r"
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: fc8
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 127.0.0.1:0
      advertise: 127.0.0.1:15118
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
      rpc: 127.0.0.1:7223
  locationStore:
    backend: memory
{extra}  tenant:
    - name: default
      id: 1
      domains: [example.test]
"
    )
}

fn identity() -> NodeIdentity {
    NodeIdentity {
        node: 1,
        zone: "a".to_owned(),
        roles: BTreeSet::from([Role::Edge, Role::Registrar]),
    }
}

fn assert_refusal_is_redacted(tag: &str, text: &str, path: &str, sentinel: &str) {
    let config_path = support::write_document(text, tag);

    // `StartupError::Rejected` is the stable rendered list handed to the process. It deliberately
    // contains errors rather than the input document; the operator's future status handoff is a
    // separate executable claim and is not pretended into existence here.
    let startup_messages = match from_document(&config_path, &identity(), &BTreeMap::default()) {
        Err(StartupError::Rejected(messages)) => messages,
        Err(other) => panic!("expected a configuration refusal, got {other}"),
        Ok(_) => panic!("a document containing {path} loaded"),
    };
    let expected_refusal = format!("{path} [CC-V9]");
    assert!(
        startup_messages
            .iter()
            .any(|message| message.contains(&expected_refusal)),
        "the startup messages must name {path} under CC-V9: {startup_messages:#?}"
    );
    assert!(
        startup_messages
            .iter()
            .all(|message| !message.contains(sentinel)),
        "the startup messages leaked {sentinel}: {startup_messages:#?}"
    );

    let refusal = match support::BinaryNode::try_start(&config_path) {
        Err(refusal) => refusal,
        Ok(node) => {
            let stderr = node.stop();
            panic!("a document containing {path} started a node; stderr was:\n{stderr}")
        }
    };
    assert!(refusal.exited, "the refusing process did not exit");
    assert!(
        refusal.stderr.contains(&expected_refusal),
        "the process log must name {path} under CC-V9:\n{}",
        refusal.stderr
    );
    assert!(
        !refusal.stderr.contains(sentinel),
        "the process log leaked {sentinel}:\n{}",
        refusal.stderr
    );
    assert!(
        !refusal.stdout.contains(sentinel),
        "stdout leaked {sentinel}:\n{}",
        refusal.stdout
    );
}

/// Every V9 reference spelling reaches the real process without publishing the refused value.
///
/// KY3 and `tenant[].auth.secret` were the two explicitly unproved call sites when `FC-8` was
/// filed. DSN already behaved correctly; listener and management TLS are included because both
/// carry the `keyRef` spelling and the failing-first run found that neither inspected its neighbour.
#[test]
fn cc_v_25_every_inline_secret_neighbour_is_redacted_in_startup_messages_and_process_output() {
    let dsn = "dsn-process-redaction-sentinel";
    assert_refusal_is_redacted(
        "fc8-dsn",
        &document("").replace(
            "    backend: memory\n",
            &format!(
                "    backend: postgres\n    dsnRef: location-dsn\n    dsn: postgres://{dsn}@db/loc\n"
            ),
        ),
        "cluster.locationStore.dsn",
        dsn,
    );

    let affinity = "affinity-process-redaction-sentinel";
    assert_refusal_is_redacted(
        "fc8-affinity",
        &document(&format!(
            "  keys:\n    - id: 3\n      algorithm: chacha20-poly1305\n      secretRef: affinity-key-3\n      secret: {affinity}\n      verifyFrom: \"2026-07-28T12:00:00Z\"\n      verifyUntil: \"2026-08-04T12:00:30Z\"\n      mint: true\n"
        )),
        "cluster.keys[0].secret",
        affinity,
    );

    let nonce = "nonce-process-redaction-sentinel";
    assert_refusal_is_redacted(
        "fc8-nonce",
        &document("").replace(
            "      domains: [example.test]\n",
            &format!(
                "      domains: [example.test]\n      auth:\n        realm: example.test\n        secretRef: nonce-key\n        secret: {nonce}\n"
            ),
        ),
        "cluster.tenant[0].auth.secret",
        nonce,
    );

    let listener_key = "listener-key-process-redaction-sentinel";
    assert_refusal_is_redacted(
        "fc8-listener-key",
        &document("").replace(
            "      advertise: 127.0.0.1:15118\n",
            &format!(
                "      advertise: 127.0.0.1:15118\n      tls:\n        certRef: edge-cert\n        keyRef: edge-key\n        key: {listener_key}\n"
            ),
        ),
        "cluster.listener[0].tls.key",
        listener_key,
    );

    let management_key = "management-key-process-redaction-sentinel";
    assert_refusal_is_redacted(
        "fc8-management-key",
        &document(&format!(
            "  management:\n    bind: 127.0.0.1:9090\n    tls:\n      certRef: management-cert\n      keyRef: management-key\n      key: {management_key}\n"
        )),
        "cluster.management.tls.key",
        management_key,
    );
}
