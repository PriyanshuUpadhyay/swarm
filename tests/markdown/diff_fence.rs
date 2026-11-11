use serde_json::{Value, json};
use swarm::view::transcript::markdown::display::display;

fn patch(source: &str) -> Value {
    serde_json::to_value(display(&format!("```diff\n{source}\n```")).unwrap()).unwrap()["blocks"][0]
        ["value"]["diff"]
        .clone()
}

#[test]
fn wrong_hunk_counts_do_not_take_in_the_next_file() {
    // Main cd934f25: real chair output, including a missing and a bare hunk header.
    let source = [
        "--- a.ts",
        "+++ a.ts",
        "@@ -102,7 +102,7 @@",
        " export function make(",
        "-    old: A,",
        "+    old: B,",
        "--- b.ts",
        "+++ b.ts",
        "-import type { A } from \"x\";",
        "@@",
        "-    const a = {} as A;",
        "+    const a = { b: 1 };",
    ]
    .join("\n");
    let result = patch(&source);
    let lines = result["lines"].as_array().unwrap();
    let files: Vec<_> = lines
        .iter()
        .filter(|line| line["kind"] == "meta")
        .filter_map(|line| line["text"].as_str()?.strip_prefix("+++ "))
        .collect();
    let hunks: Vec<_> = lines
        .iter()
        .filter(|line| line["kind"] == "hunk")
        .map(|line| line["text"].clone())
        .collect();
    assert_eq!(files, vec!["a.ts", "b.ts"]);
    assert!(
        !lines
            .iter()
            .any(|line| (line["kind"] == "add" && line["text"] == "++ b.ts")
                || (line["kind"] == "del" && line["text"] == "-- b.ts"))
    );
    assert_eq!(
        hunks,
        json!(["@@ -102,2 +102,2 @@", "@@ -1,1 +1,0 @@", "@@ -2,1 +1,1 @@"])
            .as_array()
            .unwrap()
            .clone()
    );
    assert!(lines.iter().any(|line| line["kind"] == "del"
        && line["text"] == "import type { A } from \"x\";"
        && line["old"] == 1));
}

#[test]
fn bare_fence_counts_the_final_deletion_without_an_insertion() {
    let result =
        patch("@@ -1,1 +1,1 @@\n keep\n-<script>x</script>\n+new\n-gone\n@@ -9,1 +9,1 @@\n-late");
    let lines = result["lines"].as_array().unwrap();
    assert_eq!(lines.iter().filter(|line| line["kind"] == "del").count(), 3);
    assert_eq!(lines.iter().filter(|line| line["kind"] == "add").count(), 1);
    assert!(
        lines
            .iter()
            .any(|line| line["kind"] == "hunk" && line["text"] == "@@ -9,1 +9,0 @@")
    );
}
