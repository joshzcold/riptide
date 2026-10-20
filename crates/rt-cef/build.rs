// Embeds the git commit so --version and the help page can show it.
// Builds without a checkout (such as Nix's) can pass it in RT_GIT_COMMIT.
fn main() {
    let commit = std::env::var("RT_GIT_COMMIT")
        .ok()
        .filter(|c| !c.is_empty())
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short=10", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo::rustc-env=RT_GIT_COMMIT={commit}");
    println!("cargo::rerun-if-env-changed=RT_GIT_COMMIT");
    println!("cargo::rerun-if-changed=../../.git/HEAD");
    println!("cargo::rerun-if-changed=../../.git/refs/heads");
}
