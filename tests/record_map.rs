//! The record-end guard from ADR 0139. Fixtures use public provider snapshots.
use serde_json::Value;
use swarm::view::transcript::source::{
    EndPolicy, NativeFormat, RowProjection, SnapshotError, SnapshotReader, SourceMeta, Work,
};
use swarm::view::transcript::{Row, RowKind};

#[path = "record_map/fixtures.rs"]
mod fixtures;
#[path = "record_map/mapping.rs"]
mod mapping;
#[path = "record_map/readers.rs"]
mod readers;

use fixtures::fixture;
use mapping::{End, RECORD_MAP};
use readers::{READERS, reader_kinds};

struct Ready;
impl Work for Ready {
    fn check(&mut self) -> Result<(), SnapshotError> {
        Ok(())
    }
}

fn snapshot(cli: &str, lines: &[Value]) -> swarm::view::transcript::source::Snapshot {
    let mut reader = SnapshotReader::with_open_path(
        SourceMeta {
            source_id: "map".into(),
        },
        0,
        EndPolicy::SnapshotEof,
        NativeFormat::from_name(cli).unwrap(),
        "",
    );
    let text = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    reader.push(text.as_bytes(), &mut Ready).unwrap();
    reader.finish(&mut Ready).unwrap()
}

fn all_rows(rows: &[Row]) -> Vec<&Row> {
    rows.iter()
        .flat_map(|row| std::iter::once(row).chain(all_rows(&row.steps)))
        .collect()
}

#[test]
fn every_named_record_has_an_end_and_each_row_or_part_replays() {
    let bad = "match kind.as_str() {\n    \"new_guard_kind\" => emit(),\n}";
    assert!(
        reader_kinds(bad).contains("new_guard_kind"),
        "the kind scanner must reject a new unmapped branch"
    );
    for (cli, file, source) in READERS {
        for name in reader_kinds(source) {
            assert!(
                RECORD_MAP
                    .iter()
                    .any(|(entry_cli, kind, _)| entry_cli == cli
                        && kind.rsplit('/').next() == Some(name.as_str())),
                "{cli} reader {file} names {name:?}, but RECORD_MAP has no end state for it"
            );
        }
    }
    for &(cli, kind, end) in RECORD_MAP {
        let input = fixture(cli, kind);
        let snapshot = snapshot(cli, &input);
        assert!(
            snapshot.lines.iter().all(|line| !line.events.is_empty()),
            "{cli}/{kind} dropped a physical record"
        );
        let RowProjection::Rows(rows) = snapshot.rows else {
            panic!("{cli}/{kind} cannot project rows")
        };
        let all = all_rows(&rows);
        let visible: Vec<_> = all
            .iter()
            .copied()
            .filter(|row| !row.collapsed && row.kind != RowKind::Fold)
            .collect();
        let events: Vec<Value> = snapshot
            .lines
            .last()
            .unwrap()
            .events
            .iter()
            .map(|event| serde_json::to_value(event).unwrap())
            .collect();
        match end {
            End::Row => assert!(!visible.is_empty(), "{cli}/{kind} has no visible row"),
            End::Part => {
                let state = match (cli, kind) {
                    ("claude", "ai-title" | "custom-title" | "attachment/model")
                    | ("codex", "session_meta" | "turn_context")
                    | ("pi", "model_change" | "session_info") => Some("session_info"),
                    ("codex", "event_msg/token_count") => Some("usage"),
                    ("codex", "event_msg/task_started") => Some("turn_started"),
                    ("codex", "event_msg/thread_rolled_back") => Some("thread_rolled_back"),
                    _ => None,
                };
                if let Some(state) = state {
                    assert!(
                        events.iter().any(|event| event["type"] == state),
                        "{cli}/{kind} lost its {state} part"
                    );
                    if state == "thread_rolled_back" {
                        assert!(visible.is_empty(), "rolled-back turn is still visible");
                    }
                } else {
                    assert_eq!(
                        visible.len(),
                        1,
                        "{cli}/{kind} must belong to one named row"
                    );
                    // These old Codex copies are deliberately ignored after their canonical
                    // row. Every other part must keep its physical source on that row.
                    let ignored_copy = cli == "codex"
                        && matches!(
                            kind,
                            "event_msg/user_message"
                                | "event_msg/agent_message"
                                | "event_msg/exec_command_end"
                                | "event_msg/mcp_tool_call_end"
                                | "event_msg/web_search_end"
                        );
                    let source_prefix =
                        format!("raw-{}-", snapshot.lines.last().unwrap().start_byte);
                    assert!(
                        ignored_copy
                            || visible[0]
                                .source_ids
                                .iter()
                                .any(|id| id.starts_with(&source_prefix)),
                        "{cli}/{kind} lost its part's source"
                    );
                }
            }
            End::Hidden | End::NotKnown => assert!(
                visible.is_empty(),
                "{cli}/{kind} unexpectedly became visible"
            ),
        }
        if matches!(end, End::Row | End::Part | End::Hidden) {
            assert!(
                !all.iter()
                    .any(|row| row.system_kind.as_deref() == Some("unmapped")),
                "{cli}/{kind} is counted as unmapped"
            );
        }
    }
}
