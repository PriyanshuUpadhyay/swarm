//! Stamps the commit and branch this binary was built from into it. The commit lets a machine say
//! whether the bus it runs is the bus the repository states: `swarm --version` prints it, and the
//! app's PATH swarm check compares that line with its own helper's (ADR 0048). The branch picks the
//! default swarm home (ADR 0027, `paths::branch_home`): only a release build, which release.yml
//! marks with SWARM_RELEASE_BUILD=1, records `""` and so uses `~/.swarm`.
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
    // A dev build is never "": a detached HEAD, or a folder git cannot read, is "HEAD", which git
    // refuses as a branch name. So a local build of main or of a tag cannot migrate `~/.swarm`.
    println!("cargo:rerun-if-env-changed=SWARM_RELEASE_BUILD");
    let branch = if std::env::var("SWARM_RELEASE_BUILD").as_deref() == Ok("1") {
        String::new()
    } else {
        git(&["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_else(|| "HEAD".into())
    };
    println!("cargo:rustc-env=SWARM_BUILD_BRANCH={branch}");
}
