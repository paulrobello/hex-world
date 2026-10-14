//! Upgrade tripwire for assets/shaders/kit_prepass.wesl.
//!
//! The kit prepass fragment copies the non-bindless, non-meshlet path of bevy_pbr's
//! render/pbr_prepass.wesl and render/pbr_prepass_functions.wesl. This test hashes those upstream
//! files from the resolved bevy_pbr crate, so a Bevy upgrade that edits them fails here.
//!
//! On failure: diff the upstream file against assets/shaders/kit_prepass.wesl, port the change,
//! then update the pinned hash below.
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;

const PINNED: [(&str, &str); 2] = [
    (
        "render/pbr_prepass.wesl",
        "8569ab217cd9f251d4325f7037d1d5e14072653cbefc02217f693a623370cca6",
    ),
    (
        "render/pbr_prepass_functions.wesl",
        "f956393975ae73f04a66fc213ec149de4d3352adc1418ce52a7216b874d454a1",
    ),
];
const PINNED_VERSION: &str = "0.20.0";

fn bevy_pbr_dir() -> (PathBuf, String) {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pkg = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "bevy_pbr")
        .expect("bevy_pbr in the dependency graph");
    let manifest = PathBuf::from(pkg["manifest_path"].as_str().unwrap());
    (
        manifest.parent().unwrap().join("src"),
        pkg["version"].as_str().unwrap().to_string(),
    )
}

#[test]
fn upstream_prepass_shader_matches_the_kit_copy() {
    let (src, version) = bevy_pbr_dir();
    let mut drift = vec![];
    for (rel, pinned) in PINNED {
        let bytes = std::fs::read(src.join(rel))
            .unwrap_or_else(|e| panic!("bevy_pbr {version}: {rel}: {e}"));
        let got = format!("{:x}", Sha256::digest(&bytes));
        if got != pinned {
            drift.push(format!(
                "  {rel}: pinned {pinned}, bevy_pbr {version} has {got}"
            ));
        }
    }
    assert!(
        drift.is_empty(),
        "bevy_pbr's prepass shader changed (pinned against bevy_pbr {PINNED_VERSION}). Re-sync \
         assets/shaders/kit_prepass.wesl with the upstream file(s), then update PINNED in \
         crates/hex-render/tests/bevy_prepass_tripwire.rs:\n{}",
        drift.join("\n")
    );
}
