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
        let pi = turn_text("pi", fixture!("pi-turn-text")).unwrap();
        assert!(pi.starts_with("I should aim for plain"), "{pi}");
        assert_eq!(
            turn_text("agy", fixture!("agy-turn-spinner")).as_deref(),
            Some("Initial draft focuses on a concise history, starting with the draisine, ev...")
        );
    }

    #[test]
    fn a_tool_call_or_an_unread_screen_shows_nothing() {
        assert_eq!(turn_text("claude", fixture!("claude-turn-tool")), None);
        assert_eq!(turn_text("codex", fixture!("codex-turn-tool")), None);
        // pi's own message has no mark, so a turn shows nothing before its first tool.
        assert_eq!(turn_text("pi", fixture!("pi-turn-start")), None);
        assert_eq!(turn_text("claude", "$ claude --model haiku\n"), None);
        // Before the first block, the rows above the box are the rest of the sent message.
        let sent = "❯ Run the shell command ls docs once, then\n  write about the railway.\n\n\
                    ✢ Synthesizing… (1s)\n────\n❯ \n────\n";
        assert_eq!(turn_text("claude", sent), None);
        assert_eq!(turn_text("shell", fixture!("claude-turn-text")), None);
    }
}
