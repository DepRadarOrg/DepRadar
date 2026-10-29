use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_MANIFEST_DIR");
    println!("cargo:rerun-if-changed=.git/HEAD");
    let head = std::fs::read_to_string(".git/HEAD").ok();
    if let Some(reference) = head.as_deref().and_then(|head| head.strip_prefix("ref: ")) {
        println!("cargo:rerun-if-changed=.git/{reference}");
    }
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|root| root.trim().to_string());
    let sha = root
        .filter(|root| std::path::Path::new(root) == std::path::Path::new(&manifest_dir))
        .and_then(|_| {
            Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|sha| sha.trim().to_string())
                .filter(|sha| !sha.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DEPRADAR_GIT_SHA={sha}");
}
