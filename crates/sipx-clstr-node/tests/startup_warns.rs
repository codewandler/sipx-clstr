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

mod support;

/// A document that declares three things this build accepts and does not apply, one of them nested
/// two levels down where a set of section names could never have named it.
///
/// It binds `127.0.0.1:0` (`CF-13`) — the kernel picks the port, so no run of this suite can collide
/// with another. The advertised address is a literal because `Advertised` refuses port zero and the
/// advertised address is decided before the bind; nothing binds it, and this node never forwards
/// anything, so it is inert here.
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
      bind: 127.0.0.1:0
      advertise: 127.0.0.1:15071
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
/// Each test gets its **own** node, still: they run in parallel by default, and one shared port made
/// three of them pass while the fourth failed on `Address already in use` — a node that never got as
/// far as the line under test. Since `CF-13` no test picks the number, so the same collision cannot
/// come back between two *suites* either.
///
/// The wait is on output rather than on a clock. `node listening` is the last line the node emits
/// during startup and it comes after every line asserted on here, so seeing it means the record is
/// complete — where the old fixed 1500 ms sleep meant "probably complete, on an unloaded machine".
fn stderr_of_a_started_node(document: &str, tag: &str) -> String {
    let path = support::write_document(document, tag);
    let node = support::BinaryNode::start(&path);
    let _ = node.stderr_until(|seen| seen.contains("node listening"));
    node.stop()
}

/// **The failing-first test for `FC-2`.** The warning must arrive, at the default log level.
#[test]
fn fc2_the_unapplied_configuration_warning_reaches_stderr() {
    let stderr = stderr_of_a_started_node(DOCUMENT, "unapplied-warning");
    assert!(
        stderr.contains("does NOT apply"),
        "the node must say that it is ignoring configuration. stderr was:\n{stderr}"
    );
}

/// It must name the keys **by path**, including the one nested inside a list element.
#[test]
fn fc2_the_warning_names_the_nested_security_keys() {
    let stderr = stderr_of_a_started_node(DOCUMENT, "nested-security-keys");
    // `auth` is applied since FC-3, so it must NOT appear here — a warning naming it would lie.
    // `tls` remains unapplied and must be named by path.
    assert!(
        stderr.contains("cluster.listener[0].tls"),
        "an accepted-and-ignored tls block must be nameable. stderr was:\n{stderr}"
    );
    assert!(
        !stderr.contains("cluster.tenant[0].auth"),
        "tenant auth is applied, so it must NOT be warned about. stderr was:\n{stderr}"
    );
}

/// A security-relevant key gets a line of its own, because "not applied" reads very differently for
/// `observability` than for the thing standing between a stranger and the registrar.
#[test]
fn fc2_a_security_key_is_called_out_separately() {
    let stderr = stderr_of_a_started_node(DOCUMENT, "security-called-out");
    assert!(
        stderr.contains("SECURITY:"),
        "an ignored auth or tls block must be escalated, not listed. stderr was:\n{stderr}"
    );
}

/// The startup line says whether authentication is on, and it must reflect the *document* — this
/// fixture declares `tenant[].auth`, so the line must say `required`, not `open`. When FC-3 landed,
/// this assertion failing was exactly the signal that authentication had become real.
#[test]
fn fc2_the_startup_line_says_whether_authentication_is_on() {
    let stderr = stderr_of_a_started_node(DOCUMENT, "auth-on-the-startup-line");
    assert!(
        stderr.contains(r#"auth="required""#),
        "a document declaring tenant[].auth must produce auth=required. stderr was:\n{stderr}"
    );
}
