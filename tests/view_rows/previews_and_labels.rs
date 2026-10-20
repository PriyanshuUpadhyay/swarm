// Tests for previews and labels.

use super::*;

#[test]
fn preserves_small_output() {
    let text = "first\n\n  indented λ\n";
    let preview = TextPreview::new(text, 120);
    assert_eq!(preview.text, text);
    assert!(!preview.is_truncated);
    assert!(!TextPreview::new("", 120).is_truncated);
}
#[test]
fn bounds_long_and_multiline_output() {
    let long_line = "👩🏽‍💻".repeat(12001);
    let preview = TextPreview::new(&long_line, 120);
    assert_eq!(
        preview.text,
        long_line.chars().take(12000).collect::<String>()
    );
    assert!(preview.is_truncated);
    let many_lines = (0..121)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let preview = TextPreview::new(&many_lines, 120);
    assert!(preview.text.ends_with("line 119"));
    assert!(preview.is_truncated);
}
#[test]
fn shell_output_folds_at_fifty_lines() {
    let fifty = (0..50)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!TextPreview::new(&fifty, 50).is_truncated);
    let preview = TextPreview::new(&(fifty.clone() + "\nline 50"), 50);
    assert_eq!(preview.text, fifty);
    assert!(preview.is_truncated);
}
#[test]
fn tool_output_folds_at_five_lines() {
    let preview = TextPreview::new("one\ntwo\nthree\nfour\nfive\nsix", 5);
    assert_eq!(preview.text, "one\ntwo\nthree\nfour\nfive");
    assert_eq!(preview.hidden_lines, 1);
    assert!(!TextPreview::new("one\ntwo\nthree\nfour\nfive", 5).is_truncated);
    assert_eq!(TextPreview::new(&"x".repeat(12001), 5).hidden_lines, 0);
}
#[test]
fn trailing_newline_ends_the_last_line() {
    let text = "a\nb\nc\nd\ne\n";
    let preview = TextPreview::new(text, 5);
    assert_eq!(preview.text, text);
    assert!(!preview.is_truncated);
    assert_eq!(preview.full_lines, 5);
    assert_eq!(preview.hidden_lines, 0);
    assert_eq!(TextPreview::new("\n", 120).full_lines, 1);
}
#[test]
fn character_cap_counts_every_line_it_hides() {
    let source = "x".repeat(20000)
        + (0..9)
            .map(|index| format!("\nline {index}"))
            .collect::<String>()
            .as_str();
    let preview = TextPreview::new(&source, 5);
    assert_eq!(preview.text, "x".repeat(12000));
    assert_eq!(preview.hidden_lines, 9);
    let source = vec!["w".repeat(290); 60].join("\n");
    assert_eq!(TextPreview::new(&source, 50).hidden_lines, 18);
}
#[test]
fn windows_line_ends_count_and_fold_as_lines() {
    let preview = TextPreview::new("one\r\ntwo\r\nthree", 2);
    assert_eq!(preview.text, "one\r\ntwo\r");
    assert!(preview.is_truncated);
    assert_eq!(preview.full_lines, 3);
    assert_eq!(preview.hidden_lines, 1);
    assert_eq!(TextPreview::new("", 120).full_lines, 1);
}
#[test]
fn chunks_keep_ordinary_lines_together() {
    assert_eq!(
        view::text_chunks("first\nsecond\nthird\n", 10),
        ["first\n", "second\n", "third\n"]
    );
}
#[test]
fn chunks_keep_every_character_in_order() {
    let source = "αβγ\n\n".to_owned() + &"👩🏽‍💻".repeat(17) + "\nlast\n";
    let pieces = view::text_chunks(&source, 5);
    assert_eq!(pieces.join(""), source);
    assert!(pieces.iter().all(|piece| piece.chars().count() <= 5));
    assert!(view::text_chunks("", 5).is_empty());
}

#[test]
fn known_input_keys() {
    let command = "printf 'one\ntwo'";
    let path = "/work/Folder With Spaces/file.txt";
    for key in ["command", "cmd", "CommandLine"] {
        let mut input = json!({});
        input[key] = json!(command);
        assert_eq!(view::command(&input, "Bash").as_deref(), Some(command));
    }
    for key in ["file_path", "TargetFile", "AbsolutePath"] {
        let mut input = json!({});
        input[key] = json!(path);
        assert_eq!(view::path(&input).as_deref(), Some(path));
    }
}
#[test]
fn duration() {
    assert_eq!(
        view::duration("2026-09-29T10:00:00Z", "2026-09-29T10:00:12Z"),
        Some(12.0)
    );
    assert_eq!(
        view::duration("2026-09-29T10:00:00.250Z", "2026-09-29T10:00:00.750Z"),
        Some(0.5)
    );
    assert_eq!(view::duration("", "2026-09-29T10:00:12Z"), None);
    assert_eq!(
        view::duration("2026-09-29T10:00:12Z", "2026-09-29T10:00:00Z"),
        None
    );
    assert_eq!(view::duration_label(0.42), "0.4s");
    assert_eq!(view::duration_label(12.4), "12s");
    assert_eq!(view::duration_label(185.0), "3m 5s");
}
#[test]
fn raw_input() {
    assert_eq!(
        view::command(&json!("ls -la"), "exec_command").as_deref(),
        Some("ls -la")
    );
    assert_eq!(
        view::command(
            &json!({"code":"await tools.exec_command({cmd: 'ls'})"}),
            "functions.exec"
        ),
        None
    );
    assert_eq!(view::command(&json!("some text"), "write_file"), None);
}
#[test]
fn header_title() {
    assert_eq!(
        view::title(
            &json!({"description":"Build the UI package","command":"swift build"}),
            Some("swift build"),
            None
        ),
        "Build the UI package"
    );
    assert_eq!(
        view::title(
            &json!({}),
            Some("\n  swift build \\\n  --package-path ui"),
            None
        ),
        "swift build \\"
    );
    assert_eq!(
        view::title(&json!({}), Some(&"x".repeat(5000)), None)
            .chars()
            .count(),
        200
    );
    for (input, expected) in [
        (
            json!({"offset":300,"limit":61}),
            "SessionDetail.swift · lines 300–360",
        ),
        (json!({"limit":40}), "SessionDetail.swift · lines 1–40"),
        (json!({"offset":12}), "SessionDetail.swift · from line 12"),
        (json!({}), "SessionDetail.swift"),
    ] {
        assert_eq!(
            view::title(&input, None, Some("/work/ui/SessionDetail.swift")),
            expected
        );
    }
    assert_eq!(
        view::title(&json!({"skill":"research"}), None, None),
        "research"
    );
    assert_eq!(
        view::title(
            &json!({"pattern":"ShellRecord","path":"/work/ui/Sources"}),
            None,
            None
        ),
        "\"ShellRecord\" in Sources"
    );
    assert_eq!(
        view::title(&json!({"pattern":"**/*.swift"}), None, None),
        "\"**/*.swift\""
    );
    assert_eq!(view::title(&Value::Null, None, None), "");
}
#[test]
fn a_pi_tool_names_its_file_by_path() {
    let input = json!({"path":"/work/ui/a.swift","offset":3,"limit":2});
    let path = view::path(&input);
    assert_eq!(path.as_deref(), Some("/work/ui/a.swift"));
    assert_eq!(
        view::title(&input, None, path.as_deref()),
        "a.swift · lines 3–4"
    );
}
#[test]
fn read_range_edges() {
    for (input, expected) in [
        (
            json!({"offset":2,"limit":i64::MAX}),
            "a.swift · from line 2".into(),
        ),
        (
            json!({"offset":1,"limit":i64::MAX}),
            format!("a.swift · lines 1–{}", i64::MAX),
        ),
        (json!({"limit":0}), "a.swift · from line 1".into()),
        (
            json!({"offset":5,"limit":i64::MIN}),
            "a.swift · from line 5".into(),
        ),
    ] {
        assert_eq!(view::title(&input, None, Some("/work/a.swift")), expected);
    }
}
#[test]
fn row_text_uses_header_title() {
    assert_eq!(
        texts(&build(vec![call(
            "grep",
            "Grep",
            json!({"pattern":"ShellRecord","path":"/work/ui/Sources"})
        )])),
        ["Grep · \"ShellRecord\" in Sources"]
    );
}
#[test]
fn diff_counts() {
    let diff: view::Diff=serde_json::from_value(json!({"tool_call_id":"edit-1","path":"/work/a.swift","hunks":[{"old_start":1,"old_lines":2,"new_start":1,"new_lines":3,"lines":[" keep","-old","+new","+more"]},{"old_start":9,"old_lines":1,"new_start":10,"new_lines":0,"lines":["-gone"]}]})).unwrap();
    let preview = DiffPreview::new(&diff);
    assert_eq!(preview.added * 2, 4);
    assert_eq!(preview.removed * 2, 4);
    assert!(
        tool(&build(vec![call("read", "Read", Value::Null)])[0])
            .diffs
            .is_empty()
    );
}
#[test]
fn exit_code() {
    assert_eq!(
        view::exit_code(Some("Exit code 1\nusage: swarm"), Some("false"), false),
        Some(1)
    );
    assert_eq!(
        view::exit_code(Some("Exit code 127"), Some("false"), false),
        Some(127)
    );
    assert_eq!(
        view::exit_code(Some("build ok\nExit code 1"), Some("false"), false),
        None
    );
    assert_eq!(view::exit_code(Some("Exit code 1"), None, false), None);
    assert_eq!(view::exit_code(None, Some("false"), false), None);
}
#[test]
fn previews_bound_wire_bytes_and_keep_full_counts() {
    let huge = (0..100000)
        .map(|index| format!("line {index}\n"))
        .collect::<String>();
    let records = records(
        vec![
            call("giant", "Bash", json!({"command":"print"})),
            update("giant", "completed", &huge),
        ],
        false,
    );
    assert!(records[1].event["content"].as_str().unwrap().len() < 12000);
    let rows = transcript::rows(&records, false, false);
    let preview = tool(&rows[0]).output.as_ref().unwrap();
    assert_eq!(preview.full_lines, 100000);
    assert_eq!(preview.full_bytes, huge.len());
    assert!(serde_json::to_vec(&rows).unwrap().len() < 2500);
    let wire = serde_json::to_value(&rows).unwrap();
    assert!(wire[0].get("collapsed").is_none());
    assert!(wire[0].get("steps").is_none());
    for field in [
        "diffs",
        "path",
        "duration_seconds",
        "exit_code",
        "skill_body",
    ] {
        assert!(wire[0]["tool"].get(field).is_none(), "{field}");
    }
    assert_eq!(wire[0]["tool"]["output"]["is_truncated"], true);
    let small = TextPreview::new("done", 120);
    assert!(
        serde_json::to_value(small)
            .unwrap()
            .get("is_truncated")
            .is_none()
    );
    let plain = Row::new(RowKind::User, "hello".into(), "user".into());
    assert_eq!(
        serde_json::to_value(plain).unwrap(),
        json!({"id":"user","kind":"user","text":"hello","order":[0,0]})
    );
}
