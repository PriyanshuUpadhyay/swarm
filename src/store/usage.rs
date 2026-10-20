//! Agent usage updates and cursor tests.

use super::*;

/// Add Claude message usage or copy Codex cumulative usage. NULL fields keep prior values.
pub fn update_usage(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    provider: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let stored = connection
        .query_row(
            "SELECT log, usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
            (session_id, agent_id),
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()?;
    let Some((Some(log), old_state)) = stored else {
        return Ok(());
    };
    let (snapshot, state) = if provider == "claude" {
        let mut state: crate::host::UsageState = old_state
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        // Message ids count once within each log; a repeat across logs counts again.
        let saved = state.logs.get(&log).cloned().unwrap_or_default();
        let Some(read) = crate::host::claude_usage(Path::new(&log), &saved) else {
            return Ok(());
        };
        let mut snapshot = read.snapshot;
        state.logs.insert(log.clone(), read.log);
        if snapshot.tokens.is_some()
            || saved.tokens > 0
            || !saved.ids.is_empty()
            || state
                .logs
                .values()
                .any(|log| log.tokens > 0 || !log.ids.is_empty())
        {
            let Some(total) = state
                .logs
                .values()
                .try_fold(0_i64, |total, log| total.checked_add(log.tokens))
            else {
                return Ok(());
            };
            snapshot.tokens = Some(total);
        }
        (snapshot, Some(serde_json::to_string(&state)?))
    } else {
        (
            crate::host::codex_usage_snapshot(Path::new(&log)),
            old_state.clone(),
        )
    };
    write_usage(
        connection,
        session_id,
        agent_id,
        &log,
        &snapshot,
        state.as_deref(),
        old_state.as_deref(),
    )
}

fn write_usage(
    connection: &Connection,
    session_id: &str,
    agent_id: &str,
    log: &str,
    snapshot: &crate::host::UsageSnapshot,
    state: Option<&str>,
    old_state: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // If another Stop changed the state, its write wins; the next Stop reads from that state.
    connection.execute(
        "UPDATE agent SET model = coalesce(?3, model), cost_usd = coalesce(?4, cost_usd),
                          tokens = coalesce(?5, tokens), usage_state = ?6
         WHERE session_id = ?1 AND id = ?2 AND usage_state IS ?7 AND log IS ?8",
        (
            session_id,
            agent_id,
            &snapshot.model,
            snapshot.cost_usd,
            snapshot.tokens,
            state,
            old_state,
            log,
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn claude_usage_keeps_each_log_cursor_and_resets_only_a_shorter_log() {
        let (root, connection) = usage_fixture("usage-reset");
        let path = root.join("chat.jsonl");
        let first = assistant_usage("m1", "claude");
        let second = assistant_usage("m2", "claude");
        std::fs::write(&path, format!("{first}\n{second}\n")).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let next = root.join("next.jsonl");
        std::fs::write(&next, format!("{first}\n")).unwrap();
        set_log(&connection, SESSION, CODER, &next).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(192));
        set_log(&connection, SESSION, CODER, &path).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(192));
        std::fs::write(&path, format!("{first}\n")).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(128));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_usage_adds_only_the_increase_in_a_later_message_record() {
        use std::io::Write;
        let (root, connection) = usage_fixture("usage-increase");
        let path = root.join("chat.jsonl");
        let first = assistant_usage("m1", "claude");
        std::fs::write(&path, format!("{first}\n")).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let higher = first.replace("\"output_tokens\":4", "\"output_tokens\":300");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{higher}\n{higher}\n{first}").unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(360));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_usage_defers_model_and_cost_until_a_stop_reaches_the_log_end() {
        let (root, connection) = usage_fixture("usage-budget");
        let runner = crate::config::Runner {
            provider: crate::providers::Provider::Claude,
            model: "claude-launch".into(),
            effort: "high".into(),
            sandbox: None,
            approval: None,
            permission: None,
        };
        set_launch(&connection, SESSION, CODER, "coder", &runner, None).unwrap();
        let path = root.join("chat.jsonl");
        let first = assistant_usage("m1", "claude-old");
        let second = assistant_usage("m2", "claude-new");
        let old_cost = r#"{"type":"cost-state","totalCostUSD":1,"hasUnknownModelCost":false}"#;
        let new_cost = r#"{"type":"cost-state","totalCostUSD":2,"hasUnknownModelCost":false}"#;
        let filler = format!("{}\n", "x".repeat(1023));
        let log = format!(
            "{first}\n{old_cost}\n{}{second}\n{new_cost}\n",
            filler.repeat(8192)
        );
        std::fs::write(&path, &log).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let agent = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(agent.tokens, Some(64));
        assert_eq!(agent.model.as_deref(), Some("claude-launch"));
        assert_eq!(agent.cost_usd, None);
        let stored: String = connection
            .query_row(
                "SELECT usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
                (SESSION, CODER),
                |row| row.get(0),
            )
            .unwrap();
        let state: crate::host::UsageState = serde_json::from_str(&stored).unwrap();
        let saved = &state.logs[&path.to_string_lossy().into_owned()];
        assert!(saved.offset > 0 && saved.offset <= 8 * 1024 * 1024);
        assert!(saved.offset < log.len() as i64);
        assert_eq!(saved.tokens, 64);
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let agent = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(agent.tokens, Some(128));
        assert_eq!(agent.model.as_deref(), Some("claude-new"));
        assert_eq!(agent.cost_usd, Some(2.0));
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(128));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_usage_defers_an_ordinary_record_cut_by_the_budget() {
        let (root, connection) = usage_fixture("usage-cut-record");
        let path = root.join("chat.jsonl");
        let first = assistant_usage("m1", "claude");
        let second = assistant_usage("m2", "claude");
        let limit = 8 * 1024 * 1024;
        let mut prefix = format!(
            "{first}\n{}",
            format!("{}\n", "x".repeat(1023)).repeat(8191)
        );
        prefix.push_str(&format!("{}\n", "x".repeat(limit - prefix.len() - 81)));
        std::fs::write(&path, format!("{prefix}{second}\n")).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(64));
        let stored: String = connection
            .query_row(
                "SELECT usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
                (SESSION, CODER),
                |row| row.get(0),
            )
            .unwrap();
        let state: crate::host::UsageState = serde_json::from_str(&stored).unwrap();
        assert_eq!(
            state.logs[&path.to_string_lossy().into_owned()].offset,
            prefix.len() as i64
        );
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(128));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_usage_skips_an_oversized_record_in_bounded_chunks() {
        let (root, connection) = usage_fixture("usage-oversized");
        let path = root.join("chat.jsonl");
        let huge = assistant_usage("huge", "claude").replace(
            "\"usage\"",
            &format!("\"content\":\"{}\",\"usage\"", "x".repeat(9 * 1024 * 1024)),
        );
        std::fs::write(
            &path,
            format!("{huge}\n{}\n", assistant_usage("m1", "claude")),
        )
        .unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let stored: String = connection
            .query_row(
                "SELECT usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
                (SESSION, CODER),
                |row| row.get(0),
            )
            .unwrap();
        let state: crate::host::UsageState = serde_json::from_str(&stored).unwrap();
        let offset = state.logs[&path.to_string_lossy().into_owned()].offset;
        assert!(offset > 0 && offset <= 8 * 1024 * 1024);
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(64));
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(64));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_concurrent_usage_state_change_wins_over_a_stale_stop_write() {
        let (root, connection) = usage_fixture("usage-write-race");
        let path = root.join("chat.jsonl");
        std::fs::write(&path, format!("{}\n", assistant_usage("m1", "claude"))).unwrap();
        let other = open(&root.join("swarm.db")).unwrap();
        for seed in [None, Some(r#"{"logs":{}}"#)] {
            connection
                .execute("UPDATE agent SET usage_state = ?1", [seed])
                .unwrap();
            let old_state: Option<String> = connection
                .query_row(
                    "SELECT usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
                    (SESSION, CODER),
                    |row| row.get(0),
                )
                .unwrap();
            let read =
                crate::host::claude_usage(&path, &crate::host::ClaudeLogUsage::default()).unwrap();
            let winning_state = r#"{"logs":{"other":{"offset":100,"tokens":123,"ids":[]}}}"#;
            other.execute("UPDATE agent SET usage_state = ?1, tokens = 123, model = 'winner', cost_usd = 9", [winning_state]).unwrap();
            write_usage(
                &connection,
                SESSION,
                CODER,
                &path.to_string_lossy(),
                &read.snapshot,
                Some(r#"{"logs":{}}"#),
                old_state.as_deref(),
            )
            .unwrap();
            let agent = &agents(&connection, SESSION).unwrap()[0];
            assert_eq!(agent.tokens, Some(123));
            assert_eq!(agent.model.as_deref(), Some("winner"));
            assert_eq!(agent.cost_usd, Some(9.0));
            let actual: String = connection
                .query_row(
                    "SELECT usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
                    (SESSION, CODER),
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(actual, winning_state);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_usage_ignores_partial_cost_and_synthetic_models() {
        let (root, connection) = usage_fixture("usage-partial");
        let path = root.join("chat.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n{}\n",
                assistant_usage("m1", "claude"),
                r#"{"type":"cost-state","totalCostUSD":3,"hasUnknownModelCost":true}"#,
                assistant_usage("stub", "<synthetic>")
            ),
        )
        .unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let agent = &agents(&connection, SESSION).unwrap()[0];
        assert_eq!(agent.cost_usd, None);
        assert_eq!(agent.tokens, Some(64));
        assert_eq!(agent.model.as_deref(), Some("claude"));
        std::fs::write(
            &path,
            r#"{"type":"cost-state","totalCostUSD":0,"hasUnknownModelCost":false}"#,
        )
        .unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].cost_usd, Some(0.0));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn claude_usage_keeps_an_unfinished_record_for_the_next_read() {
        use std::io::Write;
        let (root, connection) = usage_fixture("usage-unfinished");
        let path = root.join("chat.jsonl");
        let first = assistant_usage("m1", "claude");
        let second = assistant_usage("m2", "claude");
        let split = second.len() / 2;
        std::fs::write(
            &path,
            format!(
                "{first}\n{}\n{}",
                "x".repeat(1024 * 1024 + 100),
                &second[..split]
            ),
        )
        .unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(64));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{}", &second[split..]).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(agents(&connection, SESSION).unwrap()[0].tokens, Some(128));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_usage_retains_only_the_latest_message_ids() {
        use std::io::Write;
        let (root, connection) = usage_fixture("usage-id-limit");
        let path = root.join("chat.jsonl");
        let log = (0..66)
            .map(|id| assistant_usage(&format!("m{id}"), "claude"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, format!("{log}\n")).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        let stored: String = connection
            .query_row(
                "SELECT usage_state FROM agent WHERE session_id = ?1 AND id = ?2",
                (SESSION, CODER),
                |row| row.get(0),
            )
            .unwrap();
        let state: crate::host::UsageState = serde_json::from_str(&stored).unwrap();
        let ids = &state.logs[&path.to_string_lossy().into_owned()].ids;
        assert_eq!(ids.len(), crate::host::COUNTED_MESSAGES);
        assert_eq!(ids.front().map(|(id, _)| id.as_str()), Some("m2"));
        assert_eq!(ids.back().map(|(id, _)| id.as_str()), Some("m65"));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{}", assistant_usage("m65", "claude")).unwrap();
        update_usage(&connection, SESSION, CODER, "claude").unwrap();
        assert_eq!(
            agents(&connection, SESSION).unwrap()[0].tokens,
            Some(66 * 64)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
