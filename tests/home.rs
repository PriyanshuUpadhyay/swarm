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
