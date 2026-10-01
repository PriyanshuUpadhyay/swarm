use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

/// An explicit SWARM_HOME is used without reading HOME, so it works when HOME is unset or is not
/// UTF-8 (ADR 0027).
#[test]
fn explicit_swarm_home_works_without_a_usable_home() {
    for (name, home) in [
        ("unset", None),
        ("not-utf8", Some(OsStr::from_bytes(b"/tmp/\xff"))),
    ] {
        let swarm_home =
            std::env::temp_dir().join(format!("swarm-home-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&swarm_home);
        let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
        command
            .env_clear()
            .env("SWARM_HOME", &swarm_home)
            .arg("init");
        if let Some(home) = home {
            command.env("HOME", home);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "HOME {name}: {output:?}");
        assert!(swarm_home.join(".swarm/swarm.db").is_file(), "HOME {name}");
        std::fs::remove_dir_all(&swarm_home).unwrap();
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-home-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn swarm(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .args(args)
        .output()
        .unwrap()
}

/// Each file under `dir` with its bytes, so a test can show that nothing changed.
fn snapshot(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(snapshot(&path));
        } else {
            files.push((path.clone(), std::fs::read(&path).unwrap()));
        }
    }
    files.sort();
    files
}

/// Another tool's `~/.swarm` (ADR 0036): swarm writes nothing in it and says why.
#[test]
fn init_refuses_a_swarm_folder_that_another_tool_made() {
    let notes = scratch("other-tool-notes");
    std::fs::create_dir_all(notes.join(".swarm/cache")).unwrap();
    std::fs::write(
        notes.join(".swarm/cache/notes.txt"),
        "the other tool's data",
    )
    .unwrap();
    // Another tool's SQLite file that has swarm's name but not swarm's tables.
    let database = scratch("other-tool-database");
    std::fs::create_dir_all(database.join(".swarm")).unwrap();
    rusqlite::Connection::open(database.join(".swarm/swarm.db"))
        .unwrap()
        .execute_batch("CREATE TABLE bookmark (url TEXT);")
        .unwrap();

    for home in [&notes, &database] {
        let before = snapshot(home);
        let output = swarm(home, &["init"]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{home:?}: {output:?}");
        assert!(
            stderr.contains(&home.join(".swarm").display().to_string())
                && stderr.contains("SWARM_HOME"),
            "{stderr}"
        );
        assert_eq!(snapshot(home), before, "{home:?}");
        std::fs::remove_dir_all(home).unwrap();
    }
}

/// An empty folder is claimed, and a home from a swarm older than the marker is adopted.
#[test]
fn init_claims_an_empty_home_and_adopts_an_older_swarms_home() {
    let empty = scratch("empty");
    std::fs::create_dir_all(empty.join(".swarm")).unwrap();
    let output = swarm(&empty, &["init"]);
    assert!(output.status.success(), "{output:?}");
    assert!(empty.join(".swarm/swarm-home").is_file());
    std::fs::remove_dir_all(&empty).unwrap();

    let older = scratch("older");
    assert!(swarm(&older, &["init"]).status.success());
    std::fs::remove_file(older.join(".swarm/swarm-home")).unwrap();
    let db = std::fs::read(older.join(".swarm/swarm.db")).unwrap();
    let output = swarm(&older, &["sessions", "--json"]);
    assert!(output.status.success(), "{output:?}");
    assert!(older.join(".swarm/swarm-home").is_file());
    assert_eq!(std::fs::read(older.join(".swarm/swarm.db")).unwrap(), db);
    std::fs::remove_dir_all(&older).unwrap();
}
