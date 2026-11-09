use super::*;
#[path = "../s9d/frozen.rs"]
mod frozen;
struct Server(super::Server);
impl std::ops::Deref for Server {
    type Target = super::Server;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Server {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Server {
    fn new() -> Self {
        let mut s = super::Server::with_actions(true);
        s.child.kill().unwrap();
        s.child.wait().unwrap();
        let home = s.dir.join("home");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_swarm"));
        cmd.env_clear()
            .env("HOME", &home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .env("SWARM_HOME", &home)
            .env("GIT_CEILING_DIRECTORIES", "/")
            .env("SWARM_TRANSCRIPT_TOOL", s.dir.join("unavailable-zig"))
            .args(["serve", "--port", "0", "--assets"])
            .arg(s.dir.join("assets"))
            .args(["--dev-origin", "http://127.0.0.1:5173"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        // Discovery uses the same synthetic homes as the G2 process.
        cmd.env("HOME", std::env::var_os("HOME").unwrap());
        for key in ["PI_CODING_AGENT_DIR", "PI_SESSION_FILE"] {
            if let Some(value) = std::env::var_os(key) {
                cmd.env(key, value);
            }
        }
        s.child = cmd.spawn().unwrap();
        let mut first = String::new();
        BufReader::new(s.child.stdout.take().unwrap())
            .read_line(&mut first)
            .unwrap();
        let (address, token) = first
            .trim()
            .split_once("/#token=")
            .expect("real server URL");
        s.port = address.rsplit_once(':').unwrap().1.parse().unwrap();
        s.token = token.into();
        Self(s)
    }
}
include!("../s9d/routes_all.rs");
#[test]
fn g3_pi_routes_work_without_zig() {
    run_modes();
}
