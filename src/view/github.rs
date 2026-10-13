//! PR identity checks from the Swift GitHubInspection boundary.
use super::discovery::run;
use serde_json::Value;
use std::{collections::BTreeSet, time::Instant};

pub fn repository(remote: &str) -> Option<String> {
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;
    if path.contains(['?', '#']) {
        return None;
    }
    name(path.strip_suffix(".git").unwrap_or(path))
}
fn name(path: &str) -> Option<String> {
    let parts: Vec<_> = path.split('/').collect();
    (parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
        }))
    .then(|| path.to_ascii_lowercase())
}
pub fn lookup(path: &str) -> Option<Value> {
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    let command = |program, args: &[&str]| {
        let remaining = deadline.saturating_duration_since(Instant::now()).as_secs();
        if remaining == 0 {
            None
        } else {
            run(program, args, remaining)
        }
    };
    let git = |args: &[&str]| {
        let mut flags = vec!["-C", path];
        flags.extend(args);
        String::from_utf8(command("git", &flags)?)
            .ok()
            .map(|s| s.trim().to_owned())
    };
    let branch = git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let head = git(&["rev-parse", "HEAD"])?;
    let remotes: Vec<_> = git(&["remote"])?.lines().map(str::to_owned).collect();
    let push = [
        format!("branch.{branch}.pushRemote"),
        "remote.pushDefault".into(),
        format!("branch.{branch}.remote"),
    ]
    .iter()
    .find_map(|key| git(&["config", "--get", key]).filter(|v| v != "." && !v.is_empty()))
    .or_else(|| {
        if remotes.iter().any(|r| r == "origin") {
            Some("origin".into())
        } else if remotes.len() == 1 {
            Some(remotes[0].clone())
        } else {
            None
        }
    })?;
    if !remotes.contains(&push) {
        return None;
    }
    let urls = git(&["remote", "get-url", "--push", "--all", "--", &push])?;
    let head_repositories: Option<BTreeSet<_>> = urls.lines().map(repository).collect();
    let head_repositories = head_repositories?;
    if head_repositories.len() != 1 {
        return None;
    }
    let head_repository = head_repositories.first()?.clone();
    let mut targets = BTreeSet::from([head_repository.clone()]);
    for remote in remotes {
        if let Some(repo) =
            git(&["remote", "get-url", "--", &remote]).and_then(|url| repository(&url))
        {
            targets.insert(repo);
        }
    }
    let gh =
        |args: &[&str]| command("gh", args).and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let info = gh(&[
        "repo",
        "view",
        &format!("github.com/{head_repository}"),
        "--json",
        "nameWithOwner,parent",
    ])?;
    if info["nameWithOwner"].as_str().and_then(name).as_deref() != Some(&head_repository) {
        return None;
    }
    if !info["parent"].is_null() {
        targets.insert(name(&format!(
            "{}/{}",
            info["parent"]["owner"]["login"].as_str()?,
            info["parent"]["name"].as_str()?
        ))?);
    }
    let mut matches = Vec::new();
    for target in targets {
        let requests = gh(&[
            "pr",
            "list",
            "--repo",
            &format!("github.com/{target}"),
            "--head",
            &branch,
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,state,isDraft,baseRefName,baseRefOid,headRefName,headRefOid,headRepository,reviewDecision,statusCheckRollup,url",
        ])?;
        let requests = requests.as_array()?;
        if requests.len() >= 100 {
            return None;
        }
        for request in requests.iter().filter(|r| r["headRefName"] == branch) {
            let repo = request["headRepository"]["nameWithOwner"]
                .as_str()
                .and_then(name)?;
            if repo != head_repository {
                continue;
            }
            if request["number"].as_u64()? == 0
                || request["baseRefOid"].as_str()?.is_empty()
                || request["headRefOid"].as_str()?.is_empty()
            {
                return None;
            }
            let url = request["url"].as_str()?;
            if url
                != format!(
                    "https://github.com/{target}/pull/{}",
                    request["number"].as_u64()?
                )
            {
                return None;
            }
            matches.push(request.clone());
        }
    }
    if matches.len() > 1
        || git(&["symbolic-ref", "--quiet", "--short", "HEAD"])? != branch
        || git(&["rev-parse", "HEAD"])? != head
    {
        return None;
    }
    Some(matches.pop().unwrap_or(Value::Null))
}
