//! Test the text a running turn shows.

mod tests {
    use super::super::*;

    #[test]
    fn each_cli_shows_the_last_block_of_its_turn() {
        let claude = turn_text("claude", fixture!("claude-turn-text")).unwrap();
        assert!(
            claude.starts_with("In the 1860s, French makers"),
            "{claude}"
        );
        assert!(claude.ends_with("clean city\ntravel."), "{claude}");
        assert!(!claude.contains("Sautéed") && !claude.contains("Plugins"));
        let codex = turn_text("codex", fixture!("codex-turn-text")).unwrap();
        assert!(codex.starts_with("A rider on an early bicycle"), "{codex}");
        assert!(codex.ends_with("rough ride, which"), "{codex}");
        assert_eq!(
            turn_text("agy", fixture!("agy-turn-spinner")).as_deref(),
            Some("Initial draft focuses on a concise history, starting with the draisine, ev...")
        );
    }

    #[test]
    fn a_screen_that_still_shows_the_ended_turn_shows_nothing() {
        let claude = fixture!("claude-turn-text").replace(
            "✻ Sautéing… (11s · esc to interrupt)",
            "✻ Sautéed for 11s · done 6:52 PM",
        );
        assert_eq!(turn_text("claude", &claude), None);
        let codex = fixture!("codex-turn-text")
            .replace("\n› Ask Codex", "  Worked for 12s • 6:52 PM\n› Ask Codex");
        assert_eq!(turn_text("codex", &codex), None);
    }

    #[test]
    fn a_message_queued_while_a_turn_runs_never_enters_the_live_text() {
        // Claude and Codex echo a queued message into the turn, so the cut starts under it.
        let claude = turn_text("claude", fixture!("claude-queued")).unwrap();
        assert!(claude.starts_with("The bicycle developed"), "{claude}");
        assert!(!claude.contains("Queued note"), "{claude}");
        // Codex has drawn no block since the queued message, only its working row.
        assert_eq!(turn_text("codex", fixture!("codex-queued")), None);
    }

    #[test]
    fn a_pi_screen_shows_nothing_because_pi_text_comes_from_its_extension() {
        // A running bash box, its clock, queued messages, and earlier text are all on this screen.
        assert_eq!(turn_text("pi", fixture!("pi-queued")), None);
        assert_eq!(turn_text("pi", fixture!("pi-turn-text")), None);
    }

    #[test]
    fn a_tool_call_or_an_unread_screen_shows_nothing() {
        assert_eq!(turn_text("claude", fixture!("claude-turn-tool")), None);
        assert_eq!(turn_text("codex", fixture!("codex-turn-tool")), None);
        assert_eq!(turn_text("claude", "$ claude --model haiku\n"), None);
        // Before the first block, the rows above the box are the rest of the sent message.
        let sent = "❯ Run the shell command ls docs once, then\n  write about the railway.\n\n\
                    ✢ Synthesizing… (1s)\n────\n❯ \n────\n";
        assert_eq!(turn_text("claude", sent), None);
        assert_eq!(turn_text("shell", fixture!("claude-turn-text")), None);
    }
}
