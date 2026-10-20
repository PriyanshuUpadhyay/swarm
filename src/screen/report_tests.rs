//! Test hook and screen report resolution.

mod tests {
    use super::super::*;

    #[test]
    fn a_send_reads_as_working_until_a_report_a_busy_screen_or_the_hold_ends() {
        let idle = Some(ScreenState::Idle);
        assert!(held_by_send(Some(100), Some(90), idle, 104));
        assert!(held_by_send(Some(100), None, None, 100));
        assert!(!held_by_send(Some(100), Some(90), idle, 105));
        assert!(!held_by_send(Some(100), Some(101), idle, 102));
        assert!(!held_by_send(
            Some(100),
            Some(90),
            Some(ScreenState::Waiting),
            101
        ));
        assert!(!held_by_send(None, Some(90), idle, 101));
    }

    #[test]
    fn a_fresh_hook_wins_then_a_differing_screen_is_written() {
        let now = 1_000;
        let waiting = Some(ScreenState::Waiting);
        let idle = Some(ScreenState::Idle);
        // A hook report 9 s old is the authority, whatever the screen shows.
        assert_eq!(
            resolve(
                Some("claude"),
                Some("working"),
                Some(now - 9),
                Some("hook"),
                waiting,
                now
            ),
            (Some("working".into()), None)
        );
        // At 10 s the screen wins: Esc after a permission prompt sends no hook.
        assert_eq!(
            resolve(
                Some("claude"),
                Some("waiting"),
                Some(now - 10),
                Some("hook"),
                idle,
                now
            ),
            (Some("done".into()), Some("done"))
        );
        // A screen that agrees writes nothing.
        assert_eq!(
            resolve(
                Some("claude"),
                Some("waiting"),
                Some(now - 30),
                Some("hook"),
                waiting,
                now
            ),
            (Some("waiting".into()), None)
        );
        assert_eq!(
            resolve(
                Some("claude"),
                Some("failed"),
                Some(now - 30),
                Some("hook"),
                idle,
                now
            ),
            (Some("failed".into()), None)
        );
        // An agent with no report yet that sits at its prompt is done.
        assert_eq!(
            resolve(None, None, None, None, idle, now),
            (Some("done".into()), Some("done"))
        );
        // A screen source has no authority window.
        assert_eq!(
            resolve(
                Some("claude"),
                Some("waiting"),
                Some(now - 1),
                Some("screen"),
                idle,
                now
            ),
            (Some("done".into()), Some("done"))
        );
    }

    #[test]
    fn only_provider_hook_gaps_overturn_a_recent_report() {
        let now = 1_000;
        for provider in [None, Some("claude"), Some("codex"), Some("agy")] {
            for screen in [ScreenState::Idle, ScreenState::Failed, ScreenState::Waiting] {
                let gap = screen == ScreenState::Idle
                    || (provider == Some("codex") && screen == ScreenState::Failed)
                    || (provider == Some("agy") && screen == ScreenState::Waiting);
                assert_eq!(screen_gap(provider, screen), gap);
                assert_eq!(
                    resolve(
                        provider,
                        Some("working"),
                        Some(now - 11),
                        Some("hook"),
                        Some(screen),
                        now
                    ),
                    if gap {
                        (Some(screen.as_state().into()), Some(screen.as_state()))
                    } else {
                        (Some("working".into()), None)
                    },
                    "{provider:?} {screen:?}"
                );
                // Even a gap cannot replace a hook inside its initial authority window.
                assert_eq!(
                    resolve(
                        provider,
                        Some("working"),
                        Some(now - 9),
                        Some("hook"),
                        Some(screen),
                        now
                    ),
                    (Some("working".into()), None)
                );
            }
            assert!(!screen_gap(provider, ScreenState::Working));
        }
    }

    #[test]
    fn a_screen_failure_clears_on_an_idle_screen_but_a_hook_failure_stays() {
        let now = 1_000;
        let idle = Some(ScreenState::Idle);
        // The screen saw the error row; the retry finished with no hook, the row scrolled away.
        assert_eq!(
            resolve(
                Some("claude"),
                Some("failed"),
                Some(now - 30),
                Some("screen"),
                idle,
                now
            ),
            (Some("done".into()), Some("done"))
        );
        // A failure the provider reported by hook ends at the same idle prompt; it stays.
        assert_eq!(
            resolve(
                Some("claude"),
                Some("failed"),
                Some(now - 30),
                Some("hook"),
                idle,
                now
            ),
            (Some("failed".into()), None)
        );
        // The error row still showing keeps the screen's failure.
        assert_eq!(
            resolve(
                Some("claude"),
                Some("failed"),
                Some(now - 30),
                Some("screen"),
                Some(ScreenState::Failed),
                now
            ),
            (Some("failed".into()), None)
        );
    }

    #[test]
    fn a_working_screen_keeps_a_long_turn_and_corrects_a_stale_report() {
        let now = 1_000;
        let working = Some(ScreenState::Working);
        assert_eq!(
            resolve(
                Some("claude"),
                Some("working"),
                Some(now - 60),
                Some("hook"),
                working,
                now
            ),
            (Some("working".into()), None)
        );
        for state in ["waiting", "done"] {
            // A working screen cannot replace a hook report through STALE_S.
            for age in [11, 44, 45] {
                assert_eq!(
                    resolve(
                        Some("claude"),
                        Some(state),
                        Some(now - age),
                        Some("hook"),
                        working,
                        now
                    ),
                    (Some(state.into()), None),
                    "{state} at {age} s"
                );
            }
            assert_eq!(
                resolve(
                    Some("claude"),
                    Some(state),
                    Some(now - 46),
                    Some("hook"),
                    working,
                    now
                ),
                (Some("working".into()), Some("working")),
                "{state}"
            );
        }
        assert_eq!(
            resolve(
                Some("claude"),
                Some("done"),
                Some(now - 5),
                Some("hook"),
                working,
                now
            ),
            (Some("done".into()), None)
        );
    }

    #[test]
    fn an_old_unconfirmed_turn_is_shown_as_no_report() {
        let now = 1_000;
        assert_eq!(
            resolve(
                Some("claude"),
                Some("working"),
                Some(now - 45),
                Some("hook"),
                None,
                now
            ),
            (Some("working".into()), None)
        );
        assert_eq!(
            resolve(
                Some("claude"),
                Some("working"),
                Some(now - 46),
                Some("hook"),
                None,
                now
            ),
            (None, None)
        );
        assert_eq!(
            resolve(
                Some("claude"),
                Some("waiting"),
                Some(now - 46),
                Some("screen"),
                None,
                now
            ),
            (None, None)
        );
        assert_eq!(
            resolve(
                Some("claude"),
                Some("waiting"),
                Some(now - 300),
                Some("hook"),
                Some(ScreenState::Waiting),
                now
            ),
            (Some("waiting".into()), None)
        );
        assert_eq!(
            resolve(
                Some("claude"),
                Some("done"),
                Some(now - 300),
                Some("hook"),
                None,
                now
            ),
            (Some("done".into()), None)
        );
        assert_eq!(
            resolve(
                Some("claude"),
                Some("working"),
                Some(now - 300),
                Some("hook"),
                Some(ScreenState::Working),
                now
            ),
            (Some("working".into()), None)
        );
    }
}
