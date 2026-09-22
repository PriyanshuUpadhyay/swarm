//! Stamps the commit this binary was built from into it, so a machine can say whether the bus it
//! runs is the bus the repository states. `swarm --version` prints it, and `ui/Tools/build.sh`
//! refuses to assemble the app when the installed binary answers with another commit.
fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=SWARM_BUILD_COMMIT={commit}");
}
