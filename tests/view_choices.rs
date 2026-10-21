mod sidebar_fixture;
use serde_json::json;
use sidebar_fixture::*;
use std::{
    fs,
    fs::OpenOptions,
    time::{Duration, Instant},
};
use swarm::view::{
    choices::{self, ChoiceWrite, OwnerChoices},
    choices_store::{self, StoreError},
};
#[test]
fn empty_home_absent_null_keys_and_separate_homes() {
    let a = ChoicesHome::new();
    let b = ChoicesHome::new();
    let before = choices_store::load(&a.root, true).unwrap();
    assert_eq!(before.owner, OwnerChoices::default());
    assert_eq!(
        serde_json::from_str::<OwnerChoices>("{}").unwrap(),
        OwnerChoices::default()
    );
    assert_eq!(
        serde_json::from_str::<OwnerChoices>(r#"{"pinned":null,"tabs":null,"fields":null}"#)
            .unwrap(),
        OwnerChoices::default()
    );
    choices_store::update(&a.root, Some(&before.revision), |o, _| {
        o.pinned.insert("/repo".into());
        Ok(())
    })
    .unwrap();
    assert!(
        choices_store::load(&b.root, true)
            .unwrap()
            .owner
            .pinned
            .is_empty()
    );
}
#[test]
fn round_trip_preserves_shared_format_and_foreign_keys() {
    let home = ChoicesHome::new();
    fs::write(home.root.join("choices.json"),json!({"prefs":{"theme":"dark"},"projectDefaults":{"/repo":{"provider":"claude","model":"sonnet"}},"future":{"nested":42},"plainFolders":["/folder"]}).to_string()).unwrap();
    let before = choices_store::load(&home.root, true).unwrap();
    let after = choices_store::update(&home.root, Some(&before.revision), |o, s| {
        o.pinned.insert("/folder".into());
        s.selected_workspace = Some("/folder".into());
        Ok(())
    })
    .unwrap();
    assert_eq!(after.owner.prefs, before.owner.prefs);
    assert_eq!(after.owner.project_defaults, before.owner.project_defaults);
    assert_eq!(after.owner.extra, before.owner.extra);
    let raw: serde_json::Value =
        serde_json::from_slice(&fs::read(home.root.join("choices.json")).unwrap()).unwrap();
    assert!(raw.get("selectedWorkspace").is_none());
    assert!(raw.get("revision").is_none());
    assert!(home.root.join("view-state.json").exists());
}
#[test]
fn stale_revision_changes_no_bytes() {
    let home = ChoicesHome::new();
    let before = choices_store::load(&home.root, true).unwrap();
    choices_store::update(&home.root, Some(&before.revision), |o, _| {
        o.names.insert("/repo".into(), "Title".into());
        Ok(())
    })
    .unwrap();
    let raw = fs::read(home.root.join("choices.json")).unwrap();
    assert!(matches!(
        choices_store::update(&home.root, Some(&before.revision), |o, _| {
            o.names.clear();
            Ok(())
        }),
        Err(StoreError::Stale)
    ));
    assert_eq!(raw, fs::read(home.root.join("choices.json")).unwrap());
    assert!(!home.root.join("view-state.json").exists());
}
#[test]
fn bad_file_is_backed_up_but_bad_view_state_is_unchanged() {
    let home = ChoicesHome::new();
    fs::write(home.root.join("choices.json"), "{bad").unwrap();
    let saved = choices_store::load(&home.root, true).unwrap();
    assert_eq!(saved.owner, OwnerChoices::default());
    assert!(!home.root.join("choices.json").exists());
    assert!(fs::read_dir(&home.root).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("choices.json.bad-")
    }));
    fs::write(home.root.join("view-state.json"), "{broken").unwrap();
    assert!(choices_store::load(&home.root, true).is_err());
    assert_eq!(
        fs::read_to_string(home.root.join("view-state.json")).unwrap(),
        "{broken"
    );
}
#[test]
fn unclaimed_home_never_writes() {
    let home = ChoicesHome::new();
    fs::remove_file(home.root.join("swarm-home")).unwrap();
    assert!(
        choices_store::load(&home.root, true)
            .unwrap()
            .owner
            .pinned
            .is_empty()
    );
    assert!(choices_store::update(&home.root, None, |_, _| Ok(())).is_err());
    assert_eq!(fs::read_dir(&home.root).unwrap().count(), 0);
}
#[test]
fn held_lock_times_out_and_immediate_read_keeps_file() {
    let home = ChoicesHome::new();
    let held = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(home.root.join("choices.lock"))
        .unwrap();
    held.lock().unwrap();
    let started = Instant::now();
    assert!(matches!(
        choices_store::load(&home.root, false),
        Err(StoreError::Busy)
    ));
    assert!(started.elapsed() < Duration::from_millis(200));
    let started = Instant::now();
    assert!(matches!(
        choices_store::load(&home.root, true),
        Err(StoreError::Busy)
    ));
    assert!(started.elapsed() >= Duration::from_millis(900));
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(held);
    assert!(choices_store::load(&home.root, true).is_ok());
}
#[test]
fn launch_read_waits_for_writer() {
    let home = ChoicesHome::new();
    let held = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(home.root.join("choices.lock"))
        .unwrap();
    held.lock().unwrap();
    let root = home.root.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        fs::write(root.join("choices.json"), r#"{"names":{"/repo":"Writer"}}"#).unwrap();
        drop(held);
    });
    let saved = choices_store::load(&home.root, true).unwrap();
    writer.join().unwrap();
    assert_eq!(saved.owner.names["/repo"], "Writer");
}
#[test]
fn view_state_encoding_and_invalid_operation_rollback() {
    let home = ChoicesHome::new();
    let saved = choices_store::load(&home.root, true).unwrap();
    let after = choices_store::update(&home.root, Some(&saved.revision), |_, s| {
        s.collapsed.insert("workspace:/repo".into());
        s.last_seen.insert("chat".into(), 5);
        Ok(())
    })
    .unwrap();
    assert_eq!(after.state.collapsed.len(), 1);
    assert_eq!(after.state.last_seen["chat"], 5);
    let raw = fs::read(home.root.join("view-state.json")).unwrap();
    assert!(
        choices_store::update(&home.root, None, |o, _| {
            o.pinned.insert("no".into());
            Err("Invalid".into())
        })
        .is_err()
    );
    assert_eq!(raw, fs::read(home.root.join("view-state.json")).unwrap());
    assert!(!home.root.join("choices.json").exists());
}
#[test]
fn operation_json_validation_and_plain_folder_pruning() {
    let write: ChoiceWrite =
        serde_json::from_value(json!({"revision":"r","op":"pin","workspace":"/repo/wt/main"}))
            .unwrap();
    let mut o = owner();
    let mut s = Default::default();
    choices::apply(&mut o, &mut s, &tree(), write.operation, 100).unwrap();
    assert!(o.pinned.contains("/repo/wt/main"));
    assert!(
        serde_json::from_value::<ChoiceWrite>(
            json!({"revision":"r","op":"pin","workspace":"x","unexpected":1})
        )
        .is_err()
    );
    o.plain_folders.insert("/repo".into());
    o.tabs.insert("/repo/wt/main".into(), Default::default());
    o.remove_project("/repo", &["/repo/wt/main".into()]);
    assert!(o.pinned.is_empty() && o.plain_folders.is_empty() && o.tabs.is_empty());
}

#[test]
fn preference_ops_set_swift_keys_mute_listed_projects_and_refuse_bad_values() {
    let tree = tree();
    let project = tree.projects[0].path.clone();
    let mut o = owner();
    o.prefs = json!({"usageHUD": true, "notices": {"startAtLogin": false}});
    let mut s = Default::default();
    let mut apply = |o: &mut OwnerChoices, op: serde_json::Value| {
        let mut body = json!({"revision": "r"});
        body.as_object_mut()
            .unwrap()
            .extend(op.as_object().unwrap().clone());
        let write: ChoiceWrite = serde_json::from_value(body).unwrap();
        choices::apply(o, &mut s, &tree, write.operation, 100)
    };
    for op in [
        json!({"op":"pref","key":"theme","value":"light"}),
        json!({"op":"pref","key":"sendKey","value":"commandReturn"}),
        json!({"op":"pref","key":"fontSans","value":"avenir"}),
        json!({"op":"pref","key":"fontMono","value":"menlo"}),
        json!({"op":"pref","key":"usageHUDAsked","value":true}),
        json!({"op":"pref","key":"notices.post","value":false}),
        json!({"op":"pref","key":"notices.sound","value":false}),
        json!({"op":"pref","key":"notices.done","value":false}),
        json!({"op":"pref","key":"notices.badge","value":false}),
        json!({"op":"mute_project","project":project,"muted":true}),
        json!({"op":"mute_project","project":project,"muted":true}),
    ] {
        apply(&mut o, op).unwrap();
    }
    assert_eq!(
        o.prefs,
        json!({"usageHUD": true, "theme": "light", "sendKey": "commandReturn", "fontSans": "avenir", "fontMono": "menlo", "usageHUDAsked": true, "notices": {
            "startAtLogin": false, "post": false, "sound": false, "done": false, "badge": false,
            "mutedProjects": [project]}})
    );
    apply(
        &mut o,
        json!({"op":"mute_project","project":project,"muted":false}),
    )
    .unwrap();
    assert_eq!(o.prefs["notices"]["mutedProjects"], json!([]));
    let before = o.prefs.clone();
    for op in [
        json!({"op":"pref","key":"theme","value":"blue"}),
        json!({"op":"pref","key":"sendKey","value":true}),
        json!({"op":"pref","key":"fontSans","value":"Comic Sans"}),
        json!({"op":"pref","key":"fontMono","value":"avenir"}),
        json!({"op":"pref","key":"notices.post","value":"no"}),
        json!({"op":"pref","key":"notices.startAtLogin","value":true}),
        json!({"op":"pref","key":"usageHUD","value":true}),
        json!({"op":"pref","key":"usageHUDAsked","value":"yes"}),
        json!({"op":"mute_project","project":"/not/listed","muted":true}),
    ] {
        assert!(apply(&mut o, op).is_err());
    }
    assert_eq!(o.prefs, before);
    o.prefs["notices"] = json!("broken");
    assert!(
        apply(
            &mut o,
            json!({"op":"pref","key":"notices.post","value":true})
        )
        .is_err()
    );
    assert_eq!(o.prefs["notices"], "broken");
}

#[test]
fn process_lock_reads_completed_foreign_write() {
    use std::io::{BufRead, BufReader};
    let home = ChoicesHome::new();
    let mut child = std::process::Command::new("/usr/bin/python3")
        .args([
            "-c",
            r#"import sys,time,json,fcntl
from pathlib import Path
root=Path(sys.argv[1])
with (root/'choices.lock').open('a+') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX)
 print('locked',flush=True)
 time.sleep(.15)
 (root/'choices.json').write_text(json.dumps({'names':{'/repo':'Foreign writer'}}))
"#,
        ])
        .arg(&home.root)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let saved = choices_store::load(&home.root, true);
    let status = child.wait().unwrap();
    assert!(status.success());
    assert_eq!(line.trim(), "locked");
    assert_eq!(saved.unwrap().owner.names["/repo"], "Foreign writer");
}
#[test]
fn unchanged_updates_do_not_write_and_dismissed_children_are_pruned() {
    let home = ChoicesHome::new();
    let saved = choices_store::load(&home.root, true).unwrap();
    let after = choices_store::update(&home.root, Some(&saved.revision), |_, _| Ok(())).unwrap();
    assert_eq!(saved.revision, after.revision);
    assert!(!home.root.join("choices.json").exists());
    assert!(!home.root.join("view-state.json").exists());
    let mut state = swarm::view::choices::ViewState::default();
    let t = tree();
    let chat = t.chats().next().unwrap();
    state
        .dismissed_children
        .insert(chat.key.clone(), vec!["child".into()]);
    state
        .dismissed_children
        .insert("gone".into(), vec!["old".into()]);
    state.record_first_sight(&t);
    assert!(state.dismissed_children.contains_key(&chat.key));
    assert!(!state.dismissed_children.contains_key("gone"));
}
