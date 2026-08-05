//! The devspace walkthrough's call step targets an address a phone can actually dial.
//!
//! Three constraints meet on one string (`KO-18`). `sipx dial` takes a literal destination and
//! resolves no names, so the address-of-record's host must be something a socket can send to.
//! `FC-4` refuses a `REGISTER` whose address-of-record is outside the tenant's `domains`
//! (location-service §5.1 S1, compared byte-exactly), so that host must be a literal in the
//! cluster document. And `FC-5` put that document in a ConfigMap written before any pod exists,
//! so the host cannot be a runtime-assigned address. The one string that satisfies all three is
//! an address that is *declared* rather than assigned: the per-node Service's static `clusterIP`,
//! stated in the same manifest as the document that serves it.
//!
//! Four artifacts must carry it byte-identically — the tenant's `domains` entry, the greeting's
//! registered address-of-record, the `dial` command `website/docs/getting-started.md` §4 has the
//! reader run, and the domain `scripts/k8s-two-node-call.sh` registers in — because the location
//! lookup keys on the whole address of record. `scripts/check-proof-domains.py` already holds the
//! scripted proofs to the document; what it deliberately does not read is the website (prose is
//! out of its scope) or whether the shared string is *dialable*. `DX-13` shipped with all four in
//! agreement on a Service name no spelling of `sipx dial` could send to, and the gate stayed
//! green. This test is the missing half: the agreement, plus the property that makes it usable.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;

use serde_yaml_ng::Value;
use sipx_clstr_node::config::{self, NodeIdentity, Role};

/// A file read from the repository root, so the test holds the artifacts that actually ship.
fn repo_file(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The manifest's YAML documents. Split on whole `---` lines rather than parsed as a stream, so a
/// comment-only preamble is simply a document that parses to nothing.
fn manifest_documents() -> Vec<Value> {
    let text = repo_file("deploy/devspace/manifests/node.yaml");
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.trim_end() == "---" {
            chunks.push(std::mem::take(&mut current));
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    chunks.push(current);
    chunks
        .iter()
        .filter_map(|chunk| serde_yaml_ng::from_str::<Value>(chunk).ok())
        .filter(Value::is_mapping)
        .collect()
}

/// The one manifest object of the given kind and name.
fn object<'a>(documents: &'a [Value], kind: &str, name: &str) -> &'a Value {
    documents
        .iter()
        .find(|doc| {
            doc.get("kind").and_then(Value::as_str) == Some(kind)
                && doc
                    .get("metadata")
                    .and_then(|meta| meta.get("name"))
                    .and_then(Value::as_str)
                    == Some(name)
        })
        .unwrap_or_else(|| panic!("the manifest should contain a {kind} named {name}"))
}

/// The cluster document both nodes mount, exactly as the ConfigMap carries it.
fn cluster_document(documents: &[Value]) -> String {
    object(documents, "ConfigMap", "sipx-clstr-cluster")
        .get("data")
        .and_then(|data| data.get("cluster.yaml"))
        .and_then(Value::as_str)
        .expect("the ConfigMap should carry a cluster.yaml document")
        .to_owned()
}

/// The host of a `sip:` URI — the domain the registrar keys the binding under.
fn host_of(uri: &str) -> String {
    let body = uri.trim_start_matches("sip:");
    let after_user = body.rsplit_once('@').map_or(body, |(_, host)| host);
    after_user
        .split(|c: char| c == ':' || c == ';' || c == '?')
        .next()
        .unwrap_or(after_user)
        .to_owned()
}

/// The first `sip:` URI following `needle` in `text`, as the shell would see it.
fn uri_after(text: &str, needle: &str, what: &str) -> String {
    let start = text
        .find(needle)
        .unwrap_or_else(|| panic!("{what} should contain `{needle}`"))
        + needle.len();
    text[start..]
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'' && *c != '\\')
        .collect()
}

/// The devspace document, read by the same loader the node runs — not by a second parser with its
/// own opinions. `${POD_IP}` resolves per node from the environment (cluster-config §8 V4), so the
/// test supplies one the way the kubelet does.
fn loaded_config() -> config::Config {
    let identity = NodeIdentity {
        node: 1,
        zone: "a".to_owned(),
        roles: [Role::Edge, Role::Registrar].into_iter().collect(),
    };
    let env: BTreeMap<String, String> =
        [("POD_IP".to_owned(), "10.42.0.10".to_owned())].into();
    config::load(cluster_document(&manifest_documents()).as_bytes(), &identity, &env)
        .unwrap_or_else(|errors| {
            panic!("the devspace document should load through the node's own loader: {errors:?}")
        })
}

#[test]
fn the_devspace_document_loads_and_still_names_its_domains() {
    // The `FC-4` half of the non-regression: whatever address this profile settles on, the tenant
    // still declares it — `domains: []` would mean "any", which is the fail-open `FC-4` removed,
    // and a `REGISTER` outside the list must still be refused.
    let config = loaded_config();
    let tenant = config
        .tenants
        .first()
        .expect("the devspace document should declare a tenant");
    assert!(
        !tenant.domains.is_empty(),
        "the devspace tenant declares no domains, which means \"any\" — the fail-open FC-4 closed"
    );
}

#[test]
fn the_walkthrough_agrees_on_one_address_and_a_phone_can_dial_it() {
    let documents = manifest_documents();
    let config = loaded_config();
    let served = &config.tenants.first().expect("a tenant").domains;

    // The greeting's address-of-record, as the manifest registers it.
    let manifest_text = repo_file("deploy/devspace/manifests/node.yaml");
    let greeting = host_of(&uri_after(
        &manifest_text,
        "sipx register \"",
        "the greeting deployment",
    ));

    // The address the published walkthrough has the reader dial (§4's caller command).
    let walkthrough = repo_file("website/docs/getting-started.md");
    let dialled = host_of(&uri_after(&walkthrough, "sipx dial ", "getting-started §4"));

    // The domain the scripted cluster proof registers and dials in.
    let script = repo_file("scripts/k8s-two-node-call.sh");
    let proof_domain = script
        .lines()
        .find_map(|line| line.strip_prefix("DOMAIN=\"")?.strip_suffix('"'))
        .expect("k8s-two-node-call.sh should assign DOMAIN as a literal")
        .to_owned();

    // One string, everywhere the reader meets it. The location lookup keys on the whole address
    // of record and the registrar compares domains byte-exactly, so "equivalent" is not enough.
    assert_eq!(
        served,
        &vec![greeting.clone()],
        "the greeting registers in a domain the tenant does not serve — that REGISTER is a 403"
    );
    assert_eq!(
        greeting, dialled,
        "getting-started §4 dials a different address than the greeting registered — that call \
         is a 480"
    );
    assert_eq!(
        greeting, proof_domain,
        "k8s-two-node-call.sh registers in a different domain than the greeting"
    );

    // ... and the string is one `sipx dial` can send to. `dial` takes a literal destination and
    // resolves no names, so a Service *name* here exits on a usage error before a packet leaves
    // the pod — which is exactly what stopped `DX-13`.
    assert!(
        greeting.parse::<IpAddr>().is_ok(),
        "the shared address `{greeting}` is not a literal IP address, so `sipx dial` cannot send \
         to it — the walkthrough's call step fails on a usage error"
    );

    // The FC-5 half of the non-regression: the address is *declared* in the manifest — a static
    // `clusterIP` on node-a's Service, written before any pod exists — not assigned at runtime.
    // A ConfigMap authored before the pods can hold it precisely because nothing has to run first.
    let service = object(&documents, "Service", "sipx-clstr-node-a");
    let declared = service
        .get("spec")
        .and_then(|spec| spec.get("clusterIP"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "node-a's Service declares no static clusterIP, so the address the document \
                 serves is runtime-assigned — the situation FC-5 exists to rule out"
            )
        });
    assert_eq!(
        declared, greeting,
        "node-a's Service declares clusterIP {declared} but the walkthrough's address is \
         {greeting} — the dialled packet would not reach the node that serves the domain"
    );
}
