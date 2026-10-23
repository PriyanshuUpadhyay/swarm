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

static LATEST: Mutex<Option<(std::time::Instant, String)>> = Mutex::new(None);
static UPGRADE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Set by an upgrade that changed the HUD of a set-up login item; the restart takes it.
static HUD_CHANGED: AtomicBool = AtomicBool::new(false);

/// The SHA-256 of the HUD executable that the login item runs through `opt`, or None without one.
// ponytail: the executable only; a change to the bundle's resources alone does not restart it.
fn hud_hash(prefix: &Path) -> Option<[u8; 32]> {
    use sha2::Digest;
    let path = prefix.join("opt/swarm/libexec/SwarmHUD.app/Contents/MacOS/SwarmHUD");
    Some(sha2::Sha256::digest(std::fs::read(path).ok()?).into())
}

/// Whether the upgrade brought a different HUD. With no HUD after it, there is nothing to start.
pub(super) fn hud_changed(before: Option<[u8; 32]>, after: Option<[u8; 32]>) -> bool {
    after.is_some() && before != after
}

/// Whether the owner set up the HUD login item (ADR 0087).
fn hud_set_up() -> bool {
    std::env::var_os("HOME").is_some_and(|home| swarm::hud::plist_path(Path::new(&home)).exists())
}

/// `<prefix>` when `exe` is `<prefix>/Cellar/swarm/<version>/...`, else None: brew cannot
/// upgrade a cargo or checkout build.
pub(super) fn cellar_prefix(exe: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = exe.components().collect();
    let at = parts
        .windows(2)
        .position(|pair| pair[0].as_os_str() == "Cellar" && pair[1].as_os_str() == "swarm")?;
    Some(parts[..at].iter().collect())
}

/// The Homebrew prefix of this swarm, read once. `brew upgrade` deletes this version's Cellar
/// folder, so the path cannot be resolved again after the upgrade; `start` reads it first.
pub(super) fn brew_prefix() -> Option<&'static Path> {
    static PREFIX: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    PREFIX
        .get_or_init(|| {
            let exe = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
            cellar_prefix(&exe)
        })
        .as_deref()
}

/// The version in the formula's release URL: `.../releases/download/v0.0.38/...`.
pub(super) fn formula_version(formula: &str) -> Option<&str> {
    let rest = formula.split_once("/releases/download/v")?.1;
    let version = rest.split_once('/')?.0;
    version_parts(version).map(|_| version)
}

fn version_parts(version: &str) -> Option<Vec<u64>> {
    version.split('.').map(|part| part.parse().ok()).collect()
}

/// Whether `latest` sorts above `current` as dotted numbers. A version that does not parse is
/// never newer.
pub(super) fn newer(latest: &str, current: &str) -> bool {
    match (version_parts(latest), version_parts(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// Runs `program` without a shell, with Homebrew's prompts off. Its stderr, else its stdout, is
/// the error on a failed exit.
async fn run(program: &Path, args: &[&str], timeout: Duration) -> Result<String, ApiError> {
    let child = tokio::process::Command::new(program)
        .args(args)
        .env("HOMEBREW_NO_ASK", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
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
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let text = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        let message = format!("{name} {} failed: {text}", args.join(" "));
        return Err(error(StatusCode::BAD_GATEWAY, &message));
    }
    Ok(stdout)
}

/// `{current, latest, available}`. `latest` is null for a swarm that brew did not install.
pub(super) async fn update_read() -> Result<Json<Value>, ApiError> {
    let current = env!("CARGO_PKG_VERSION");
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
            let args = [
                "--fail",
                "--silent",
                "--show-error",
                "--max-time",
                "10",
                FORMULA_URL,
            ];
            let formula = run(curl, &args, Duration::from_secs(15)).await?;
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

/// Step one: `brew update`, then `brew upgrade swarm`, and the version that is now installed.
pub(super) async fn update_write() -> Result<Json<Value>, ApiError> {
    let prefix = brew_prefix().ok_or_else(|| {
        error(
            StatusCode::CONFLICT,
            "this swarm is not a Homebrew install; rebuild it from its checkout",
        )
    })?;
    let running = UPGRADE
        .try_lock()
        .map_err(|_| error(StatusCode::CONFLICT, "an upgrade is already running"))?;
    // The lock, brew, and the results all live in this task, so a closed page that drops the
    // request neither frees the lock while brew runs nor loses the HUD mark.
    let upgrade = tokio::spawn(async move {
        let _running = running;
        let before = hud_set_up().then(|| hud_hash(prefix));
        let brew = prefix.join("bin/brew");
        run(&brew, &["update", "--quiet"], Duration::from_secs(300)).await?;
        run(&brew, &["upgrade", "swarm"], Duration::from_secs(900)).await?;
        let hud = before.is_some_and(|before| hud_changed(before, hud_hash(prefix)));
        let swarm = prefix.join("opt/swarm/bin/swarm");
        let printed = run(&swarm, &["--version"], Duration::from_secs(10)).await?;
        // `swarm 0.0.38 abc1234 branch`
        let version = printed
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string();
        if !newer(&version, env!("CARGO_PKG_VERSION")) {
            // A local build sorts above the release of its tag, so brew keeps it.
            let message = format!(
                "brew left swarm at {version}; run `brew reinstall --formula priyanshuupadhyay/tap/swarm`"
            );
            return Err(error(StatusCode::CONFLICT, &message));
        }
        *LATEST.lock().unwrap() = None;
        HUD_CHANGED.store(hud, Ordering::SeqCst);
        Ok(Json(json!({"version": version, "hud": hud})))
    });
    upgrade
        .await
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?
}

/// Step two: restart the HUD login item when the upgrade changed it, then start `swarm ui` of the
/// installed version on this port. That finds this server of an older build, stops it, and starts
/// its own, and the open pages reload on the new build.
pub(super) async fn update_restart(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::os::unix::process::CommandExt;
    let prefix = brew_prefix().ok_or_else(|| {
        error(
            StatusCode::CONFLICT,
            "this swarm is not a Homebrew install; run `swarm ui` again",
        )
    })?;
    let fail = |err: std::io::Error| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
    let hud = HUD_CHANGED.load(Ordering::SeqCst);
    if hud {
        // A login item that is not running starts the new app through `opt` when it next starts.
        let restarted = tokio::task::spawn_blocking(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            if !swarm::hud::status(Path::new(&home)).running {
                return Ok(());
            }
            let output = swarm::hud::restart()?;
            if output.status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(
                    String::from_utf8_lossy(&output.stderr).trim().to_string(),
                ))
            }
        })
        .await
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
        // One try only: the server is not touched yet, and the next press restarts it alone.
        HUD_CHANGED.store(false, Ordering::SeqCst);
        restarted.map_err(|err| {
            let message = format!(
                "the HUD did not restart: {err}; press Restart server again to restart only the server"
            );
            error(StatusCode::BAD_GATEWAY, &message)
        })?;
    }
    // The server writes its URL with the token there, so only the owner may read the log.
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(app.root.join("ui.log"))
        .map_err(fail)?;
    // `mode` applies only to a new file; an older log may be wider, as `spawn_ui` also fixes.
    log.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(fail)?;
    std::process::Command::new(prefix.join("opt/swarm/bin/swarm"))
        .args(["ui", "--port", &app.port.to_string()])
        .env("SWARM_HOME", app.root.parent().expect("swarm home"))
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(fail)?)
        .stderr(log)
        // Its own process group, so it outlives this server, which it stops.
        .process_group(0)
        .spawn()
        .map_err(fail)?;
    Ok(Json(json!({"ok": true, "hud": hud})))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn finds_the_homebrew_prefix() {
        let exe = Path::new("/opt/homebrew/Cellar/swarm/0.0.37/bin/swarm");
        assert_eq!(cellar_prefix(exe), Some(PathBuf::from("/opt/homebrew")));
        assert_eq!(cellar_prefix(Path::new("/Users/me/.cargo/bin/swarm")), None);
    }
}
