//! Stamps the commit and branch this binary was built from into it. The commit lets a machine say
//! whether the bus it runs is the bus the repository states: `swarm --version` prints it. The branch
//! picks the default swarm home (ADR 0027, `paths::branch_home`): only a release build, which
//! release.yml marks with SWARM_RELEASE_BUILD=1, records `""` and so uses `~/.swarm`.
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
    if std::env::var_os("CARGO_FEATURE_SERVE").is_some() {
        embed_web();
    }
}

/// Writes `$OUT_DIR/web_assets.rs`, a list of `(path, include_bytes!(..))` for every file in
/// `web/dist`, so `swarm serve` and `swarm ui` carry the web UI (ADR 0079). A dev build with no
/// `web/dist` embeds nothing; a release build refuses, so no release ships without the UI.
fn embed_web() {
    use std::path::{Path, PathBuf};
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, files);
            } else {
                files.push(path);
            }
        }
    }
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    let mut files = Vec::new();
    if dist.join("index.html").is_file() {
        walk(&dist, &mut files);
    } else if std::env::var("PROFILE").as_deref() == Ok("release") {
        panic!(
            "{} is missing; run `pnpm --dir web install && pnpm --dir web build` first",
            dist.join("index.html").display()
        );
    }
    files.sort();
    let mut out = String::from("&[\n");
    for file in &files {
        let name = file.strip_prefix(&dist).unwrap().to_str().unwrap();
        let file = file.to_str().unwrap();
        out.push_str(&format!("    ({name:?}, include_bytes!({file:?})),\n"));
    }
    out.push_str("]\n");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(Path::new(&out_dir).join("web_assets.rs"), out).unwrap();
}
