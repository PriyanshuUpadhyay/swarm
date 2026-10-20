//! Tests for server tool output.

use super::*;
use crate::serve::test_support::*;
#[tokio::test]
async fn full_diff_uses_source_ordinal_and_retains_all_lines() {
    let fixture = Fixture::new();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("packages/transcript/src/fixtures/claude-edit.jsonl"),
        &fixture.log,
    )
    .unwrap();
    let page = row_page(&fixture, 200, None).await.unwrap();
    let locator = page["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|row| row["diff"]["id"].as_str())
        .unwrap();
    let Json(output) = tool_output(
        State(fixture.app.clone()),
        Query(ToolOutputQuery {
            session: fixture.session.clone(),
            agent: "chair".into(),
            row: locator.into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(locator, "raw-0-1:diff");
    let diff = page["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["diff"]["id"] == locator)
        .unwrap();
    assert_eq!(output["text"], diff["diff"]["patch"]);
    assert!(
        output["text"]
            .as_str()
            .unwrap()
            .contains("@@ -445,6 +445,7 @@")
    );
}
