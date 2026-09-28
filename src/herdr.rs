//! The Herdr adapter's `spawn` verb: split one new child pane straight into the main-grid shape.
//! The orchestrator keeps the left half and the children stack in the right column, so the layout
//! never needs a rebuild. A rebuild has to park panes in a scratch tab, because Herdr refuses a
//! pane move inside one tab, and that tab blinks in the tab bar.

use serde_json::Value;

const FORWARDED: [&str; 4] = [
    "SWARM_SESSION_ID",
    "SWARM_AGENT_ID",
    "SWARM_HOME",
    "SWARM_ADAPTER",
];

fn herdr(args: &[&str]) -> Result<Value, String> {
    let output = std::process::Command::new("herdr")
        .args(args)
        .output()
        .map_err(|error| format!("herdr: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err([stderr, stdout]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or(format!("herdr {} failed", args[0])));
    }
    match stdout.is_empty() {
        true => Ok(Value::Null),
        false => {
            serde_json::from_str(&stdout).map_err(|error| format!("herdr {}: {error}", args[0]))
        }
    }
}

/// Panes right of the orchestrator, top to bottom, as (pane id, height).
fn children(orchestrator: &str) -> Result<Vec<(String, f64)>, String> {
    let layout = herdr(&["pane", "layout", "--pane", orchestrator])?;
    let panes = layout["result"]["layout"]["panes"]
        .as_array()
        .ok_or("herdr pane layout: no panes")?;
    let x = |pane: &Value| pane["rect"]["x"].as_f64().unwrap_or_default();
    let left = panes
        .iter()
        .find(|pane| pane["pane_id"] == orchestrator)
        .map(x)
        .ok_or("orchestrator pane is not in its layout")?;
    let mut children: Vec<&Value> = panes.iter().filter(|pane| x(pane) > left).collect();
    children.sort_by(|a, b| {
        a["rect"]["y"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&b["rect"]["y"].as_f64().unwrap_or_default())
    });
    Ok(children
        .into_iter()
        .map(|pane| {
            (
                pane["pane_id"].as_str().unwrap_or_default().to_string(),
                pane["rect"]["height"].as_f64().unwrap_or_default(),
            )
        })
        .collect())
}

/// One (split index, ratio delta) per split, top to bottom, so every child ends the same height.
/// A split's ratio is its top pane's share of that split's own rect, which is scale free, so
/// moving one split leaves the ratios below it alone and every delta comes from one layout read.
pub fn equalise_steps(heights: &[f64]) -> Vec<(usize, f64)> {
    (0..heights.len().saturating_sub(1))
        .map(|index| {
            (
                index,
                1.0 / (heights.len() - index) as f64
                    - heights[index] / heights[index..].iter().sum::<f64>(),
            )
        })
        .collect()
}

/// `--token` options for the child pane. Herdr cuts a value at 80 characters and clears a key
/// whose value is empty, so an empty value is left out instead of published blank.
pub fn tag_options(tree: &str, seat: &str) -> Vec<String> {
    [("tree", tree), ("seat", seat)]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .flat_map(|(name, value)| {
            [
                "--token".to_string(),
                format!("{name}={}", value.chars().take(80).collect::<String>()),
            ]
        })
        .collect()
}

/// Splits the child pane, tags it for the sidebar, evens the column, and returns the pane id.
/// `env` reads one variable; an unset one reads as empty.
pub fn split(
    orchestrator: &str,
    cwd: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let before = children(orchestrator)?;
    let (target, direction) = match before.last() {
        Some((pane, _)) => (pane.as_str(), "down"),
        None => (orchestrator, "right"),
    };
    let forwarded: Vec<String> = FORWARDED
        .iter()
        .map(|name| format!("{name}={}", env(name).unwrap_or_default()))
        .collect();
    let mut args = vec![
        "pane",
        "split",
        target,
        "--cwd",
        cwd,
        "--direction",
        direction,
        "--ratio",
        "0.5",
        "--no-focus",
    ];
    for value in &forwarded {
        args.extend(["--env", value.as_str()]);
    }
    let split = herdr(&args)?;
    let pane = split["result"]["pane"]["pane_id"]
        .as_str()
        .ok_or("herdr pane split: no pane id")?
        .to_string();
    // The sidebar rows render `$tree` and `$seat`; only the spawner knows which panes are children
    // and which seat each holds. `SWARM_PANE_TREE` picks another marker, and empty turns it off.
    let options = tag_options(
        &env("SWARM_PANE_TREE").unwrap_or("↳".into()),
        &env("SWARM_AGENT_ID").unwrap_or_default(),
    );
    if !options.is_empty() {
        // Best effort: a missing token only costs the sidebar its label. The pane id goes first.
        let mut args = vec![
            "pane",
            "report-metadata",
            pane.as_str(),
            "--source",
            "swarm-split",
        ];
        args.extend(options.iter().map(String::as_str));
        if let Err(error) = herdr(&args) {
            eprintln!("swarm herdr-split: pane tags skipped: {error}");
        }
    }
    let after = children(orchestrator)?;
    let heights: Vec<f64> = after.iter().map(|(_, height)| *height).collect();
    for (index, delta) in equalise_steps(&heights) {
        if delta.abs() < 0.01 {
            continue;
        }
        // Each direction moves the border on that side of the named pane, and that pane grows.
        let (pane, direction) = if delta > 0.0 {
            (&after[index].0, "down")
        } else {
            (&after[index + 1].0, "up")
        };
        herdr(&[
            "pane",
            "resize",
            "--pane",
            pane,
            "--direction",
            direction,
            "--amount",
            &format!("{:.4}", delta.abs()),
        ])?;
    }
    Ok(pane)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(steps: Vec<(usize, f64)>, expected: &[f64]) {
        assert_eq!(steps.len(), expected.len(), "{steps:?}");
        for ((_, delta), want) in steps.iter().zip(expected) {
            assert!((delta - want).abs() < 1e-9, "{steps:?}");
        }
    }

    #[test]
    fn equalise_steps_even_out_the_column() {
        close(equalise_steps(&[40.0]), &[]);
        close(equalise_steps(&[40.0, 40.0]), &[0.0]);
        close(equalise_steps(&[40.0, 20.0, 20.0]), &[1.0 / 3.0 - 0.5, 0.0]);
        close(
            equalise_steps(&[20.0, 30.0, 30.0]),
            &[1.0 / 3.0 - 0.25, 0.0],
        );
    }

    #[test]
    fn tag_options_cut_at_80_characters_and_drop_empty_values() {
        assert_eq!(
            tag_options("↳", "ws-claude-run1"),
            ["--token", "tree=↳", "--token", "seat=ws-claude-run1"]
        );
        assert_eq!(
            tag_options("↳", &"s".repeat(100)).last().unwrap(),
            &format!("seat={}", "s".repeat(80))
        );
        assert_eq!(tag_options("", ""), Vec::<String>::new());
    }
}
