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

    #[test]
    fn pi_queue_lists_the_queued_lines_above_its_box() {
        let queued = |mode: &str, text: &str| QueuedMessage {
            mode: mode.into(),
            text: text.into(),
        };
        // pi cuts a long line to the pane's width.
        assert_eq!(
            pi_queue(fixture!("pi-queued")),
            Some(vec![
                queued("steering", "first queued note"),
                queued(
                    "steering",
                    "second queued note that is quite long so that it will need to be cut by the truncated..."
                ),
            ])
        );
        let follow_up = fixture!("pi-queued")
            .replace(" ↳ Option+Up", " Follow-up: after the turn\n ↳ Option+Up");
        assert_eq!(
            pi_queue(&follow_up).unwrap()[2],
            queued("follow-up", "after the turn")
        );
        // A queued line that the history quotes is not in the queue.
        let quoted = format!(" Steering: a quoted line\n{}", fixture!("pi-queued"));
        assert_eq!(pi_queue(&quoted).unwrap().len(), 2);
        // A hint that the history quotes, as when pi prints this fixture, is not pi's own hint.
        let rows: Vec<&str> = fixture!("pi-queued").lines().collect();
        let printed = format!(
            "{}\n Reply\n{}",
            rows[..rows.len() - 6].join("\n"),
            fixture!("pi-idle")
        );
        assert_eq!(pi_queue(&printed), Some(vec![]));
        // With no hint line nothing is queued.
        assert_eq!(pi_queue(fixture!("pi-idle")), Some(vec![]));
        assert_eq!(pi_queue(&quoted.replace("↳ ", "")), Some(vec![]));
        // A starting pi has no box, so its queue is not read.
        assert_eq!(pi_queue("$ pi --model x\n"), None);
    }
}
