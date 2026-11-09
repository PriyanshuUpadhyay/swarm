//! G3 checks the AGY switch against OLD public values.
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
            Self(super::super::Server::new())
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
