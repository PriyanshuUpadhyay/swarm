//! G3 keeps the existing route guards, then checks the captured Claude journey.
use super::s9a_frozen as frozen;
use super::*;
include!("legacy_body.rs");

mod captured {
    use super::*;
    struct Server(super::super::Server);
    impl std::ops::Deref for Server {
        type Target = super::super::Server;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl Server {
        fn new() -> Self {
            Self(super::super::Server::new())
        }
    }
    include!("../s9a/routes_switch.rs");
}

pub(super) fn journey() {
    captured::switch_journey();
    let mut expected = frozen::expected(&["claude-chair-run/", "follow-claude-chair-run/"]);
    // Reopen must keep the already captured final page, with the same session and source ids.
    expected["follow-claude-chair-run/page-03-reopen.txt"] =
        expected["follow-claude-chair-run/page-02-final.txt"].clone();
    let actual = without_body(captured::result());
    assert_eq!(actual, expected);
}
