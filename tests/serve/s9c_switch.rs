//! G3 checks the AGY switch with the Zig executable absent.
use super::*;
#[path = "../s9c/frozen.rs"]
mod frozen;
mod captured {
    use super::*;
    pub(super) struct Server(super::super::Server);
    impl std::ops::Deref for Server {
        type Target = super::super::Server;
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
        pub(super) fn new() -> Self {
            let mut server = super::super::Server::new();
            // Only this test's child is restarted. The existing fixture and real binary stay in use.
            server.child.kill().unwrap();
            server.child.wait().unwrap();
            let home = server.dir.join("home");
            let mut child = Command::new(env!("CARGO_BIN_EXE_swarm"))
                .env_clear()
                .env("HOME", &home)
                .env(
                    "PATH",
                    format!("{}:/usr/bin:/bin", home.join("bin").display()),
                )
                .env("GIT_CEILING_DIRECTORIES", "/")
                .env("SWARM_HOME", &home)
                .env("SWARM_TRANSCRIPT_TOOL", server.dir.join("unavailable-zig"))
                .args(["serve", "--port", "0", "--assets"])
                .arg(server.dir.join("assets"))
                .args(["--dev-origin", "http://127.0.0.1:5173"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let mut first = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut first)
                .unwrap();
            server.child = child;
            let (address, token) = first.trim().split_once("/#token=").expect("real serve URL");
            server.port = address.rsplit_once(':').unwrap().1.parse().unwrap();
            server.token = token.to_owned();
            Self(server)
        }
    }
    include!("../s9c/routes_all.rs");
    pub(super) fn journey() {
        all();
        assert_eq!(result(), frozen::expected(&["routes.json"])["routes.json"]);
    }
}
#[test]
fn g3_agy_switch_matches_the_old_public_values_without_zig() {
    captured::journey();
}
