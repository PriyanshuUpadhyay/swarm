//! Claude permission decisions. Screen choices and hook suggestions are different lists.

use serde_json::{Value, json};

mod files;
pub use files::*;

pub const HOOK_TIMEOUT_SECONDS: u64 = 60;
pub const WAIT_SECONDS: u64 = 50;

/// A tool permission request, as opposed to a setup screen or AskUserQuestion.
pub fn is_request(payload: &Value) -> bool {
    payload["hook_event_name"] == "PermissionRequest"
        && payload["tool_name"].as_str().is_some_and(|name| {
            !name.is_empty() && !matches!(name, "AskUserQuestion" | "ExitPlanMode")
        })
        && payload["tool_input"].is_object()
}

fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Match the actual tool input on a permission screen. A question, folder trust screen, or
/// another tool's permission must not receive this request's answer.
pub fn matches_prompt(payload: &Value, prompt: &crate::screen::Prompt) -> bool {
    if !is_request(payload)
        || prompt.choices.first().map(String::as_str) != Some("Yes")
        || !prompt.choices.iter().any(|choice| choice == "No")
    {
        return false;
    }
    let question = &prompt.question;
    let input = &payload["tool_input"];
    if payload["tool_name"] == "Bash" {
        return question.contains("Bash command")
            && input["command"]
                .as_str()
                .is_some_and(|command| !command.is_empty() && question.contains(command));
    }
    if let Some(path) = input["file_path"].as_str().filter(|path| !path.is_empty()) {
        let relative = payload["cwd"]
            .as_str()
            .and_then(|cwd| std::path::Path::new(path).strip_prefix(cwd).ok())
            .and_then(std::path::Path::to_str);
        return [Some(path), relative]
            .into_iter()
            .flatten()
            .filter(|path| !path.is_empty())
            .any(|path| question.contains(path));
    }
    // MCP and other tool dialogs show the tool's name. They are usable only when one waiting
    // request matches; the store refuses an ambiguous pair.
    payload["tool_name"]
        .as_str()
        .is_some_and(|name| question.contains(name))
}

/// A supported screen choice becomes a hook decision. An unknown or ambiguous form uses keys.
pub fn decision(payload: &Value, choice: &str) -> Option<Value> {
    let choice = folded(choice);
    match choice.as_str() {
        "Yes" => return Some(json!({"behavior": "allow"})),
        "No" => {
            return Some(json!({"behavior": "deny", "message": "The owner denied this request."}));
        }
        _ => {}
    }
    let suggestions = payload["permission_suggestions"].as_array()?;
    let matching: Vec<_> = suggestions
        .iter()
        .filter(|suggestion| match suggestion["type"].as_str() {
            Some("addDirectories") => {
                let Some(directories) = suggestion["directories"].as_array() else {
                    return false;
                };
                let [directory] = directories.as_slice() else {
                    return false;
                };
                directory.as_str().is_some_and(|directory| {
                    choice
                        == format!("Yes, and always allow access to {directory} from this project")
                })
            }
            Some("setMode") => {
                suggestion["mode"] == "acceptEdits"
                    && matches!(
                        choice.as_str(),
                        "Yes, allow all edits during this session"
                            | "Yes, and allow all edits during this session"
                    )
            }
            Some("addRules") => {
                if suggestion["behavior"] != "allow" {
                    return false;
                }
                if choice == "Yes, and don't ask again" {
                    return true;
                }
                let Some(rule) = choice
                    .strip_prefix("Yes, and don't ask again for:")
                    .or_else(|| choice.strip_prefix("Yes, and don't ask again for "))
                    .map(str::trim)
                else {
                    return false;
                };
                let Some(rules) = suggestion["rules"].as_array() else {
                    return false;
                };
                let [entry] = rules.as_slice() else {
                    return false;
                };
                entry["toolName"] == payload["tool_name"]
                    && entry["ruleContent"].as_str() == Some(rule)
            }
            _ => false,
        })
        .collect();
    let [suggestion] = matching.as_slice() else {
        return None;
    };
    Some(json!({"behavior": "allow", "updatedPermissions": [suggestion]}))
}

pub fn output(decision: Value) -> Value {
    json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": decision}})
}

#[cfg(test)]
mod tests;
