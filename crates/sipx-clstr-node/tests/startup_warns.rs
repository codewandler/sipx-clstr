//! The warning has to reach a human (`FC-2`).
//!
//! This is the test whose absence let four silently-discarded security keys ship. The loader's
//! "this build does not apply that" warning existed, its reasoning was right, and it was emitted into
//! a process with no `tracing` subscriber installed — so it went nowhere at every `RUST_LOG` level.
//! No unit test could have caught that: the warning *was* produced, and the defect was in the order
//! two things happened in `main`.
//!
//! So this drives the **binary**, and reads its **stderr**. Verified failing before the fix: the same
//! document produced no matching line at `RUST_LOG=trace`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::process::{Command, Stdio};

/// A document that declares three things this build accepts and does not apply, one of them nested
/// two levels down where a set of section names could never have named it.
const DOCUMENT: &str = r"
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: fc2
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 127.0.0.1:PORT
      advertise: 127.0.0.1:PORT
      tls: { certRef: some-cert }
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
  registrar:
    usePath: true
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [example.test]
      auth:
        realm: acme
        secretRef: nonce-key
";

/// Start the node, let it get as far as announcing itself, and return what it wrote to stderr.
///
/// Each test binds its **own** port. They run in parallel by default, and sharing one made three of
/// them pass while the fourth failed on `Address already in use` — a node that never got as far as the
/// line under test. A port per test is cheaper than serialising them.
fn stderr_of_a_started_node(document: &str, port: u16) -> String {
    let document = document.replace("PORT", &port.to_string());
    let document = document.as_str();
    let dir = std::env::temp_dir().join(format!("fc2-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("cluster.yaml");
    let mut file = std::fs::File::create(&path).expect("write document");
    file.write_all(document.as_bytes()).expect("write document");
    drop(file);

    let mut child = Command::new(env!("CARGO_BIN_EXE_sipx-clstr"))
        .args([
            "run",
            "--config",
            path.to_str().expect("utf-8 path"),
            "--node",
            "1",
            "--zone",
            "a",
            "--roles",
            "edge,registrar",
        ])
        // Deliberately not raising the level: this must arrive at the default, because an operator
        // who has to know to turn logging up has not been told.
        .env_remove("RUST_LOG")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");

    // The warning is emitted during load, before the socket is bound, so a short wait is enough.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let _ = child.kill();
    let output = child.wait_with_output().expect("collect output");

    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// **The failing-first test for `FC-2`.** The warning must arrive, at the default log level.
#[test]
fn fc2_the_unapplied_configuration_warning_reaches_stderr() {
    let stderr = stderr_of_a_started_node(DOCUMENT, 15071);
    assert!(
        stderr.contains("does NOT apply"),
        "the node must say that it is ignoring configuration. stderr was:\n{stderr}"
    );
}

/// It must name the keys **by path**, including the one nested inside a list element.
#[test]
fn fc2_the_warning_names_the_nested_security_keys() {
    let stderr = stderr_of_a_started_node(DOCUMENT, 15072);
    for expected in ["cluster.tenant[0].auth", "cluster.listener[0].tls"] {
        assert!(
            stderr.contains(expected),
            "`{expected}` must be named — a set of section names could not. stderr was:\n{stderr}"
        );
    }
}

/// A security-relevant key gets a line of its own, because "not applied" reads very differently for
/// `observability` than for the thing standing between a stranger and the registrar.
#[test]
fn fc2_a_security_key_is_called_out_separately() {
    let stderr = stderr_of_a_started_node(DOCUMENT, 15073);
    assert!(
        stderr.contains("SECURITY:"),
        "an ignored auth or tls block must be escalated, not listed. stderr was:\n{stderr}"
    );
}

/// The startup line says whether authentication is on. Today it is always `open`, which is precisely
/// why it is worth printing — an operator reading one line should not have to infer it.
#[test]
fn fc2_the_startup_line_says_whether_authentication_is_on() {
    let stderr = stderr_of_a_started_node(DOCUMENT, 15074);
    assert!(
        stderr.contains(r#"auth="open""#),
        "the node listening line must report the tenant's auth state. stderr was:\n{stderr}"
    );
}
