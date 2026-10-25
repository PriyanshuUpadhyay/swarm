//! `swarm hud`: the LaunchAgent that starts the notch usage HUD at login (ADR 0087).

use std::path::{Path, PathBuf};

pub const HUD_SCHEMA: &str = "swarm.hud.v1";
pub const LABEL: &str = "io.github.priyanshuupadhyay.swarm.hud";

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Status {
    pub installed: bool,
    pub running: bool,
    /// The HUD app that `install` would start, or None when no HUD app is found.
    pub app: Option<String>,
    /// Each running HUD that the login item did not start, such as one that `swarm ui` started
    /// before the login item existed. `install` and `uninstall` stop them.
    pub strays: Vec<u32>,
}

/// `SWARM_HUD_APP` alone when set, else the app that ships beside this swarm (ADR 0079).
pub fn app() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SWARM_HUD_APP") {
        return Some(PathBuf::from(path)).filter(|path| path.exists());
    }
    crate::paths::bundled("SwarmHUD.app")
}

/// `<prefix>/Cellar/swarm/<version>/rest` as `<prefix>/opt/swarm/rest`, so a LaunchAgent written
/// by one formula version still works after `brew upgrade` removes that version's folder.
pub fn stable_path(path: &Path) -> PathBuf {
    let Some(prefix) = cellar_prefix(path) else {
        return path.to_path_buf();
    };
    // `Cellar`, `swarm`, and the version come after the prefix.
    let rest: PathBuf = path.strip_prefix(&prefix).unwrap().iter().skip(3).collect();
    if rest.as_os_str().is_empty() {
        return path.to_path_buf();
    }
    prefix.join("opt/swarm").join(rest)
}

/// `<prefix>` when `path` is `<prefix>/Cellar/swarm/...`, else None, as for a cargo or checkout
/// build.
pub fn cellar_prefix(path: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = path.components().collect();
    let at = parts
        .windows(2)
        .position(|pair| pair[0].as_os_str() == "Cellar" && pair[1].as_os_str() == "swarm")?;
    Some(parts[..at].iter().collect())
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The LaunchAgent. `SuccessfulExit = false` restarts a crash but keeps a Quit from the HUD menu;
/// `ThrottleInterval` keeps a HUD app removed by `brew uninstall` from respawning every 10 s.
/// `PATH` is the installer's, because launchd's bare one has no `codex` for a usage refresh.
pub fn plist(app: &Path, swarm_bin: &Path, path: &str) -> String {
    let program = xml_escape(&app.join("Contents/MacOS/SwarmHUD").to_string_lossy());
    let swarm_bin = xml_escape(&swarm_bin.to_string_lossy());
    let path = xml_escape(path);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{program}</string>
	</array>
	<key>EnvironmentVariables</key>
	<dict>
		<key>SWARM_BIN</key>
		<string>{swarm_bin}</string>
		<key>PATH</key>
		<string>{path}</string>
	</dict>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>ThrottleInterval</key>
	<integer>60</integer>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#
    )
}

pub fn plist_path(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

/// `launchctl print` names a loaded job's state on a line such as `state = running`.
pub fn running(launchctl_print: &str) -> bool {
    launchctl_print
        .lines()
        .any(|line| line.trim() == "state = running")
}

/// The `pid = N` line of a running job in `launchctl print`.
pub fn job_pid(launchctl_print: &str) -> Option<u32> {
    launchctl_print
        .lines()
        .find_map(|line| line.trim().strip_prefix("pid = ")?.parse().ok())
}

/// The pids in `pgrep` output, without the login item's own.
pub fn others(pgrep: &str, job: Option<u32>) -> Vec<u32> {
    pgrep
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .filter(|pid| Some(*pid) != job)
        .collect()
}

unsafe extern "C" {
    pub(crate) fn getuid() -> u32;
    fn kill(pid: i32, signal: i32) -> i32;
}

fn strays(job: Option<u32>) -> Vec<u32> {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { getuid() }.to_string();
    let pgrep = std::process::Command::new("/usr/bin/pgrep")
        .args(["-x", "-U", &uid, "SwarmHUD"])
        .stdin(std::process::Stdio::null())
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    others(&pgrep, job)
}

fn stop(pids: &[u32]) {
    for &pid in pids {
        // SAFETY: kill only sends a signal; a pid that is gone returns an error we ignore.
        unsafe { kill(pid as i32, 15) };
    }
}

fn domain() -> String {
    // SAFETY: getuid has no preconditions and cannot fail.
    format!("gui/{}", unsafe { getuid() })
}

fn launchctl(args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new("/bin/launchctl")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
}

/// The login item's launchctl service target, `gui/<uid>/<label>`.
pub fn target() -> String {
    format!("{}/{LABEL}", domain())
}

/// Start the installed login item now, as a login would.
pub fn kickstart() -> std::io::Result<std::process::Output> {
    launchctl(&["kickstart", &target()])
}

pub fn status(home: &Path) -> Status {
    let print = launchctl(&["print", &target()])
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let running = running(&print);
    Status {
        installed: plist_path(home).exists(),
        running,
        app: app().map(|path| stable_path(&path).to_string_lossy().into_owned()),
        strays: strays(job_pid(&print).filter(|_| running)),
    }
}

/// The LaunchAgent text that `install` writes, so `swarm setup hud` shows and digests it first.
pub fn planned() -> Result<String, Box<dyn std::error::Error>> {
    let app = app()
        .ok_or("swarm: no SwarmHUD.app beside this swarm; set SWARM_HUD_APP to the app's path")?;
    let swarm_bin = std::env::current_exe()?.canonicalize()?;
    let search = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin:/usr/sbin:/sbin".into());
    Ok(plist(&stable_path(&app), &stable_path(&swarm_bin), &search))
}

pub fn install(home: &Path) -> Result<Status, Box<dyn std::error::Error>> {
    let text = planned()?;
    let path = plist_path(home);
    std::fs::create_dir_all(path.parent().ok_or("swarm: bad LaunchAgents path")?)?;
    std::fs::write(&path, text)?;
    // A loaded job keeps its old plist until it is booted out, and bootout returns before the job
    // is gone; a bootstrap before then fails with error 5. Not loaded at all is fine.
    let target = target();
    let _ = launchctl(&["bootout", &target]);
    for _ in 0..30 {
        if !launchctl(&["print", &target]).is_ok_and(|output| output.status.success()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // The job is gone now, so each HUD left is a stray; one HUD runs after the bootstrap.
    stop(&strays(None));
    let failure = match launchctl(&["bootstrap", &domain(), &path.to_string_lossy()]) {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        Err(error) => Some(error.to_string()),
    };
    if let Some(reason) = failure {
        // A plist left behind would start the HUD at the next login while the switch reads off.
        let _ = std::fs::remove_file(&path);
        return Err(format!("swarm: launchctl bootstrap failed: {reason}").into());
    }
    // RunAtLoad starts the HUD a moment after bootstrap; wait up to 1 s so the reply shows it.
    for _ in 0..10 {
        let now = status(home);
        if now.running {
            return Ok(now);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(status(home))
}

pub fn uninstall(home: &Path) -> Result<Status, Box<dyn std::error::Error>> {
    let _ = launchctl(&["bootout", &target()]);
    stop(&strays(None));
    match std::fs::remove_file(plist_path(home)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    // bootout returns before the HUD has quit; wait up to 1 s so the reply shows it stopped.
    for _ in 0..10 {
        let now = status(home);
        if !now.running {
            return Ok(now);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(status(home))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_path_maps_a_cellar_version_to_opt_and_keeps_other_paths() {
        assert_eq!(
            stable_path(Path::new(
                "/opt/homebrew/Cellar/swarm/0.0.7/libexec/SwarmHUD.app"
            )),
            PathBuf::from("/opt/homebrew/opt/swarm/libexec/SwarmHUD.app")
        );
        assert_eq!(
            stable_path(Path::new("/opt/homebrew/Cellar/swarm/0.0.7/bin/swarm")),
            PathBuf::from("/opt/homebrew/opt/swarm/bin/swarm")
        );
        for kept in [
            "/Users/me/swarm/target/debug/swarm",
            "/opt/homebrew/Cellar/other/1.0/bin/swarm",
            "/opt/homebrew/Cellar/swarm/0.0.7",
        ] {
            assert_eq!(stable_path(Path::new(kept)), PathBuf::from(kept));
        }
    }

    #[test]
    fn cellar_prefix_reads_a_cellar_path_and_refuses_other_paths() {
        let exe = Path::new("/opt/homebrew/Cellar/swarm/0.0.37/bin/swarm");
        assert_eq!(cellar_prefix(exe), Some(PathBuf::from("/opt/homebrew")));
        assert_eq!(cellar_prefix(Path::new("/Users/me/.cargo/bin/swarm")), None);
    }

    #[test]
    fn plist_names_the_app_binary_and_swarm_and_escapes_xml() {
        let text = plist(
            Path::new("/a&b/SwarmHUD.app"),
            Path::new("/opt/homebrew/opt/swarm/bin/swarm"),
            "/opt/homebrew/bin:/usr/bin",
        );
        assert!(text.contains("<string>/a&amp;b/SwarmHUD.app/Contents/MacOS/SwarmHUD</string>"));
        assert!(text.contains(
            "<key>SWARM_BIN</key>\n\t\t<string>/opt/homebrew/opt/swarm/bin/swarm</string>"
        ));
        assert!(text.contains("<key>PATH</key>\n\t\t<string>/opt/homebrew/bin:/usr/bin</string>"));
        assert!(text.contains(&format!("<string>{LABEL}</string>")));
        assert!(text.contains("<key>SuccessfulExit</key>\n\t\t<false/>"));
        assert!(text.contains("<key>ThrottleInterval</key>\n\t<integer>60</integer>"));
        assert!(!text.contains("/a&b"));
    }

    #[test]
    fn a_stray_is_each_hud_pid_but_the_login_items() {
        let print = "gui/501/io.github.priyanshuupadhyay.swarm.hud = {\n\tstate = running\n\tpid = 63253\n}";
        assert_eq!(job_pid(print), Some(63253));
        assert_eq!(job_pid("\tstate = not running\n"), None);
        assert_eq!(others("63253\n88792\n", Some(63253)), vec![88792]);
        assert_eq!(others("63253\n", None), vec![63253]);
        assert!(others("", Some(1)).is_empty());
    }

    #[test]
    fn running_reads_the_state_line_only() {
        assert!(running("\tpid = 42\n\tstate = running\n"));
        assert!(!running("\tstate = not running\n"));
        assert!(!running(""));
    }

    #[test]
    fn plist_path_is_in_the_home_launch_agents() {
        assert_eq!(
            plist_path(Path::new("/Users/me")),
            PathBuf::from(format!("/Users/me/Library/LaunchAgents/{LABEL}.plist"))
        );
    }
}
