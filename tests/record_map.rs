//! The record-end guard from ADR 0139. Fixtures use public provider snapshots.
use serde_json::Value;
use swarm::view::transcript::source::{NativeFormat, RowProjection};
use swarm::view::transcript::{Row, RowKind};

mod common;

#[path = "record_map/fixtures.rs"]
mod fixtures;
#[path = "record_map/mapping.rs"]
mod mapping;
#[path = "record_map/readers.rs"]
mod readers;

use fixtures::fixture;
use mapping::{End, Part, RECORD_MAP};
use readers::{reader_kinds, readers};

fn snapshot(cli: &str, lines: &[Value]) -> swarm::view::transcript::source::Snapshot {
    common::snapshot(NativeFormat::from_name(cli).unwrap(), "map", "", lines)
}

fn all_rows(rows: &[Row]) -> Vec<&Row> {
    rows.iter()
        .flat_map(|row| std::iter::once(row).chain(all_rows(&row.steps)))
        .collect()
}

#[test]
fn every_named_record_has_an_end_and_each_row_or_part_replays() {
    for (cli, file, source) in readers() {
        for kind in reader_kinds(&cli, &file, &source) {
            assert!(
                RECORD_MAP
                    .iter()
                    .any(|(entry_cli, entry_kind, _)| *entry_cli == cli && *entry_kind == kind),
                "{cli} reader {file} names {kind:?}, but RECORD_MAP has no end state for it"
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
            End::Part(Part::State(state)) => assert!(
                events.iter().any(|event| event["type"] == state),
                "{cli}/{kind} lost its {state} part"
            ),
            End::Part(Part::Rollback) => {
                assert!(
                    events
                        .iter()
                        .any(|event| event["type"] == "thread_rolled_back")
                );
                assert!(visible.is_empty(), "rolled-back turn is still visible");
            }
            End::Part(part) => {
                assert_eq!(
                    visible.len(),
                    1,
                    "{cli}/{kind} must belong to one named row"
                );
                let source_prefix = format!("raw-{}-", snapshot.lines.last().unwrap().start_byte);

                assert!(
                    part == Part::IgnoredCopy
                        || visible[0]
                            .source_ids
                            .iter()
                            .any(|id| id.starts_with(&source_prefix)),
                    "{cli}/{kind} lost its part's source"
                );
            }
            End::Hidden | End::NotKnown => assert!(
                visible.is_empty(),
                "{cli}/{kind} unexpectedly became visible"
            ),
        }

        if matches!(end, End::Row | End::Part(_) | End::Hidden) {
            assert!(
                !all.iter()
                    .any(|row| row.system_kind.as_deref() == Some("unmapped")),
                "{cli}/{kind} is counted as unmapped"
            );
        }
    }
}
