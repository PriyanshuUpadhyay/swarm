//! Test provider screen classification and pane reads.

mod tests {
    use super::super::*;

    #[test]
    fn classifies_every_fixture() {
        use ScreenState::{Idle, Waiting, Working};
        let cases = [
            ("claude", fixture!("claude-idle"), Some(Idle)),
            ("claude", fixture!("claude-done"), Some(Idle)),
            ("claude", fixture!("claude-after-esc"), Some(Idle)),
            ("claude", fixture!("claude-waiting"), Some(Waiting)),
            ("claude", fixture!("claude-submit-answers"), Some(Waiting)),
            ("claude", fixture!("claude-working"), Some(Working)),
            ("codex", fixture!("codex-idle"), Some(Idle)),
            ("codex", fixture!("codex-waiting"), Some(Waiting)),
            ("codex", fixture!("codex-working"), Some(Working)),
            ("agy", fixture!("agy-idle"), Some(Idle)),
            ("agy", fixture!("agy-waiting"), Some(Waiting)),
            ("agy", fixture!("agy-working"), Some(Working)),
            ("pi", fixture!("pi-idle"), Some(Idle)),
            ("pi", fixture!("pi-trust"), Some(Waiting)),
            ("pi", fixture!("pi-ring-unsent"), Some(Idle)),
            ("pi", fixture!("pi-working"), Some(Working)),
            ("pi", "$ pi --model x\n", None),
            ("claude", "", None),
            ("claude", "$ ls\nsrc\n", None),
            ("gemini", fixture!("claude-idle"), None),
        ];
        for (provider, rows, expected) in cases {
            assert_eq!(
                screen_state(provider, rows),
                expected,
                "{provider}:\n{rows}"
            );
        }
    }

    #[test]
    fn a_claude_answer_with_no_end_row_under_its_echo_is_working() {
        // Claude hides its spinner row while it writes the answer.
        let writing = "❯ Write about the windmill.\n\n⏺ The first windmills ground grain.\n\n────\n❯ \n────\n";
        assert_eq!(screen_state("claude", writing), Some(ScreenState::Working));
        let ended = writing.replace("\n\n────", "\n✻ Sautéed for 9s · done 1:34 AM\n────");
        assert_eq!(screen_state("claude", &ended), Some(ScreenState::Idle));
    }

    #[test]
    fn a_codex_answer_with_no_end_row_under_its_echo_is_working() {
        // Codex hides its working row while it writes the answer.
        let writing = fixture!("codex-turn-text");
        assert_eq!(screen_state("codex", writing), Some(ScreenState::Working));
        let ended = writing.replace("\n› Ask Codex", "  Worked for 12s • 6:52 PM\n› Ask Codex");
        assert_eq!(screen_state("codex", &ended), Some(ScreenState::Idle));
    }

    #[test]
    fn a_codex_error_row_above_the_idle_composer_is_a_failure_with_its_message() {
        let rows = fixture!("codex-failed");
        assert_eq!(screen_state("codex", rows), Some(ScreenState::Failed));
        assert_eq!(
            failure_detail("codex", rows).as_deref(),
            Some("stream disconnected before completion: rate limit reached for gpt-5.5-codex")
        );
        // Output after the error means a later turn went on.
        let recovered = rows.replace("\n\n› Ask", "\n\n• Retried and passed.\n\n› Ask");
        assert_eq!(screen_state("codex", &recovered), Some(ScreenState::Idle));
        assert_eq!(failure_detail("codex", fixture!("codex-idle")), None);
        assert_eq!(
            failure_detail("codex", "› prompt\n■ error after the composer\n"),
            None
        );
        let stopped =
            "■ Conversation interrupted - tell the model what to do differently.\n\n› Ask\n";
        assert_eq!(screen_state("codex", stopped), Some(ScreenState::Idle));
        assert_eq!(failure_detail("codex", stopped), None);
        assert_eq!(
            resolve(
                Some("codex"),
                Some("done"),
                Some(970),
                Some("hook"),
                Some(ScreenState::Failed),
                1_000
            ),
            (Some("failed".into()), Some("failed"))
        );
    }

    #[test]
    fn an_idle_herdr_status_still_reads_the_pane_for_a_codex_error_row() {
        let idle = r#"{"result":{"agent":{"agent_status":"idle"}}}"#;
        let working = r#"{"result":{"agent":{"agent_status":"working"}}}"#;
        assert_eq!(
            read_pane(Some("codex"), idle, || Some(
                fixture!("codex-failed").to_string()
            )),
            Some((
                ScreenState::Failed,
                Some(
                    "stream disconnected before completion: rate limit reached for gpt-5.5-codex"
                        .into()
                )
            ))
        );
        assert_eq!(
            read_pane(Some("codex"), idle, || Some(
                fixture!("codex-idle").to_string()
            )),
            Some((ScreenState::Idle, None))
        );
        // A failed or late text read proves nothing, so the pane is unknown, not idle, and a
        // failure the screen stored stays with nothing written.
        assert_eq!(read_pane(Some("codex"), idle, || None), None);
        let unknown = read_pane(Some("codex"), idle, || None).map(|(state, _)| state);
        assert_eq!(
            resolve(
                Some("codex"),
                Some("failed"),
                Some(900),
                Some("screen"),
                unknown,
                1_000
            ),
            (Some("failed".into()), None)
        );
        // Only Codex has an error row to look for; other providers keep Herdr's idle.
        let no_read = || -> Option<String> { panic!("no text read for claude") };
        assert_eq!(
            read_pane(Some("claude"), idle, no_read),
            Some((ScreenState::Idle, None))
        );
        let never = || -> Option<String> { panic!("a working pane needs no text read") };
        assert_eq!(
            read_pane(Some("codex"), working, never),
            Some((ScreenState::Working, None))
        );
        // A text screen verb is classified as before.
        assert_eq!(
            read_pane(Some("codex"), fixture!("codex-failed"), || None).map(|(state, _)| state),
            Some(ScreenState::Failed)
        );
    }

    #[test]
    fn a_numbered_draft_on_the_input_line_is_the_idle_prompt() {
        // Build the draft from the fixture's own input line, which has U+00A0 after the ❯.
        let fixture = fixture!("claude-idle");
        let input = fixture.lines().find(|line| line.starts_with('❯')).unwrap();
        let claude = fixture.replace(input, "❯\u{a0}1. rerun the tests");
        assert_ne!(claude, fixture);
        assert_eq!(screen_state("claude", &claude), Some(ScreenState::Idle));
        let asked = format!("● Do you want to add a test?\n\n{claude}");
        assert_eq!(screen_state("claude", &asked), Some(ScreenState::Idle));
        let codex =
            fixture!("codex-idle").replace("› Ask Codex to do anything", "› 1. rerun the tests");
        assert_ne!(codex, fixture!("codex-idle"));
        assert_eq!(screen_state("codex", &codex), Some(ScreenState::Idle));
        let asked = format!("• Would you like to run the suite next?\n{codex}");
        assert_eq!(screen_state("codex", &asked), Some(ScreenState::Idle));
        // Real approvals still wait.
        assert_eq!(
            screen_state("claude", fixture!("claude-waiting")),
            Some(ScreenState::Waiting)
        );
        assert_eq!(
            screen_state("codex", fixture!("codex-waiting")),
            Some(ScreenState::Waiting)
        );
    }

    #[test]
    fn a_codex_answer_with_a_numbered_list_and_a_numbered_draft_is_idle() {
        let rows = "• Would you like to:\n  1. Fix the flaky test\n  2. Rerun the suite\n\n› 1. Fix the flaky test\n\n  ? for shortcuts                                100% context left\n";
        assert_eq!(screen_state("codex", rows), Some(ScreenState::Idle));
    }

    #[test]
    fn a_question_in_an_answer_above_the_idle_prompt_is_not_an_approval() {
        let answer = "● Done. Do you want to add a test for it?\n\n";
        let rows = format!("{answer}{}", fixture!("claude-idle"));
        assert_eq!(screen_state("claude", &rows), Some(ScreenState::Idle));
        let codex = format!(
            "• Would you like to run the suite next?\n{}",
            fixture!("codex-idle")
        );
        assert_eq!(screen_state("codex", &codex), Some(ScreenState::Idle));
        // Codex's request for input keeps its footer on one row, with no choice rows.
        let asking = "  Which branch should I use?\n\n  enter to submit answer · esc to cancel\n";
        assert_eq!(screen_state("codex", asking), Some(ScreenState::Waiting));
    }

    #[test]
    fn reads_only_the_bottom_rows() {
        let old_prompt = format!("{}{}", fixture!("claude-waiting"), "text\n".repeat(ROWS));
        assert_eq!(screen_state("claude", &old_prompt), None);
    }

    #[test]
    fn maps_herdr_agent_status() {
        let status = |value: &str| {
            herdr_state(&format!(
                r#"{{"result":{{"agent":{{"agent_status":"{value}"}}}}}}"#
            ))
        };
        assert_eq!(status("idle"), Some(ScreenState::Idle));
        assert_eq!(status("working"), Some(ScreenState::Working));
        assert_eq!(status("blocked"), Some(ScreenState::Waiting));
        assert_eq!(status("unknown"), None);
        assert_eq!(herdr_state("not json"), None);
    }
}
