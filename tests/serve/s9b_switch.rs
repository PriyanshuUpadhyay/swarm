//! G3 checks the Codex switch against OLD public values.
use super::*;
#[path = "../s9b/frozen.rs"]
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
    impl Server {
        pub(super) fn new() -> Self {
            Self(super::super::Server::new())
        }
    }
    include!("../s9b/routes_all.rs");
    pub(super) fn journey() {
        all();
        assert_eq!(
            result(),
            frozen::expected(&[
                "codex-",
                "kernel-",
                "question-cancel/",
                "follow-",
                "overlap-",
                "account-",
                "origin-",
                "prefix-",
                "pending-origin-"
            ])
        );
    }
}
#[test]
fn g3_codex_switch_matches_the_old_public_values_without_zig() {
    captured::journey();
}
