//! Test prompt questions, choices, and key mapping.

mod tests {
    use super::super::*;

    #[test]
    fn every_prompt_shape_gives_its_question_choices_cursor_and_keys() {
        let cases = [
            (
                fixture!("claude-waiting"),
                "Bash command\n│ touch /tmp/work/probe.txt\nCreate empty probe file\nDo you want to proceed?",
                &[
                    "Yes",
                    "Yes, and always allow access to\n/tmp/work from this project",
                    "No",
                ][..],
                true,
            ),
            (
                fixture!("claude-question"),
                "☐ Color\nWhich color do you prefer?",
                &[
                    "Red\nThe color red",
                    "Blue\nThe color blue",
                    "Type something.",
                    "Chat about this",
                ],
                true,
            ),
            (
                fixture!("codex-waiting"),
                "Would you like to run the following command?\nEnvironment: local\n\
                 Reason: Do you approve running touch probe.txt outside the sandbox?\n$ touch probe.txt",
                &[
                    "Yes, proceed (y)",
                    "Yes, and don't ask again for commands that start with `touch probe.txt` (p)",
                    "No, and tell Codex what to do differently (esc)",
                ],
                true,
            ),
            (
                fixture!("codex-trust"),
                "Folder access\n/tmp/work\nTrust this folder? Codex can read, edit, and run files here, \
                 subject to your permission settings. Folder\nsettings can run code automatically, even \
                 without a model request. Continue only if you trust these files.\nYour trust decision \
                 will be saved.",
                &["Trust and continue", "Back to Agent Command Center"],
                true,
            ),
            (
                fixture!("agy-question"),
                "Question 1/1: Which color do you choose?",
                &["Red", "Blue", "Write-in..."],
                true,
            ),
            (
                fixture!("agy-trust"),
                "Accessing workspace:\n/tmp/work\nDo you trust the contents of this project?\n\
                 Antigravity CLI requires permission to read, edit, and execute files here.",
                &["Yes, I trust this folder", "No, exit"],
                false,
            ),
            (
                fixture!("agy-waiting"),
                "Agent is requesting permission for: run_command\ntouch /tmp/work/probe.txt\n\
                 Do you want to proceed?",
                &["Yes", "No"],
                false,
            ),
            (
                // The last step of a Claude question with several tabs has no footer row, and its
                // cursor row is the bottom choice.
                fixture!("claude-submit-answers"),
                "←  ☒ Color  ☒ Size  ✔ Submit  →\nReview your answers\n● Which color?\n→ Red\n\
                 ● Which size?\n→ Small\nReady to submit your answers?",
                &["Submit answers", "Cancel"],
                true,
            ),
        ];
        for (rows, question, choices, numbered) in cases {
            let prompt = prompt(rows).unwrap_or_else(|| panic!("no prompt in:\n{rows}"));
            assert_eq!(prompt.question, question, "{rows}");
            assert_eq!(prompt.choices, choices, "{rows}");
            assert_eq!((prompt.numbered, prompt.cursor), (numbered, 0), "{rows}");
        }

        let claude = prompt(fixture!("claude-waiting")).unwrap();
        assert_eq!(claude.keys(2), Some(vec!["3".to_string()]));
        assert_eq!(claude.keys(3), None);
        let agy = prompt(fixture!("agy-trust")).unwrap();
        // AGY's trust screen opens with its own header row, so the card shows the whole question
        // even though no rule or history row sits above it.
        assert_eq!(
            whole_prompt(fixture!("agy-trust"), || None),
            Some(agy.clone())
        );
        let headless = fixture!("agy-trust").replace("Accessing workspace:", "");
        assert_eq!(whole_prompt(&headless, || None), None);
        assert_eq!(agy.keys(0), Some(vec!["Enter".to_string()]));
        assert_eq!(
            agy.keys(1),
            Some(vec!["Down".to_string(), "Enter".to_string()])
        );
        // A moved cursor changes the arrows, and the id, so a stale answer is refused.
        let moved = prompt(
            &fixture!("agy-trust")
                .replace("> Yes, I trust", "  Yes, I trust")
                .replace("  No, exit", "> No, exit"),
        )
        .unwrap();
        assert_eq!(moved.cursor, 1);
        assert_eq!(
            moved.keys(0),
            Some(vec!["Up".to_string(), "Enter".to_string()])
        );
        assert_eq!(moved.id, agy.id);
        let other = prompt(&fixture!("claude-waiting").replace("probe.txt", "other.txt")).unwrap();
        assert_ne!(other.id, claude.id);
    }

    #[test]
    fn every_row_of_a_long_command_is_in_the_question_and_its_id() {
        let command = |first: &str| {
            let rows: String = (1..=6).map(|n| format!("   echo step {n}\n")).collect();
            format!(
                "────\n Bash command\n\n   {first}\n{rows}\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel\n"
            )
        };
        let harmless = prompt(&command("ls /tmp/work")).unwrap();
        let deleting = prompt(&command("rm -rf /tmp/work")).unwrap();
        assert!(
            deleting.question.contains("rm -rf /tmp/work"),
            "{}",
            deleting.question
        );
        assert_ne!(deleting.id, harmless.id);
        assert!(!deleting.cut);

        // A question that reaches the top row is read again from the pane's history.
        let screen = command("rm -rf /tmp/work").replace("────\n Bash command\n\n", "");
        assert!(prompt(&screen).unwrap().cut);
        let history = format!("● I will clean up.\n Bash command\n{screen}");
        let whole = whole_prompt(&screen, || Some(history.clone())).unwrap();
        assert!(
            whole.question.starts_with("Bash command\nrm -rf"),
            "{}",
            whole.question
        );
        assert_eq!(whole_prompt(&screen, || None), None);
        // History that lost the question's first rows is refused too, also when it lost the
        // same rows as the screen (a pane that keeps no rows above it).
        let lost = format!("   echo step 0\n{screen}");
        assert_eq!(whole_prompt(&screen, || Some(lost)), None);
        assert_eq!(whole_prompt(&screen, || Some(screen.clone())), None);
        let never = || -> Option<String> { panic!("a whole question needs no history") };
        assert_eq!(whole_prompt(&command("ls"), never), prompt(&command("ls")));
    }

    #[test]
    fn a_blank_row_inside_a_command_at_the_top_row_does_not_end_the_question() {
        let command = |first: &str| {
            format!(
                "────\n Bash command\n\n   {first}\n\n   echo done\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n\n Esc to cancel\n"
            )
        };
        // The pane shows the rows from the blank row below the first command row.
        let screen = |first: &str| {
            let history = command(first);
            let start = history.find("\n\n   echo done").unwrap() + 1;
            history[start..].to_string()
        };
        assert!(prompt(&screen("rm -rf /tmp/work")).unwrap().cut);
        let whole = |first: &str| whole_prompt(&screen(first), || Some(command(first))).unwrap();
        assert!(
            whole("rm -rf /tmp/work")
                .question
                .contains("rm -rf /tmp/work"),
            "{}",
            whole("rm -rf /tmp/work").question
        );
        assert_ne!(whole("rm -rf /tmp/work").id, whole("ls /tmp/work").id);
    }

    #[test]
    fn a_question_that_nothing_closes_above_is_refused_even_when_the_history_is_the_screen() {
        // A Codex folder trust pane is on the alternate screen, so its history is the screen.
        let screen = fixture!("codex-trust");
        assert!(prompt(screen).unwrap().cut);
        assert_eq!(whole_prompt(screen, || Some(screen.to_string())), None);
    }

    #[test]
    fn an_indented_command_row_that_starts_like_history_stays_in_the_question() {
        let command = |first: &str| {
            format!(
                "• Ran ls\n\n  Would you like to run the following command?\n\n  $ {first}\n  > /tmp/result\n  echo done\n\n› 1. Yes, proceed (y)\n  2. No (esc)\n\n  Press enter to confirm or esc to cancel\n"
            )
        };
        let harmless = prompt(&command("printf safe")).unwrap();
        let deleting = prompt(&command("rm -rf /tmp/work")).unwrap();
        assert!(
            deleting
                .question
                .contains("$ rm -rf /tmp/work\n> /tmp/result"),
            "{}",
            deleting.question
        );
        assert_ne!(deleting.id, harmless.id);
        assert!(!deleting.cut);
    }

    #[test]
    fn a_wrapped_choice_row_is_part_of_its_label_and_id() {
        let rows = fixture!("claude-waiting");
        let other = rows.replace("      /tmp/work from", "      /etc from");
        assert_ne!(other, rows);
        assert_ne!(prompt(&other).unwrap().id, prompt(rows).unwrap().id);
        // The last choice wraps too.
        let last = rows.replace(
            "   3. No\n",
            "   3. No, and tell Claude what\n      to do instead\n",
        );
        assert_eq!(
            prompt(&last).unwrap().choices[2],
            "No, and tell Claude what\nto do instead"
        );
        // A row left of the labels is not a choice's, so the list is not a prompt.
        let stray = rows.replace("      /tmp/work from", " /tmp/work from");
        assert_eq!(prompt(&stray), None);
    }

    #[test]
    fn an_old_codex_list_above_the_idle_composer_is_no_prompt() {
        let rows = format!(
            "{}\n› Ask Codex to do anything\n\n  ? for shortcuts                                100% context left\n",
            fixture!("codex-waiting")
        );
        assert_eq!(prompt(&rows), None);
        assert_eq!(screen_state("codex", &rows), Some(ScreenState::Idle));
    }

    #[test]
    fn a_screen_with_no_footer_below_its_list_has_no_prompt() {
        for rows in [
            fixture!("claude-idle"),
            fixture!("claude-after-esc"),
            fixture!("claude-working"),
            fixture!("claude-done"),
            fixture!("codex-idle"),
            fixture!("codex-working"),
            fixture!("codex-failed"),
            fixture!("agy-idle"),
            fixture!("agy-working"),
            // A numbered answer with the working footer rows below it.
            "• Plan\n  1. Read\n  2. Edit\n────\n>\n────\nesc to cancel      Gemini 3.7 Flash\n",
            // A cursor history row with a wrapped second row, as Claude shows a sent message.
            "❯ Use the tool to ask me\n  one question\n────\n",
        ] {
            assert_eq!(prompt(rows), None, "{rows}");
        }
    }
}
