//! The Pi packages installed by setup and loaded by launch.

use std::path::{Path, PathBuf};

use super::installed;

const PINS: [(&str, &str, &[&str], &str, bool); 2] = [
    (
        "@tintinweb/pi-subagents",
        "0.26.1",
        &["src/index.ts"],
        "skills",
        // Its in-process SDK sessions do not inherit Swarm's launch guard.
        false,
    ),
    (
        "@aliou/pi-processes",
        "0.13.6",
        &[
            "extensions/processes/index.ts",
            "extensions/processes-logs/index.ts",
            "extensions/processes-dock/index.ts",
        ],
        "skills/pi-processes",
        true,
    ),
];

/// An upgrade leaves the old files available to panes that already loaded them.
pub(crate) fn prefix(root: &Path) -> PathBuf {
    root.join("pi/packages")
        .join(format!("{}-{}", PINS[0].1, PINS[1].1))
}

fn resources(prefix: &Path) -> Option<Vec<String>> {
    let mut args = Vec::new();
    for (name, version, extensions, skills, enabled) in PINS {
        let package = prefix.join("node_modules").join(name);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(package.join("package.json")).ok()?).ok()?;
        if manifest["name"] != name || manifest["version"] != version {
            return None;
        }
        for entry in extensions {
            let path = package.join(entry);
            if !path.is_file() {
                return None;
            }
            if enabled {
                args.extend(["--extension".into(), path.display().to_string()]);
            }
        }
        let path = package.join(skills);
        if !path.is_dir() {
            return None;
        }
        if enabled {
            args.extend(["--skill".into(), path.display().to_string()]);
        }
    }
    Some(args)
}

pub(crate) fn launch_args(prefix: &Path) -> Option<Vec<String>> {
    // While npm assembles node_modules, some (but not all) files already exist.
    prefix.join(".ready").is_file().then(|| resources(prefix))?
}

pub(crate) fn setup_state(prefix: &Path) -> (&'static str, String) {
    if launch_args(prefix).is_some() {
        ("ok", "The pinned Pi packages are installed".into())
    } else if !installed("npm") {
        (
            "broken",
            "Install npm to set up the pinned Pi packages".into(),
        )
    } else {
        ("setup", "The pinned Pi packages are not installed".into())
    }
}

fn install_command(prefix: &Path) -> Vec<String> {
    let mut args = vec![
        "npm".into(),
        "install".into(),
        "--prefix".into(),
        prefix.display().to_string(),
        "--global=false".into(),
        "--no-save".into(),
        "--no-package-lock".into(),
        "--legacy-peer-deps".into(),
        "--ignore-scripts".into(),
        "--no-audit".into(),
        "--no-fund".into(),
    ];
    args.extend(PINS.map(|(name, version, _, _, _)| format!("{name}@{version}")));
    args
}

pub(crate) fn plan_line(prefix: &Path) -> String {
    format!(
        "run  {}",
        swarm::adapter::shell_line(&install_command(prefix))
    )
}

pub(crate) fn install(prefix: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(prefix)?;
    swarm::managed::with_lock(&prefix.join(".install.lock"), || {
        if launch_args(prefix).is_some() {
            return Ok(());
        }
        let ready = prefix.join(".ready");
        if let Err(error) = std::fs::remove_file(&ready)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error.to_string());
        }
        let argv = install_command(prefix);
        let status = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(prefix)
            .status()
            .map_err(|error| format!("swarm: cannot run npm: {error}"))?;
        if !status.success() || resources(prefix).is_none() {
            return Err(format!(
                "swarm: npm did not install both pinned Pi packages ({status})"
            ));
        }
        std::fs::File::create(ready)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })?;
    Ok(())
}
