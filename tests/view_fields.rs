mod sidebar_fixture;
use serde_json::json;
use sidebar_fixture::*;
use swarm::view::{
    choices::ViewState,
    discovery::{Discovery, dirty_count, parse_worktrees},
    fields::{self, RowFieldLists},
    tree,
};
#[test]
fn field_defaults_and_unknown_names() {
    assert_eq!(RowFieldLists::default().project, ["title", "status"]);
    assert_eq!(
        RowFieldLists::default().workspace,
        ["status", "title", "branch", "children", "age", "steps"]
    );
    assert_eq!(RowFieldLists::default().chat, ["status", "title", "unread"]);
    assert_eq!(RowFieldLists::default().tab, ["status", "title", "unread"]);
    let fields: RowFieldLists =
        serde_json::from_value(json!({"chat":["title","future","cost"],"tab":[]})).unwrap();
    assert_eq!(fields.chat, ["title", "cost"]);
    assert!(fields.tab.is_empty());
    assert_eq!(fields.project, RowFieldLists::default().project);
}
#[test]
fn field_old_bundled_lists_load_as_current_defaults() {
    let fields: RowFieldLists = serde_json::from_value(json!({
        "chat": ["status", "title", "age", "steps", "children", "unread", "tokens"],
        "tab": ["status", "title", "provider", "unread", "tokens"]
    }))
    .unwrap();
    assert_eq!(fields, RowFieldLists::default());
    assert_eq!(fields.chat, ["status", "title", "unread"]);
    assert_eq!(fields.tab, ["status", "title", "unread"]);
}
#[test]
fn field_defaults_keep_unread_for_recent_rows_and_tabs() {
    let t = tree();
    let o = owner();
    let c = t.chats().next().unwrap();
    let mut state = ViewState::default();
    state.last_seen.insert(c.key.clone(), c.last_activity - 1);
    let context = fields::chat_context(c, &state, 100, None, None);
    for list in [&o.fields.chat, &o.fields.tab] {
        assert!(context.values(list).iter().any(|f| f.field == "unread"));
    }
}
#[test]
fn field_custom_lists_keep_order_and_optional_fields() {
    let saved = json!({
        "project": ["status", "title"],
        "workspace": [],
        "chat": ["title", "status", "tokens"],
        "tab": ["title", "status", "provider", "unread", "tokens"]
    });
    let fields: RowFieldLists = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(&fields).unwrap(), saved);
    assert_eq!(fields.chat, ["title", "status", "tokens"]);
    assert_eq!(
        fields.tab,
        ["title", "status", "provider", "unread", "tokens"]
    );
}
#[test]
fn field_serialization_writes_only_lists_that_differ_from_defaults() {
    let mut fields = RowFieldLists::default();
    assert_eq!(serde_json::to_value(&fields).unwrap(), json!({}));
    for surface in ["project", "workspace", "chat", "tab"] {
        let list = match surface {
            "project" => &mut fields.project,
            "workspace" => &mut fields.workspace,
            "chat" => &mut fields.chat,
            _ => &mut fields.tab,
        };
        list.clear();
        let saved = serde_json::to_value(&fields).unwrap();
        assert_eq!(saved, json!({surface: []}));
        assert_eq!(
            serde_json::from_value::<RowFieldLists>(saved).unwrap(),
            fields
        );
        fields = RowFieldLists::default();
    }
    let owner = swarm::view::choices::OwnerChoices::default();
    assert_eq!(serde_json::to_value(owner).unwrap()["fields"], json!({}));
}
#[test]
fn agent_keys_order_and_currency_cost() {
    let mut a = agent("worker", "waiting");
    a["model"] = json!("model");
    a["tokens"] = json!(12);
    a["costUsd"] = json!(1.235);
    a["prompt"] = json!({"question":"Choose"});
    let ctx = fields::agent_context(&a, "Worker");
    let order: Vec<String> = ["question", "future", "tokens", "cost", "title"]
        .map(str::to_owned)
        .to_vec();
    let values = ctx.values(&order);
    assert_eq!(
        values.iter().map(|v| v.field.as_str()).collect::<Vec<_>>(),
        ["question", "tokens", "cost", "title"]
    );
    assert_eq!(values[2].text, "$1.24");
}
#[test]
fn current_session_questions_and_chain_usage() {
    let mut a = agent("orchestrator", "waiting");
    a["tokens"] = json!(5);
    a["costUsd"] = json!(0.2);
    a["prompt"] = json!({"question":"Old"});
    let mut b = agent("orchestrator", "working");
    b["tokens"] = json!(7);
    b["costUsd"] = json!(0.3);
    b["prompt"] = json!({"question":"Current"});
    let mut old = session("old", "/repo/wt/main", 1);
    old["chair_id"] = json!("same");
    let mut new = session("new", "/repo/wt/main", 2);
    new["chair_id"] = json!("same");
    let t = tree::build(
        &snapshot(vec![old, new], json!({"old":[a],"new":[b]})),
        &owner(),
    );
    let c = &t.projects[0].workspaces[0].chats[0];
    let ctx = fields::chat_context(c, &ViewState::default(), 100, None, None);
    assert_eq!(ctx.text["question"], "Current");
    assert_eq!(ctx.text["tokens"], "12 tokens");
    assert_eq!(ctx.text["cost"], "$0.50");
}
#[test]
fn unread_baseline_refresh_and_pruning() {
    let mut state = ViewState::default();
    let t = tree();
    let chat = &t.projects[0].workspaces[0].chats[0];
    state.record_first_sight(&t);
    assert!(!state.is_unread(&chat.key, chat.last_activity));
    assert!(state.is_unread(&chat.key, chat.last_activity + 1));
    state.mark_seen(&chat.key, chat.last_activity, 100);
    assert!(!state.is_unread(&chat.key, 99));
    state.last_seen.insert("gone".into(), 12);
    state.record_first_sight(&t);
    assert!(!state.last_seen.contains_key("gone"));
}
#[test]
fn requested_paths_and_independent_cache_intervals() {
    let t = tree();
    let mut o = owner();
    let mut cache = Discovery::default();
    let mut dirty = 0;
    let mut github = 0;
    assert!(!cache.refresh_fields_with(
        &t,
        &o,
        0,
        |_| {
            dirty += 1;
            Some(2)
        },
        |_| {
            github += 1;
            None
        }
    ));
    assert_eq!((dirty, github), (0, 0));
    o.fields.chat.push("dirty".into());
    o.fields.tab.push("ci".into());
    assert!(cache.refresh_fields_with(
        &t,
        &o,
        1,
        |_| {
            dirty += 1;
            Some(2)
        },
        |_| {
            github += 1;
            Some(json!({"number":1,"statusCheckRollup":[{"state":"SUCCESS"}]}))
        }
    ));
    assert_eq!((dirty, github), (2, 2));
    cache.refresh_fields_with(
        &t,
        &o,
        10,
        |_| {
            dirty += 1;
            Some(2)
        },
        |_| {
            github += 1;
            None
        },
    );
    assert_eq!((dirty, github), (2, 2));
    cache.refresh_fields_with(
        &t,
        &o,
        11,
        |_| {
            dirty += 1;
            Some(2)
        },
        |_| {
            github += 1;
            None
        },
    );
    assert_eq!((dirty, github), (4, 2));
    cache.refresh_fields_with(
        &t,
        &o,
        121,
        |_| {
            dirty += 1;
            None
        },
        |_| {
            github += 1;
            None
        },
    );
    assert_eq!((dirty, github), (6, 4));
    assert!(
        cache
            .fields
            .values()
            .all(|v| v.pr.is_none() && v.ci.is_none() && v.dirty_count.is_none())
    );
    o.fields.workspace.push("dirty".into());
    assert_eq!(fields::requested_paths("dirty", &t, &o.fields).len(), 3);
}
#[test]
fn check_states() {
    for (checks, expected) in [
        (json!([]), None),
        (json!([{"state":"SUCCESS"}]), Some("CI passed")),
        (
            json!([{"conclusion":"SKIPPED"},{"state":"SUCCESS"}]),
            Some("CI passed"),
        ),
        (json!([{"status":"IN_PROGRESS"}]), Some("CI pending")),
        (
            json!([{"state":"FAILURE"},{"state":"PENDING"}]),
            Some("CI failed"),
        ),
        (json!([{"state":"FUTURE"}]), Some("CI unknown")),
    ] {
        assert_eq!(
            fields::github_fields(&json!({"number":42,"statusCheckRollup":checks}))
                .ci
                .as_deref(),
            expected
        );
    }
    assert_eq!(fields::github_fields(&json!({})), Default::default());
}
#[test]
fn dirty_renames_and_worktree_flags() {
    assert_eq!(dirty_count(b" M file\0R  new\0old\0?? extra\0"), 3);
    let w=parse_worktrees(b"worktree /hub/.bare\0bare\0\0worktree /hub/main\0branch refs/heads/main\0locked reason\0prunable reason\0\0worktree /hub/detached\0detached\0");
    assert!(w[0].bare);
    assert_eq!(w[1].branch.as_deref(), Some("main"));
    assert!(w[1].locked && w[1].missing);
    assert!(w[2].detached);
}

#[test]
fn slow_fields_follow_each_surface_and_continued_chats_keep_unread() {
    let mut a = session("old", "/repo/wt/main", 1);
    a["chair_id"] = json!("same");
    let mut b = session("new", "/repo/wt/main", 2);
    b["chair_id"] = json!("same");
    b["last_message_at"] = json!(20);
    let mut o = owner();
    let t = tree::build(
        &snapshot(
            vec![a, b],
            json!({"old":[agent("orchestrator","ended")],"new":[agent("orchestrator","working")]}),
        ),
        &o,
    );
    let c = t.chats().next().unwrap();
    let mut state = ViewState::default();
    state.last_seen.insert(c.key.clone(), 10);
    assert!(state.is_unread(&c.key, c.last_activity));
    state.record_first_sight(&t);
    assert_eq!(state.last_seen[&c.key], 10);
    o.fields.project = vec!["ci".into(), "pr".into()];
    o.fields.workspace = vec!["pr".into(), "ci".into()];
    o.fields.chat = vec!["ci".into(), "pr".into()];
    o.fields.tab = vec!["pr".into(), "ci".into()];
    let cache = std::collections::BTreeMap::from([(
        "/repo/wt/main".into(),
        fields::WorkspaceFields {
            dirty_count: None,
            pr: Some("PR #7".into()),
            ci: Some("CI pending".into()),
        },
    )]);
    let sections = tree::sections(&t, &o, &state, false, 100, &cache);
    assert_eq!(
        sections[0]
            .fields
            .iter()
            .map(|v| v.text.as_str())
            .collect::<Vec<_>>(),
        ["CI pending", "PR #7"]
    );
    assert_eq!(
        sections[0].rows[0]
            .fields
            .iter()
            .map(|v| v.text.as_str())
            .collect::<Vec<_>>(),
        ["PR #7", "CI pending"]
    );
    assert_eq!(
        sections[0].rows[1]
            .fields
            .iter()
            .map(|v| v.text.as_str())
            .collect::<Vec<_>>(),
        ["CI pending", "PR #7"]
    );
    let tabs = swarm::view::tabs::view(&t, &o, &state, "/repo/wt/main", 100, &cache).unwrap();
    assert_eq!(
        tabs.tabs[0]
            .fields
            .iter()
            .map(|v| v.text.as_str())
            .collect::<Vec<_>>(),
        ["PR #7", "CI pending"]
    );
}
