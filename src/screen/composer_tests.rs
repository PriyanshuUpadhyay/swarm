//! Test provider input boxes and unsent text.

mod tests {
    use super::super::*;

    #[test]
    fn claude_input_box_holds_only_an_unsent_ring() {
        let ring = "swarm: new message. Run swarm inbox and read each body at \
                    /Users/x/.swarm/<body_path>. Run swarm ack <seq> only after you finish that message.";
        let unsent = fixture!("claude-ring-unsent");
        assert!(holds("claude", unsent, ring));
        // The box wraps a long ring onto a second row.
        let wrapped = unsent.replace(" Run swarm ack <seq>", " Run swarm ack\n  <seq>");
        assert!(holds("claude", &wrapped, ring));
        // A sent ring stays in the history above an empty box, which shows only its hint.
        assert_eq!(
            claude_composer(fixture!("claude-idle")).as_deref(),
            Some("Try \"fix typecheck errors\"")
        );
        assert!(!holds(
            "claude",
            &format!("❯ {ring}\n{}", fixture!("claude-idle")),
            ring
        ));
        // A starting CLI has no box yet, and a question is never a held ring.
        assert_eq!(claude_composer("$ claude --model haiku\n"), None);
        assert!(!holds("claude", fixture!("claude-waiting"), ring));
    }

    #[test]
    fn pi_input_box_holds_only_an_unsent_ring() {
        let ring = "Reply with only the word ok.";
        assert!(reads_composer("pi"));
        assert!(holds("pi", fixture!("pi-ring-unsent"), ring));
        // The box is empty while idle and once the ring is sent, even though the sent text stays
        // in the history above the box.
        assert_eq!(composer("pi", fixture!("pi-idle")).as_deref(), Some(""));
        assert_eq!(composer("pi", fixture!("pi-working")).as_deref(), Some(""));
        assert!(!holds("pi", fixture!("pi-working"), ring));
        // A starting pi has no box yet.
        assert_eq!(composer("pi", "$ pi --model x\n"), None);
    }
}
