//! Test the text a running turn shows.

mod tests {
    use super::super::*;

    #[test]
    fn each_cli_shows_the_last_block_of_its_turn() {
        let claude = turn_text("claude", fixture!("claude-turn-text"), None).unwrap();
        assert!(
            claude.starts_with("In the 1860s, French makers"),
            "{claude}"
        );
        assert!(claude.ends_with("clean city\ntravel."), "{claude}");
        assert!(!claude.contains("Sautéed") && !claude.contains("Plugins"));
        let codex = turn_text("codex", fixture!("codex-turn-text"), None).unwrap();
        assert!(codex.starts_with("A rider on an early bicycle"), "{codex}");
        assert!(codex.ends_with("rough ride, which"), "{codex}");
        let pi = turn_text("pi", fixture!("pi-turn-text"), None).unwrap();
        assert!(pi.starts_with("I should aim for plain"), "{pi}");
        assert_eq!(
            turn_text("agy", fixture!("agy-turn-spinner"), None).as_deref(),
            Some("Initial draft focuses on a concise history, starting with the draisine, ev...")
        );
    }

    #[test]
    fn a_screen_that_still_shows_the_ended_turn_shows_nothing() {
        let claude = fixture!("claude-turn-text").replace(
            "✻ Sautéing… (11s · esc to interrupt)",
            "✻ Sautéed for 11s · done 6:52 PM",
        );
        assert_eq!(turn_text("claude", &claude, None), None);
        let codex = fixture!("codex-turn-text")
            .replace("\n› Ask Codex", "  Worked for 12s • 6:52 PM\n› Ask Codex");
        assert_eq!(turn_text("codex", &codex, None), None);
    }

    #[test]
    fn pi_text_with_no_tool_starts_under_the_sent_message() {
        let sent = "Run the shell command ls docs once, then without more tools write about 200 \
                    words on the history of the bicycle as plain paragraphs.";
        let screen = fixture!("pi-turn-start").replace(
            "\n── ⠦ Working",
            "\n The bicycle began in 1817.\n\n It spread fast.\n\n── ⠦ Working",
        );
        assert_eq!(
            turn_text("pi", &screen, Some(sent)).as_deref(),
            Some("The bicycle began in 1817.\n\nIt spread fast.")
        );
        // Before pi echoes the message, the screen holds only the ended turn.
        assert_eq!(
            turn_text("pi", &screen, Some("Write about the kite.")),
            None
        );
    }

    #[test]
    fn a_tool_call_or_an_unread_screen_shows_nothing() {
        assert_eq!(
            turn_text("claude", fixture!("claude-turn-tool"), None),
            None
        );
        assert_eq!(turn_text("codex", fixture!("codex-turn-tool"), None), None);
        // pi's own message has no mark, so a turn shows nothing before its first tool.
        assert_eq!(turn_text("pi", fixture!("pi-turn-start"), None), None);
        assert_eq!(turn_text("claude", "$ claude --model haiku\n", None), None);
        // Before the first block, the rows above the box are the rest of the sent message.
        let sent = "❯ Run the shell command ls docs once, then\n  write about the railway.\n\n\
                    ✢ Synthesizing… (1s)\n────\n❯ \n────\n";
        assert_eq!(turn_text("claude", sent, None), None);
        assert_eq!(turn_text("shell", fixture!("claude-turn-text"), None), None);
    }
}
