//! No proxy role links a UAS — `ET-3`'s third acceptance criterion, as a check rather than a promise.
//!
//! [e2e-probe §9](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md) makes
//! this constraint absolute: the echo is a UAS, so a process running `echo` runs no proxy role. The
//! structural half of that is the dependency graph — this crate must not depend on the forwarding
//! core — and a manifest is a thing a test can read, whereas an intention is not.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

fn manifest() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

#[test]
fn the_echo_crate_does_not_depend_on_a_proxy_role() {
    // §9's hard constraint, made structural. If someone adds the forwarding core here to reuse "just
    // one helper", the echo becomes linkable into a proxy process and the separation is gone —
    // silently, because nothing else would notice.
    let manifest = manifest();
    for forbidden in [
        "sipx-clstr-proxy",
        "sipx-clstr-registrar",
        "sipx-clstr-node",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "the probe/echo crate must not depend on {forbidden}: §9 forbids a process running \
             `echo` from running a proxy role, and the dependency graph is where that is enforced"
        );
    }
}

#[test]
fn the_echo_crate_owns_no_runtime() {
    // A UAS that spawned its own runtime would be a second driver layer, and the one place allowed
    // to own a clock or a socket is `sipx-clstr-node`.
    let manifest = manifest();
    assert!(
        !manifest.contains("tokio"),
        "the driver layer owns the runtime; this crate is decision logic"
    );
}
