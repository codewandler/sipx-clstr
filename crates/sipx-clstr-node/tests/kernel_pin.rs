//! The reported kernel version must be the kernel version.
//!
//! `KERNEL_VERSION` is what an operator reads off a running process during an incident, and the
//! `tag = "v…"` in the workspace manifest is what actually gets compiled in. Nothing keeps those
//! two in step except this test: bump the pin, forget the constant, and the binary confidently
//! reports the wrong answer to the one question it exists to answer.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

/// Pull `tag = "vX.Y.Z"` off the first sipx git dependency in the workspace manifest.
fn pinned_kernel_tag(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let line = line.trim();
        if !line.starts_with("sipx-sip") || !line.contains("codewandler/sipx") {
            continue;
        }
        let (_, after) = line.split_once("tag = \"")?;
        let (tag, _) = after.split_once('"')?;
        return Some(tag.trim_start_matches('v').to_owned());
    }
    None
}

#[test]
fn the_reported_kernel_version_matches_the_pinned_tag() {
    let workspace_manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("Cargo.toml");
    let manifest = std::fs::read_to_string(&workspace_manifest)
        .unwrap_or_else(|e| panic!("reading {}: {e}", workspace_manifest.display()));

    let pinned = pinned_kernel_tag(&manifest)
        .expect("the workspace manifest should pin sipx-sip to a git tag");

    assert_eq!(
        pinned,
        sipx_clstr_node::KERNEL_VERSION,
        "KERNEL_VERSION says {} but the workspace pins the kernel at {pinned}",
        sipx_clstr_node::KERNEL_VERSION,
    );
}

#[test]
fn a_branch_pin_is_not_mistaken_for_a_tag() {
    // A `branch = "main"` dependency has no tag, and this must read as "no pin" rather than as
    // some accidental substring. A reproducible build is the reason the pin is a tag at all.
    let manifest = r#"sipx-sip = { git = "https://github.com/codewandler/sipx", branch = "main" }"#;
    assert_eq!(pinned_kernel_tag(manifest), None);
}
