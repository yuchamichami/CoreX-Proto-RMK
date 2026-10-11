#[path = "./build_common.rs"]
mod common;

use std::process::Command;

fn main() {
    let mut cfgs = common::CfgSet::new();
    common::set_target_cfgs(&mut cfgs);

    // `RMK_COMMIT` and `RMK_FEATURES` go into the storage schema hash.
    // When either changes, the storage is erased.
    let commit = Command::new("git")
        .args(["log", "-1", "--format=%H", "--", "."])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=RMK_COMMIT={commit}");

    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_lowercase))
        .collect();
    features.sort();
    println!("cargo:rustc-env=RMK_ENABLED_FEATURES={}", features.join(","));
    // Physical host authorization adds only volatile state and command guards.
    // It changes no serialized StorageValue variant or field. Keep existing
    // CoreX layouts and bonds when enabling this runtime-only feature.
    features.retain(|feature| feature != "host_lock");
    println!("cargo:rerun-if-env-changed=COREX_FIRMWARE_VERSION");
    let version = std::env::var("COREX_FIRMWARE_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let parts: Vec<u32> = version.split('.').map(|v| v.parse().expect("numeric CoreX version")).collect();
    assert!(parts.len() == 3 && parts.iter().all(|v| *v <= 255));
    println!("cargo:rustc-env=COREX_FIRMWARE_VERSION={version}");
    println!("cargo:rustc-env=COREX_VIA_VERSION={}", (parts[0] << 16) | (parts[1] << 8) | parts[2]);
    println!("cargo:rustc-env=RMK_FEATURES={}", features.join(","));
}
