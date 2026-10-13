use super::{DiffPreview, TextPreview};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Waiting,
    Finished,
    Failed,
    Interrupted,
    Unreported,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolActivity {
    pub name: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<TextPreview>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diffs: Vec<DiffPreview>,
    pub state: ToolState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    pub header_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_body: Option<TextPreview>,
}

pub fn command(input: &Value, name: &str) -> Option<String> {
    for key in ["command", "cmd", "CommandLine"] {
        if let Some(text) = input[key].as_str() {
            return Some(text.into());
        }
    }
    let name = name.to_lowercase();
    if name.contains("exec") || name.contains("shell") {
        input
            .as_str()
            .map(|text| exec_commands(text).unwrap_or_else(|| text.into()))
    } else {
        None
    }
}
pub fn path(input: &Value) -> Option<String> {
    ["file_path", "TargetFile", "AbsolutePath"]
        .into_iter()
        .find_map(|key| input[key].as_str().map(str::to_owned))
}
pub fn title(input: &Value, command: Option<&str>, path: Option<&str>) -> String {
    for key in ["description", "Description", "toolSummary", "skill"] {
        if let Some(value) = input[key]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return value.into();
        }
    }
    if let Some(command) = command {
        let prefix = command.trim_start();
        let end = super::preview::prefix_end(prefix, 200);
        let line = prefix[..end].lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            return line.into();
        }
    }
    if let Some(pattern) = input["pattern"].as_str() {
        return match input["path"].as_str() {
            Some(folder) => format!("\"{pattern}\" in {}", basename(folder)),
            None => format!("\"{pattern}\""),
        };
    }
    let Some(path) = path else {
        return String::new();
    };
    let name = basename(path);
    let offset = input["offset"].as_i64();
    let limit = input["limit"].as_i64();
    if offset.is_none() && limit.is_none() {
        return name.into();
    }
    let first = offset.unwrap_or(1);
    if let Some(limit) = limit.filter(|value| *value > 0)
        && let Some(last) = first.checked_add(limit - 1)
    {
        return format!("{name} · lines {first}–{last}");
    }
    format!("{name} · from line {first}")
}
pub(crate) fn basename(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

fn literal_end(text: &str) -> usize {
    let quote = text.chars().next().unwrap_or('\0');
    let mut escaped = false;
    for (index, character) in text.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return index + character.len_utf8();
        } else if matches!(character, '\r' | '\n') && quote != '`' {
            return index;
        }
    }
    text.len()
}
fn cmd_literal(arguments: &str) -> Option<(String, usize)> {
    let object = arguments.trim_start().strip_prefix('{')?.trim_start();
    let value = object
        .strip_prefix("cmd:")
        .or_else(|| object.strip_prefix("\"cmd\":"))?
        .trim_start();
    if !value.starts_with('"') {
        return None;
    }
    let end = literal_end(value);
    if !value[..end].ends_with('"') || end == 1 {
        return None;
    }
    if !value[end..].trim_start().starts_with([',', '}']) {
        return None;
    }
    let decoded = serde_json::from_str(&value[..end]).unwrap_or_else(|_| value[1..end - 1].into());
    Some((decoded, arguments.len() - value.len() + end))
}
pub fn exec_commands(script: &str) -> Option<String> {
    let mut rest = script;
    let mut steps = Vec::new();
    let mut has_command = false;
    while let Some(first) = rest.chars().next() {
        if matches!(first, '\'' | '"' | '`') {
            rest = &rest[literal_end(rest)..];
            continue;
        }
        let Some(after) = rest.strip_prefix("tools.") else {
            rest = &rest[first.len_utf8()..];
            continue;
        };
        rest = after;
        let name_end = rest
            .find(|character: char| !(character.is_alphanumeric() || character == '_'))
            .unwrap_or(rest.len());
        if name_end == 0 || !rest[name_end..].starts_with('(') {
            continue;
        }
        let name = &rest[..name_end];
        rest = &rest[name_end + 1..];
        if name == "exec_command"
            && let Some((text, end)) = cmd_literal(rest)
        {
            steps.push(text);
            has_command = true;
            rest = &rest[end..];
        } else {
            steps.push(format!("{name}(…)"));
        }
    }
    has_command.then(|| steps.join("\n"))
}

fn code_mode_body(output: &str) -> Option<&str> {
    if ![
        "Script completed\n",
        "Script failed\n",
        "Script running with cell ID ",
    ]
    .iter()
    .any(|header| output.starts_with(header))
    {
        return None;
    }
    output.split_once("\nOutput:\n").map(|(_, body)| body)
}
fn code_mode_results(output: &str) -> Option<Vec<Value>> {
    code_mode_body(output)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()
        .ok()
}
pub fn exit_code(output: Option<&str>, command: Option<&str>, is_codex: bool) -> Option<i64> {
    command?;
    let output = output?;
    if let Some(code) = output.strip_prefix("Exit code ") {
        return code.lines().next()?.parse().ok();
    }
    if !is_codex {
        return None;
    }
    if output.starts_with("Chunk ID: ") {
        let header = output.split("\nOutput:\n").next()?;
        return header
            .split_once("\nProcess exited with code ")?
            .1
            .lines()
            .next()?
            .parse()
            .ok()
            .filter(|code| *code != 0);
    }
    for result in code_mode_results(output)? {
        let records = match result {
            Value::Array(records) => records,
            record => vec![record],
        };
        for record in records {
            let record = record.get("value").unwrap_or(&record);
            if let Some(code) = record["exit_code"].as_i64().filter(|code| *code != 0) {
                return Some(code);
            }
        }
    }
    None
}
pub fn reports_failure(output: Option<&str>, command: Option<&str>, is_codex: bool) -> bool {
    if exit_code(output, command, is_codex).is_some_and(|code| code != 0) {
        return true;
    }
    let Some(output) = output.filter(|_| command.is_some()) else {
        return false;
    };
    if code_mode_body(output).is_none() {
        return false;
    }
    if output.starts_with("Script failed\n") {
        return true;
    }
    code_mode_results(output).is_some_and(|results| {
        results.iter().any(|result| {
            result
                .as_array()
                .is_some_and(|entries| entries.iter().any(|entry| entry["status"] == "rejected"))
        })
    })
}

pub fn duration_label(seconds: f64) -> String {
    if seconds < 10.0 {
        return format!("{seconds:.1}s");
    }
    let whole = seconds.round() as i64;
    if seconds < 60.0 {
        return format!("{whole}s");
    }
    format!("{}m {}s", whole / 60, whole % 60)
}
fn timestamp(text: &str) -> Option<f64> {
    let date = text.get(..10)?;
    let mut components = date.split('-').map(str::parse::<i64>);
    let mut year = components.next()?.ok()?;
    let month = components.next()?.ok()?;
    let day = components.next()?.ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * shifted_month + 2) / 5 + day - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let time = text.get(11..)?;
    let zone_start = time.find(['Z', '+', '-'])?;
    let mut clock = time[..zone_start].split(':').map(str::parse::<f64>);
    let hour = clock.next()?.ok()?;
    let minute = clock.next()?.ok()?;
    let second = clock.next()?.ok()?;
    let zone = &time[zone_start..];
    let offset = if zone == "Z" {
        0.0
    } else {
        let mut pieces = zone[1..].split(':').map(str::parse::<f64>);
        (pieces.next()?.ok()? * 3600.0 + pieces.next()?.ok()? * 60.0)
            * if zone.starts_with('-') { -1.0 } else { 1.0 }
    };
    Some(days as f64 * 86400.0 + hour * 3600.0 + minute * 60.0 + second - offset)
}
pub fn duration(start: &str, end: &str) -> Option<f64> {
    let seconds = timestamp(end)? - timestamp(start)?;
    (seconds >= 0.0).then_some(seconds)
}
