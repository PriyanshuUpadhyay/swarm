// Child-agent limits and extra-argument tests.
use super::*;

mod tests {
    use super::*;

    #[test]
    fn a_looping_sweep_is_refused_only_under_the_app() {
        assert!(sweep_refusal("tmux-swarm", true).is_some());
        assert!(sweep_refusal("tmux-swarm", false).is_none());
        assert!(sweep_refusal("herdr", true).is_none());
        assert!(sweep_refusal("tmux", true).is_none());
    }

    #[test]
    fn only_the_orchestrator_launches_and_fable_stays_on_review_and_council() {
        assert!(launch_refusal(None, None).is_none());
        assert!(launch_refusal(Some("orchestrator"), None).is_none());
        assert!(launch_refusal(Some("coder"), None).is_some());
        assert!(launch_refusal(None, Some("1")).is_some());

        assert!(fable_refusal("coder", "code.complex", Some("claude-fable-5-1")).is_some());
        assert!(fable_refusal("seat", "council.claude", Some("claude-fable-5-1")).is_none());
        assert!(fable_refusal("seat", "review.pr", Some("Claude-FABLE")).is_none());
        assert!(fable_refusal("orchestrator", "code.complex", Some("claude-fable-5-1")).is_none());
        assert!(fable_refusal("coder", "code.complex", Some("claude-opus-5-5")).is_none());
    }

    #[test]
    fn a_denied_herdr_socket_refuses_launch() {
        let denied =
            r#"Error: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }"#;
        assert!(socket_refusal(denied).is_some());
        assert!(socket_refusal("").is_none());
    }

    #[test]
    fn extra_args_keep_role_flags_out_and_agy_interactive() {
        assert!(extra_args(Provider::Claude, &strings(&["--model=opus"])).is_err());
        assert!(extra_args(Provider::Codex, &strings(&["-s", "danger-full-access"])).is_err());
        assert_eq!(
            extra_args(Provider::Claude, &strings(&["--", "--model"])).unwrap(),
            strings(&["--", "--model"])
        );
        assert_eq!(
            extra_args(Provider::Agy, &strings(&["fix the bug"])).unwrap(),
            strings(&["-i", "fix the bug"])
        );
        assert_eq!(
            extra_args(Provider::Agy, &strings(&["-i", "fix"])).unwrap(),
            strings(&["-i", "fix"])
        );
    }

    #[test]
    fn a_claude_child_runs_from_the_pool_with_a_reserved_session_id() {
        let cwd = std::path::Path::new("/repo");
        let id = uuid::Uuid::now_v7();
        let (dir, args) = claude_child("coder", cwd, &strings(&["--verbose"]), &id);
        assert_eq!(dir, std::path::Path::new("/repo/.herdr/workers"));
        assert_eq!(args[0], "--session-id");
        assert!(args[1].starts_with("aaaaaaaa-") && uuid::Uuid::parse_str(&args[1]).is_ok());
        assert_eq!(
            args[2..],
            strings(&["-n", "coder", "--verbose", "--add-dir", "/repo"])
        );

        let (_, named) = claude_child("coder", cwd, &strings(&["--name", "x"]), &id);
        assert!(!named.contains(&"-n".to_string()));
        let (dir, resumed) = claude_child("coder", cwd, &strings(&["--resume", "abc"]), &id);
        assert_eq!(
            (dir.as_path(), resumed),
            (cwd, strings(&["--resume", "abc"]))
        );
    }
}
