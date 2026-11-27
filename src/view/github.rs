//! PR identity checks from the Swift GitHubInspection boundary.
use super::discovery::{DeadlineExpired, run_before};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    time::{Duration, Instant},
};

pub(super) const LOOKUP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum LookupError {
    Deadline(DeadlineExpired),
    CommandFailed,
}

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
    lookup_until(path, Instant::now() + LOOKUP_TIMEOUT)
}
pub fn lookup_until(path: &str, deadline: Instant) -> Option<Value> {
    lookup_bounded(path, deadline, false, None).ok().flatten()
}

pub(super) fn lookup_for_chip(
    path: &str,
    branch: Option<&str>,
    deadline: Instant,
) -> Result<Option<Value>, LookupError> {
    lookup_bounded(path, deadline, true, branch)
}

fn lookup_bounded(
    path: &str,
    deadline: Instant,
    include_merged: bool,
    expected_branch: Option<&str>,
) -> Result<Option<Value>, LookupError> {
    let failure = RefCell::new(None);
    let command = |program: &str, args: &[&str]| {
        if failure.borrow().is_some() {
            return None;
        }

        let output = run_before(program, args, deadline);
        match output {
            Err(cut) => *failure.borrow_mut() = Some(LookupError::Deadline(cut)),
            Ok(None) if program == "gh" => *failure.borrow_mut() = Some(LookupError::CommandFailed),
            Ok(output) => return output,
        }
        None
    };

    let response = lookup_matching(path, include_merged, expected_branch, command);
    failure.into_inner().map_or(Ok(response), Err)
}

fn lookup_matching(
    path: &str,
    include_merged: bool,
    expected_branch: Option<&str>,
    command: impl Fn(&str, &[&str]) -> Option<Vec<u8>>,
) -> Option<Value> {
    let git = |args: &[&str]| {
        let mut flags = vec!["-C", path];
        flags.extend(args);
        String::from_utf8(command("git", &flags)?)
            .ok()
            .map(|s| s.trim().to_owned())
    };

    let branch = git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if expected_branch.is_some_and(|expected| expected != branch) {
        return None;
    }
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

    let matching = |state: &str| -> Option<Vec<Value>> {
        let mut matches = Vec::new();
        for target in &targets {
            let requests = gh(&[
                "pr",
                "list",
                "--repo",
                &format!("github.com/{target}"),
                "--head",
                &branch,
                "--state",
                state,
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
                let number = request["number"].as_u64()?;
                if number == 0
                    || (include_merged && request["state"] != state.to_ascii_uppercase())
                    || request["baseRefOid"].as_str()?.is_empty()
                    || request["headRefOid"].as_str()?.is_empty()
                {
                    return None;
                }
                let url_repo = request["url"]
                    .as_str()?
                    .strip_prefix("https://github.com/")?
                    .strip_suffix(&format!("/pull/{number}"))?;
                if name(url_repo).as_ref() != Some(target) {
                    return None;
                }
                matches.push(request.clone());
            }
        }
        Some(matches)
    };

    let mut matches = matching("open")?;
    if matches.is_empty() && include_merged {
        matches = matching("merged")?;
        if matches.len() == 1 && matches[0]["headRefOid"] != head {
            matches.clear();
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
