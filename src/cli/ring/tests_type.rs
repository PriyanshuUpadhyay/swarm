//! Tests for typing the owner's text, and for emptying pi's input box.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    /// A fake pi pane that draws its input box on the third read, keeps the text unsent in the
    /// box as a starting pi does, and starts a turn on Enter.
    #[test]
    fn typed_text_waits_for_the_input_box_and_presses_enter_until_it_sends() {
        let (dir, connection, session) = ring_session("type-pane", Some("pi"), "", "$ pi\n");
        let box_rows = |text: &str| format!("────\n{text}\n────\n~/project\n");
        std::fs::write(dir.join("empty"), box_rows("")).unwrap();
        std::fs::write(
            dir.join("working"),
            "hi\n── ⠏ Working ──\n\n────\n~/project\n",
        )
        .unwrap();
        let d = dir.display();
        let adapter = swarm::adapter::parse(
            "fake",
            &format!(
                "self = true\nspawn = true\nlist = true\nclose = true\n\
                 capture = n=$(($(cat '{d}/reads' 2>/dev/null || echo 0) + 1)); echo $n > '{d}/reads'; \
                 [ $n = 3 ] && cp '{d}/empty' '{d}/screen'; echo read >> '{d}/log'; cat '{d}/screen'\n\
                 ring = echo ring >> '{d}/log'; printf '────\\n%s\\n────\\n~/project\\n' \"$SWARM_TEXT\" > '{d}/screen'\n\
                 key = echo \"$SWARM_KEY\" >> '{d}/log'; cp '{d}/working' '{d}/screen'\n"
            ),
        )
        .unwrap();
        type_text(&connection, &adapter, &session, CODER, "%2", "hi").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "read\nread\nread\nread\nring\nread\nEnter\nread\n"
        );
        // A pi that drew its box before the send takes the text and its Enter, so no read follows.
        std::fs::remove_file(dir.join("log")).unwrap();
        std::fs::copy(dir.join("empty"), dir.join("screen")).unwrap();
        type_text(&connection, &adapter, &session, CODER, "%2", "hi").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "read\nread\nring\n"
        );
        // pi puts queued text back in its box on a stop, and a send would join it.
        std::fs::remove_file(dir.join("log")).unwrap();
        std::fs::write(dir.join("screen"), box_rows("steer four")).unwrap();
        let refused = type_text(&connection, &adapter, &session, CODER, "%2", "hi").unwrap_err();
        assert!(
            refused.to_string().contains("input box holds text"),
            "{refused}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "read\nread\n"
        );
        // pi's folder trust screen is a question, so typed text and its Enter would pick a choice.
        std::fs::write(
            dir.join("screen"),
            include_str!("../../../tests/fixtures/screens/pi-trust.txt"),
        )
        .unwrap();
        let refused = type_text(&connection, &adapter, &session, CODER, "%2", "hi").unwrap_err();
        assert!(
            refused.to_string().contains("shows a question"),
            "{refused}"
        );
        assert!(
            !std::fs::read_to_string(dir.join("log"))
                .unwrap()
                .contains("ring")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A fake pi box of lines: M-Up puts the queue in it, and BSpace takes the last line, as the
    /// C-e, C-u, BSpace round does in pi.
    #[test]
    fn clear_box_takes_the_queue_back_and_empties_the_box() {
        let dir = std::env::temp_dir().join(format!("swarm-clear-box-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.display();
        std::fs::write(dir.join("queue"), "steer one\nsteer two\n").unwrap();
        std::fs::write(dir.join("lines"), "").unwrap();
        let adapter = swarm::adapter::parse(
            "fake",
            &format!(
                "self = true\nspawn = true\nlist = true\nclose = true\nring = true\n\
                 capture = [ -e '{d}/broken' ] && exit 0; printf '────\\n'; cat '{d}/lines'; printf '────\\n~/project\\n'\n\
                 key = echo \"$SWARM_KEY\" >> '{d}/log'; case \"$SWARM_KEY\" in M-Up) cp '{d}/queue' '{d}/lines' ;; BSpace) sed -i.bak '$d' '{d}/lines' ;; esac\n"
            ),
        )
        .unwrap();
        assert_eq!(
            clear_box(&adapter, "%2", true).unwrap(),
            "cleared\nsteer one steer two"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("log")).unwrap(),
            "M-Up\nC-e\nC-u\nBSpace\nC-e\nC-u\nBSpace\n"
        );
        // Nothing queued: the box stays empty, and no other key is pressed.
        std::fs::remove_file(dir.join("log")).unwrap();
        std::fs::write(dir.join("queue"), "").unwrap();
        assert_eq!(clear_box(&adapter, "%2", true).unwrap(), "empty");
        assert_eq!(std::fs::read_to_string(dir.join("log")).unwrap(), "M-Up\n");
        // A box swarm cannot read is never cleared blind.
        std::fs::remove_file(dir.join("log")).unwrap();
        std::fs::write(dir.join("lines"), "kept\n").unwrap();
        std::fs::write(dir.join("broken"), "").unwrap();
        assert!(clear_box(&adapter, "%2", false).is_err());
        assert!(!dir.join("log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
