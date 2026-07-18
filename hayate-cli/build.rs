//! Build script for hayate-cli.
//!
//! Injects git-derived version metadata into the binary.

#![allow(missing_docs)]

use std::process::Command as StdCommand;

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");
    emit_build_metadata();
}

fn emit_build_metadata() {
    let version = env!("CARGO_PKG_VERSION");

    println!("cargo:rustc-env=GIT_VERSION={version}");
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_short_hash());
}

fn git_short_hash() -> String {
    StdCommand::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}
