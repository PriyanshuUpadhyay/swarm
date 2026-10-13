//! GitHub reads use the same repository and PR identity boundary as sidebar fields.
use super::{discovery, github, lifecycle::Error};
use serde_json::{Value, json};
use std::{
    process::Command,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Error>;
fn run(root: &str, program: &str, args: &[&str], deadline: Instant) -> Result<Vec<u8>> {
    let seconds = deadline.saturating_duration_since(Instant::now()).as_secs();
    if seconds == 0 {
        return Err(Error::Failed("The GitHub read timed out".into()));
    }
    let mut command = Command::new(program);
    command
        .current_dir(root)
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    let out = discovery::run_command(&mut command, seconds).map_err(Error::Failed)?;
    if !out.status.success() {
        return Err(Error::Failed(
            String::from_utf8_lossy(&out.stderr).trim().into(),
        ));
    }
    Ok(out.stdout)
}
fn gh(root: &str, args: &[&str], deadline: Instant) -> Result<Value> {
    serde_json::from_slice(&run(root, "gh", args, deadline)?)
        .map_err(|_| Error::Failed("GitHub returned invalid JSON".into()))
}
fn git(root: &str, args: &[&str], deadline: Instant) -> Result<String> {
    String::from_utf8(run(root, "git", args, deadline)?)
        .map(|s| s.trim().to_owned())
        .map_err(|_| Error::Failed("Git returned invalid UTF-8".into()))
}
fn repository(root: &str, deadline: Instant) -> Result<String> {
    let url = git(root, &["remote", "get-url", "--", "origin"], deadline)?;
    github::repository(&url)
        .ok_or_else(|| Error::Invalid("This project has no GitHub origin".into()))
}
pub fn issues(root: &str) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let repo = repository(root, deadline)?;
    let issues = gh(
        root,
        &[
            "issue",
            "list",
            "--repo",
            &repo,
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,body,url,state,labels",
        ],
        deadline,
    )?;
    if !issues.is_array() {
        return Err(Error::Failed(
            "GitHub returned an invalid issue list".into(),
        ));
    }
    Ok(json!({"issues":issues}))
}
pub fn issue(root: &str, number: u64) -> Result<Value> {
    if number == 0 || number > i64::MAX as u64 {
        return Err(Error::Invalid("Enter a positive issue number".into()));
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let repo = repository(root, deadline)?;
    let issue = gh(
        root,
        &[
            "issue",
            "view",
            &number.to_string(),
            "--repo",
            &repo,
            "--json",
            "number,title,body,url,state",
        ],
        deadline,
    )?;
    if issue["number"].as_u64() != Some(number)
        || issue["title"].as_str().is_none_or(|s| s.trim().is_empty())
        || !issue["body"].is_string()
    {
        return Err(Error::Failed(
            "GitHub returned a different or incomplete issue".into(),
        ));
    }
    Ok(issue)
}
pub fn pr(root: &str) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let before = github::lookup_until(root, deadline)
        .ok_or_else(|| Error::Failed("GitHub could not identify one PR for this branch".into()))?;
    if before.is_null() {
        return Ok(json!({"pullRequest":null,"checks":[]}));
    }
    let repo = before["url"]
        .as_str()
        .and_then(|s| s.strip_prefix("https://github.com/"))
        .and_then(|s| s.split_once("/pull/"))
        .map(|(repo, _)| repo)
        .ok_or_else(|| Error::Failed("GitHub returned an invalid PR URL".into()))?;
    let request = gh(
        root,
        &[
            "pr",
            "view",
            &before["number"].to_string(),
            "--repo",
            repo,
            "--json",
            "number,title,body,state,isDraft,baseRefName,baseRefOid,headRefName,headRefOid,headRepository,reviewDecision,statusCheckRollup,url",
        ],
        deadline,
    )?;
    for field in [
        "number",
        "url",
        "baseRefOid",
        "headRefOid",
        "headRefName",
        "headRepository",
    ] {
        if before[field] != request[field] {
            return Err(Error::Failed(
                "The PR changed during the read. Refresh it".into(),
            ));
        }
    }
    let local_head = git(root, &["rev-parse", "HEAD"], deadline)?;
    let branch = git(
        root,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        deadline,
    )?;
    if request["headRefName"] != branch {
        return Err(Error::Failed(
            "The branch changed during the read. Refresh it".into(),
        ));
    }
    Ok(
        json!({"checks":request["statusCheckRollup"],"localHead":local_head,
        "differsFromLocalHead": request["headRefOid"] != local_head,"pullRequest":request}),
    )
}
