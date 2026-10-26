//! Server update: the page tells the owner about a newer swarm, upgrades it with Homebrew on one
//! press, and restarts `swarm ui` on the new binary on a second press (ADR 0118). The second press
//! also restarts the HUD login item when the upgrade changed the HUD app.

use super::*;

/// The formula that `brew upgrade swarm` installs. The page offers its version, not the newest
/// GitHub release, because the tap is written after a release and can lag behind it.
const FORMULA_URL: &str =
    "https://raw.githubusercontent.com/PriyanshuUpadhyay/homebrew-tap/main/Formula/swarm.rb";

/// How long one tap read stays good, so many pages and reloads do not read it again.
const CHECK_TTL: Duration = Duration::from_secs(3600);
/// curl's own limit for the tap read; the outer limit is longer, so curl's error shows first.
const CURL_SECS: u64 = 10;
/// `brew update` reads every tap, and `brew upgrade swarm` downloads and links a release.
const BREW_UPDATE_WAIT: Duration = Duration::from_secs(300);
const BREW_UPGRADE_WAIT: Duration = Duration::from_secs(900);
/// Each launchctl call and `swarm --version`.
const SHORT_WAIT: Duration = Duration::from_secs(10);
/// The last lines of a failed step that the error toast shows.
const ERROR_LINES: usize = 5;
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A SHA-256 digest.
type Digest = [u8; 32];

static LATEST: Mutex<Option<(std::time::Instant, String)>> = Mutex::new(None);
/// Held by an upgrade or a restart, so only one of them runs at a time.
static UPDATE_BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Set by an upgrade that changed the HUD of a set-up login item; the restart takes it.
static HUD_CHANGED: AtomicBool = AtomicBool::new(false);

/// The SHA-256 of the HUD executable that the login item runs through `opt`, or None without one.
// ponytail: the executable only; a change to the bundle's resources alone does not restart it.
fn hud_hash(prefix: &Path) -> Option<Digest> {
    use sha2::Digest;
    let path = prefix.join("opt/swarm/libexec/SwarmHUD.app/Contents/MacOS/SwarmHUD");
    Some(sha2::Sha256::digest(std::fs::read(path).ok()?).into())
}

/// Whether the upgrade brought a different HUD. With no HUD after it, there is nothing to start.
fn hud_changed(before: Option<Digest>, after: Option<Digest>) -> bool {
    after.is_some() && before != after
}

/// Whether the owner set up the HUD login item (ADR 0087).
fn hud_set_up() -> bool {
    std::env::var_os("HOME").is_some_and(|home| swarm::hud::plist_path(Path::new(&home)).exists())
}

/// The Homebrew prefix of this swarm, read once. `brew upgrade` deletes this version's Cellar
/// folder, so the path cannot be resolved again after the upgrade; `start` reads it first.
pub(super) fn brew_prefix() -> Option<&'static Path> {
    static PREFIX: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    PREFIX
        .get_or_init(|| {
            let exe = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
            // brew cannot upgrade a cargo or checkout build.
            swarm::hud::cellar_prefix(&exe)
        })
        .as_deref()
}

/// The version in the formula's release URL: `.../releases/download/v0.0.38/...`.
fn formula_version(formula: &str) -> Option<&str> {
    let rest = formula.split_once("/releases/download/v")?.1;
    let version = rest.split_once('/')?.0;
    version_parts(version).map(|_| version)
}

/// The `swarm` that `opt` names, which is the installed version.
fn installed_swarm(prefix: &Path) -> PathBuf {
    prefix.join("opt/swarm/bin/swarm")
}

fn version_parts(version: &str) -> Option<Vec<u64>> {
    version.split('.').map(|part| part.parse().ok()).collect()
}

/// Whether `latest` sorts above `current` as dotted numbers. A version that does not parse is
/// never newer.
fn newer(latest: &str, current: &str) -> bool {
    match (version_parts(latest), version_parts(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// The last `lines` lines of `text`, so a long brew log does not fill the error toast.
fn tail(text: &str, lines: usize) -> String {
    let all: Vec<_> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Runs `program` without a shell, with Homebrew's prompts off. Its stderr, else its stdout, is
/// the error on a failed exit.
async fn run(program: &Path, args: &[&str], timeout: Duration) -> Result<String, ApiError> {
    let child = tokio::process::Command::new(program)
        .args(args)
        .env("HOMEBREW_NO_ASK", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        // The running server still reads its own version's folder, such as its transcript tool,
        // until the restart, so brew keeps old versions; its next cleanup removes them.
        .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let name = program.file_name().unwrap_or_default().to_string_lossy();
    let output = match tokio::time::timeout(timeout, child).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("{name}: {err}"),
            ));
        }
        Err(_) => {
            let message = format!("{name} {} timed out", args.join(" "));
            return Err(error(StatusCode::GATEWAY_TIMEOUT, &message));
        }
    };
    if !output.status.success() {
        let text = failure_text(&output);
        let message = format!(
            "{name} {} failed: {}",
            args.join(" "),
            tail(&text, ERROR_LINES)
        );
        return Err(error(StatusCode::BAD_GATEWAY, &message));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `{current, latest, available}`. `latest` is null for a swarm that brew did not install.
pub(super) async fn update_read() -> Result<Json<Value>, ApiError> {
    let current = VERSION;
    if brew_prefix().is_none() {
        return Ok(Json(
            json!({"current": current, "latest": null, "available": false}),
        ));
    }
    let cached = LATEST
        .lock()
        .unwrap()
        .clone()
        .filter(|(at, _)| at.elapsed() < CHECK_TTL);
    let latest = match cached {
        Some((_, latest)) => latest,
        None => {
            let curl = Path::new("/usr/bin/curl");
            let max_time = CURL_SECS.to_string();
            let args = [
                "--fail",
                "--silent",
                "--show-error",
                "--max-time",
                &max_time,
                FORMULA_URL,
            ];
            let formula = run(curl, &args, Duration::from_secs(CURL_SECS + 5)).await?;
            let latest = formula_version(&formula)
                .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "the tap formula names no version"))?
                .to_string();
            *LATEST.lock().unwrap() = Some((std::time::Instant::now(), latest.clone()));
            latest
        }
    };
    let available = newer(&latest, current);
    Ok(Json(
        json!({"current": current, "latest": latest, "available": available}),
    ))
}

/// The one lock of the upgrade and the restart, or a 409 while it is held.
fn lock() -> Result<tokio::sync::MutexGuard<'static, ()>, ApiError> {
    UPDATE_BUSY.try_lock().map_err(|_| {
        error(
            StatusCode::CONFLICT,
            "an upgrade or a restart is already running",
        )
    })
}

/// Step one: `brew update`, then `brew upgrade swarm`, and the version that is now installed.
pub(super) async fn update_write() -> Result<Json<Value>, ApiError> {
    let prefix = brew_prefix().ok_or_else(|| {
        error(
            StatusCode::CONFLICT,
            "this swarm is not a Homebrew install; rebuild it from its checkout",
        )
    })?;
    let running = lock()?;
    // The lock, brew, and the results all live in this task, so a closed page that drops the
    // request neither frees the lock while brew runs nor loses the HUD mark.
    let upgrade = tokio::spawn(async move {
        let _running = running;
        let before = hud_set_up().then(|| hud_hash(prefix));
        let brew = prefix.join("bin/brew");
        run(&brew, &["update", "--quiet"], BREW_UPDATE_WAIT).await?;
        let upgraded = run(&brew, &["upgrade", "swarm"], BREW_UPGRADE_WAIT).await;
        let changed = before.is_some_and(|before| hud_changed(before, hud_hash(prefix)));
        // Marked now, so a step that fails keeps the HUD change for the next try, also a brew
        // that fails after it moved `opt`. A repeat upgrade that finds the HUD unchanged keeps
        // the mark of the one before it.
        let pending = HUD_CHANGED.fetch_or(changed, Ordering::SeqCst) || changed;
        upgraded?;
        let printed = run(&installed_swarm(prefix), &["--version"], SHORT_WAIT).await?;
        // `swarm 0.0.38 abc1234 branch`
        let version = printed
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| {
                let message = format!("swarm --version printed `{}`", printed.trim());
                error(StatusCode::BAD_GATEWAY, &message)
            })?
            .to_string();
        if !newer(&version, VERSION) {
            // A local build sorts above the release of its tag, so brew keeps it.
            let message = format!(
                "brew left swarm at {version}; run `brew reinstall --formula priyanshuupadhyay/tap/swarm`"
            );
            return Err(error(StatusCode::CONFLICT, &message));
        }
        *LATEST.lock().unwrap() = None;
        Ok(Json(json!({"version": version, "hud": pending})))
    });
    upgrade
        .await
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?
}

/// SIGTERM to a login item that runs. KeepAlive starts it again with the app that `opt` names
/// now, and launchd tries again after ThrottleInterval when that first start fails, as a new ad
/// hoc app can (ADR 0125). `kickstart -k` would wait out the ThrottleInterval. A login item that
/// does not run is started by the new `swarm ui` (`start_hud`).
async fn hud_restart() -> Result<(), ApiError> {
    let home = std::env::var_os("HOME").unwrap_or_default();
    let status = tokio::task::spawn_blocking(move || swarm::hud::status(Path::new(&home)));
    let status = tokio::time::timeout(SHORT_WAIT, status)
        .await
        .map_err(|_| error(StatusCode::GATEWAY_TIMEOUT, "launchctl print timed out"))?
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    if status.running {
        let launchctl = Path::new("/bin/launchctl");
        let target = swarm::hud::service_target();
        run(launchctl, &["kill", "SIGTERM", &target], SHORT_WAIT).await?;
    }
    Ok(())
}

/// Step two: restart the HUD login item when the upgrade changed it, then start `swarm ui` of the
/// installed version on this port. That finds this server of an older build, stops it, and starts
/// its own, and the open pages reload on the new build.
pub(super) async fn update_restart(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    use std::os::unix::process::CommandExt;
    if !app.ui {
        // The new `swarm ui` would stop the recorded `swarm ui` server, not this one.
        return Err(error(
            StatusCode::CONFLICT,
            "this server is `swarm serve`; stop it and run `swarm serve` again",
        ));
    }
    let prefix = brew_prefix().ok_or_else(|| {
        error(
            StatusCode::CONFLICT,
            "this swarm is not a Homebrew install; run `swarm ui` again",
        )
    })?;
    let running = lock()?;
    let fail = |err: std::io::Error| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
    let hud = HUD_CHANGED.load(Ordering::SeqCst);
    if hud {
        let restarted = hud_restart().await;
        // One try only: the server is not touched yet, and the next press restarts it alone.
        HUD_CHANGED.store(false, Ordering::SeqCst);
        restarted.map_err(|(_, body)| {
            let err = body["error"].as_str().unwrap_or_default();
            let message = format!(
                "the HUD did not restart: {err}; press Restart server again to restart only the server"
            );
            error(StatusCode::BAD_GATEWAY, &message)
        })?;
    }
    let log = owner_log(&app.root.join("ui.log")).map_err(fail)?;
    std::process::Command::new(installed_swarm(prefix))
        .args(["ui", "--port", &app.port.to_string()])
        .env("SWARM_HOME", app.root.parent().expect("swarm home"))
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(fail)?)
        .stderr(log)
        // Its own process group, so it outlives this server, which it stops.
        .process_group(0)
        .spawn()
        .map_err(fail)?;
    // The new server stops this one, so the lock stays held and a second page cannot start a
    // second `swarm ui`. If it never stops this one, `swarm ui` in a terminal does.
    std::mem::forget(running);
    Ok(Json(json!({"ok": true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::test_support::*;

    #[tokio::test]
    async fn a_swarm_serve_server_refuses_a_restart() {
        let mut fixture = Fixture::new();
        Arc::get_mut(&mut fixture.app).unwrap().ui = false;
        let (status, body) = update_restart(State(fixture.app.clone()))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("`swarm serve`"));
    }

    #[test]
    fn reads_the_tap_formula() {
        let formula = r#"  url "https://github.com/PriyanshuUpadhyay/swarm/releases/download/v0.0.38/swarm-0.0.38-macos-arm64.tar.gz""#;
        assert_eq!(formula_version(formula), Some("0.0.38"));
        assert_eq!(
            formula_version("url \"https://example.com/swarm.tar.gz\""),
            None
        );
        assert_eq!(formula_version("/releases/download/vlatest/x"), None);
    }

    #[test]
    fn keeps_the_last_lines_of_a_failure() {
        assert_eq!(tail("a\nb\nc", 2), "b\nc");
        assert_eq!(tail("a", 5), "a");
        assert_eq!(tail("", 5), "");
    }

    #[test]
    fn compares_versions_as_numbers() {
        assert!(newer("0.0.38", "0.0.37"));
        assert!(newer("0.0.100", "0.0.99"));
        assert!(newer("0.1.0", "0.0.99"));
        assert!(!newer("0.0.37", "0.0.37"));
        assert!(!newer("0.0.36", "0.0.37"));
        assert!(!newer("", "0.0.37"));
    }

    #[test]
    fn a_hud_changed_only_when_its_new_executable_differs() {
        let (old, new) = (Some([1u8; 32]), Some([2u8; 32]));
        assert!(hud_changed(old, new));
        assert!(!hud_changed(old, old));
        assert!(hud_changed(None, new));
        assert!(!hud_changed(old, None));
        assert!(!hud_changed(None, None));
    }
}
