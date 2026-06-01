use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", commit);
    println!("cargo:rerun-if-changed=.git/HEAD");

    let ref_path = Command::new("git")
        .args(["symbolic-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        });

    if let Some(ref_path) = ref_path {
        println!("cargo:rerun-if-changed=.git/{}", ref_path.trim());
    }
}
