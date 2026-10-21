//! Tests for server ui.

use super::*;

#[test]
fn the_ui_record_names_the_url_then_the_pid_and_the_token_lasts() {
    let root = std::env::temp_dir().join(format!("swarm-ui-record-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let record = root.join("ui-url");
    std::fs::write(
        &record,
        format!("http://127.0.0.1:4747/#token=ab\n42\n{BUILD}\n"),
    )
    .unwrap();
    assert_eq!(
        recorded(&record),
        Some((
            "http://127.0.0.1:4747/#token=ab".into(),
            Some(42),
            Some(BUILD.into())
        ))
    );
    // A record from an older swarm has no build line, so `swarm ui` replaces its server.
    std::fs::write(&record, "http://127.0.0.1:4747/#token=ab\n42\n").unwrap();
    assert_eq!(recorded(&record).unwrap().2, None);
    // An even older record has no pid line either.
    std::fs::write(&record, "http://127.0.0.1:4747/#token=ab\n").unwrap();
    assert_eq!(recorded(&record).unwrap().1, None);
    assert_eq!(
        page_base("http://127.0.0.1:4747/#token=ab"),
        "http://127.0.0.1:4747/"
    );
    let token = ui_token(&root).unwrap();
    assert_eq!(ui_token(&root).unwrap(), token);
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(root.join("ui-token"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_notice_click_opens_the_chat_path_and_refuses_a_bad_id() {
    assert_eq!(
        chat_url("http://127.0.0.1:4747/#token=ab", "01a1-b2"),
        "http://127.0.0.1:4747/chat/01a1-b2#token=ab"
    );
    assert!(valid_session("01a15460-dbe3-7372-88ad-662dc0d93cbd"));
    for bad in ["", "../x", "a/b", "a#b", &"a".repeat(65)] {
        assert!(!valid_session(bad), "{bad}");
    }
}
