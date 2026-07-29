//! The sans-IO rule, asserted against the **resolved** dependency graph.
//!
//! The workspace manifest says it in a comment: `tokio` is a dependency of `sipx-clstr-node` and of
//! nothing else, because decision logic that can reach a runtime eventually does. Until `RG-2` that
//! was checkable by reading this crate's own manifest — there were no kernel crates in it to hide
//! behind.
//!
//! `RG-2` adds `sipx-ua`, for the digest primitives. `sipx-ua` *can* pull `tokio`; upstream `X-20`
//! put those primitives behind `default-features = false` so it does not have to. That makes the
//! rule transitive, and a rule that has become transitive is one a manifest-text check no longer
//! catches: the workspace could turn that feature back on and nothing in this crate's `Cargo.toml`
//! would change.
//!
//! So this walks `Cargo.lock`. It is the artifact that records what will actually be linked, and it
//! is the only place the answer is not a matter of inference.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Crates that must not be reachable from this one, and why each is disqualifying.
const FORBIDDEN: &[(&str, &str)] = &[
    (
        "tokio",
        "an async runtime in the decision core is how sans-IO stops being true",
    ),
    (
        "sipx-transport",
        "a transport brings sockets, and this crate decides rather than sends",
    ),
    (
        "sipx-clstr-node",
        "the driver depends on this crate, never the other way round",
    ),
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `<root>/crates/sipx-clstr-registrar`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("a workspace two levels up")
        .to_path_buf()
}

/// Every package in `Cargo.lock`, mapped to the names it depends on.
///
/// A hand parser rather than a TOML crate: the lockfile's `[[package]]` blocks are three fields and
/// a list, and adding a dependency to a test whose whole subject is the dependency graph would be a
/// poor joke.
fn dependency_graph(lockfile: &str) -> HashMap<String, Vec<String>> {
    let mut graph = HashMap::new();
    let mut name: Option<String> = None;
    let mut dependencies: Vec<String> = Vec::new();
    let mut in_dependencies = false;

    let flush = |graph: &mut HashMap<String, Vec<String>>,
                 name: &mut Option<String>,
                 dependencies: &mut Vec<String>| {
        if let Some(name) = name.take() {
            // Several versions of one crate share a name here. Merging their edges is the
            // conservative direction: it can only make the graph larger, never hide a path.
            graph.entry(name).or_default().append(dependencies);
        }
        dependencies.clear();
    };

    for line in lockfile.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            flush(&mut graph, &mut name, &mut dependencies);
            in_dependencies = false;
        } else if let Some(value) = trimmed.strip_prefix("name = ") {
            name = Some(value.trim_matches('"').to_owned());
            in_dependencies = false;
        } else if trimmed == "dependencies = [" {
            in_dependencies = true;
        } else if trimmed == "]" {
            in_dependencies = false;
        } else if in_dependencies {
            // Entries are `"name"` or `"name version"`; the version is not our business.
            let entry = trimmed.trim_end_matches(',').trim_matches('"');
            if let Some(dependency) = entry.split_whitespace().next() {
                dependencies.push(dependency.to_owned());
            }
        }
    }
    flush(&mut graph, &mut name, &mut dependencies);
    graph
}

/// Everything reachable from `root`, following `dependencies` transitively.
fn reachable(graph: &HashMap<String, Vec<String>>, root: &str) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut queue = vec![root.to_owned()];
    while let Some(package) = queue.pop() {
        if !seen.insert(package.clone()) {
            continue;
        }
        if let Some(dependencies) = graph.get(&package) {
            queue.extend(dependencies.iter().cloned());
        }
    }
    seen.remove(root);
    seen
}

#[test]
fn the_registrar_links_no_runtime() {
    let lockfile = std::fs::read_to_string(workspace_root().join("Cargo.lock"))
        .expect("the workspace lockfile");
    let graph = dependency_graph(&lockfile);
    assert!(
        graph.contains_key("sipx-clstr-registrar"),
        "the lockfile parser found no `sipx-clstr-registrar`; it has stopped understanding the \
         format, which would make every assertion below vacuously true"
    );
    // A sanity edge: if this is missing, the parser is not reading `dependencies` at all and the
    // test would pass by finding nothing rather than by there being nothing to find.
    let reachable = reachable(&graph, "sipx-clstr-registrar");
    assert!(
        reachable.contains("sipx-ua"),
        "expected the digest primitives to be reachable; the parser is not following edges"
    );

    for (forbidden, why) in FORBIDDEN {
        assert!(
            !reachable.contains(*forbidden),
            "`{forbidden}` is reachable from sipx-clstr-registrar: {why}.\n\
             If this is the digest dependency, check that the workspace still pins \
             `sipx-ua = {{ …, default-features = false }}` — upstream X-20 created that seam \
             precisely so this crate could take digest without taking a runtime."
        );
    }
}

#[test]
fn the_parser_reads_what_it_claims_to() {
    // The graph walk is only as trustworthy as the parse, and a parser that silently returned an
    // empty graph would make the real test pass forever. This pins its behaviour on a fixture.
    let fixture = r#"
[[package]]
name = "alpha"
version = "1.0.0"
dependencies = [
 "beta",
 "gamma 2.0.0",
]

[[package]]
name = "beta"
version = "1.0.0"
dependencies = [
 "delta",
]

[[package]]
name = "gamma"
version = "2.0.0"

[[package]]
name = "delta"
version = "1.0.0"
"#;
    let graph = dependency_graph(fixture);
    assert_eq!(graph.get("alpha").unwrap(), &["beta", "gamma"]);
    assert_eq!(graph.get("beta").unwrap(), &["delta"]);
    assert!(
        graph.contains_key("gamma"),
        "a package with no dependencies"
    );

    // Transitive, which is the whole point: `delta` is two hops from `alpha`.
    let reachable = reachable(&graph, "alpha");
    assert_eq!(
        reachable,
        ["beta", "gamma", "delta"]
            .into_iter()
            .map(str::to_owned)
            .collect::<HashSet<_>>()
    );
}
