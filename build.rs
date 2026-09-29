//! Stamps the commit and branch this binary was built from into it. The commit lets a machine say
//! whether the bus it runs is the bus the repository states: `swarm --version` prints it, and
//! `ui/Tools/build.sh` refuses to assemble the app when the installed binary answers with another
//! commit. The branch picks the default swarm home (ADR 0027, `paths::resolve_home`).
fn git(args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    // A linked worktree keeps HEAD in its own git dir and refs in the common one, so ask git where
    // they are instead of assuming a `.git` directory.
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
    }
    if let Some(dir) = git(&["rev-parse", "--git-common-dir"]) {
        println!("cargo:rerun-if-changed={dir}/refs");
    }
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=SWARM_BUILD_COMMIT={commit}");
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    println!("cargo:rustc-env=SWARM_BUILD_BRANCH={branch}");
}
