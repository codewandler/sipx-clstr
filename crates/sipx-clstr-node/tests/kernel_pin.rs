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

/// The release `CX-12` moved to, named as a literal.
///
/// The test above holds the constant and the manifest to *each other*, which is what stops them
/// drifting — but it passes just as well when both say `0.7.0`, so on its own it cannot say which
/// kernel this workspace is supposed to be on. Naming the release here is what makes reverting the
/// pin a failing test rather than a silent return to a protocol core three releases behind. The
/// literal is meant to be edited by whoever moves the pin next, in the same commit that moves it.
#[test]
fn the_reported_kernel_is_the_release_this_workspace_moved_to() {
    assert_eq!(sipx_clstr_node::KERNEL_VERSION, "1.0.0-beta.4");
}

/// `--version` is the half of the claim an operator can actually reach.
///
/// `KERNEL_VERSION` being right is necessary and not sufficient: what gets read during an incident
/// is a line of output, and the published site quotes that line verbatim
/// (`website/docs/getting-started.md`, `website/docs/reference/cli.md`). Nothing held the constant
/// and the printed line together, so this runs the binary and reads what it prints — which is also
/// where those two pages' strings come from rather than being typed by hand.
#[test]
fn the_version_flag_prints_the_kernel_it_was_built_against() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_sipx-clstr"))
        .arg("--version")
        .output()
        .expect("the binary under test should run");

    assert!(output.status.success(), "--version should exit zero");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!(
            "sipx-clstr {} (sipx kernel {})",
            sipx_clstr_node::VERSION,
            sipx_clstr_node::KERNEL_VERSION
        ),
    );
}

#[test]
fn a_branch_pin_is_not_mistaken_for_a_tag() {
    // A `branch = "main"` dependency has no tag, and this must read as "no pin" rather than as
    // some accidental substring. A reproducible build is the reason the pin is a tag at all.
    let manifest = r#"sipx-sip = { git = "https://github.com/codewandler/sipx", branch = "main" }"#;
    assert_eq!(pinned_kernel_tag(manifest), None);
}
