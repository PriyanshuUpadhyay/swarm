// Workspace defaults, options, and creation.
use super::*;
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewWorkspace {
    pub project: String,
    #[serde(default)]
    pub name: String,
    pub folder: Option<String>,
    pub prefix: Option<String>,
    pub mode: Mode,
    pub base: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<u64>,
    pub issue: Option<u64>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    NewBranch,
    ExistingBranch,
    Pr,
    Issue,
}
pub fn defaults(project: &Project, owner: &OwnerChoices) -> (String, String, String) {
    let seed = if common(project)
        .is_some_and(|c| Path::new(c).file_name().is_some_and(|n| n == ".bare"))
    {
        "wt/".into()
    } else {
        format!("../{}-worktrees", project.name)
    };
    let settings = owner
        .project_defaults
        .get(&project.path)
        .unwrap_or(&Value::Null);
    let folder = settings["worktreeFolder"]
        .as_str()
        .unwrap_or(&seed)
        .trim()
        .to_owned();
    let prefix = settings["branchPrefix"]
        .as_str()
        .unwrap_or("swarm/")
        .trim()
        .to_owned();
    (folder, prefix, seed)
}
pub fn options(project: &Project, owner: &OwnerChoices) -> Result<Value> {
    let common = common(project)
        .ok_or_else(|| Error::Invalid("New Workspace needs a Git project".into()))?;
    let mut value =
        serde_json::to_value(references(common)?).map_err(|e| Error::Failed(e.to_string()))?;
    let (folder, prefix, seed) = defaults(project, owner);
    value["folder"] = folder.into();
    value["prefix"] = prefix.into();
    value["seedFolder"] = seed.into();
    value["seedPrefix"] = "swarm/".into();
    value["setup_script"] = owner
        .project_defaults
        .get(&project.path)
        .and_then(|s| s.get("setup_script"))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(value)
}
pub fn create_workspace(
    root: &Path,
    project: &Project,
    owner: &OwnerChoices,
    request: &NewWorkspace,
) -> Result<(String, Value)> {
    let common = common(project)
        .ok_or_else(|| Error::Invalid("New Workspace needs a Git project".into()))?;
    checked_path(root, &project.path, true)?;
    checked_path(root, common, true)?;
    let (folder, prefix, seed) = defaults(project, owner);
    let prefix = request.prefix.as_deref().unwrap_or(&prefix).trim();
    let branch = branch_name(&request.name, prefix).ok_or_else(|| {
        Error::Invalid("Enter a name and prefix that make a valid Git branch".into())
    })?;
    let folder_setting = request.folder.as_deref().unwrap_or(&folder).trim();
    if folder_setting.is_empty() {
        return Err(Error::Invalid("Enter a worktree folder".into()));
    }
    let folder = if let Some(suffix) = folder_setting.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").ok_or(Error::Forbidden)?).join(suffix)
    } else {
        Path::new(&project.path).join(folder_setting)
    };
    let parent = checked_path(root, &folder.to_string_lossy(), false)?;
    let target = parent.join(slug(&request.name));
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(Error::Invalid(format!(
            "A folder already exists at {}",
            target.display()
        )));
    }
    let path = target.to_str().ok_or(Error::Forbidden)?;
    let refs = references(common)?;
    let mut args = vec!["worktree", "add"];
    let expected;
    match request.mode {
        Mode::NewBranch | Mode::Issue => {
            let base = request
                .base
                .as_deref()
                .unwrap_or_else(|| refs.bases.first().map(String::as_str).unwrap_or(""));
            if refs.default_branch.is_none() {
                if !base.is_empty() {
                    return Err(Error::Invalid(
                        "The first workspace starts without a base".into(),
                    ));
                }
                args.extend(["--orphan", "-b", &branch, "--", path]);
            } else {
                if !refs.bases.iter().any(|b| b == base) {
                    return Err(Error::Invalid("Select a known base branch".into()));
                }
                args.extend(["-b", &branch, "--", path, base]);
            }
            expected = branch.clone();
        }
        Mode::ExistingBranch => {
            let existing = request.branch.as_deref().unwrap_or("");
            if refs.default_branch.is_none() || !refs.branches.iter().any(|b| b == existing) {
                return Err(Error::Invalid("Select a free existing branch".into()));
            }
            expected = existing
                .strip_prefix("origin/")
                .unwrap_or(existing)
                .to_owned();
            if expected != existing {
                args.extend(["--track", "-b", &expected, "--", path, existing]);
            } else {
                args.extend(["--", path, existing]);
            }
        }
        Mode::Pr => {
            let number = request
                .pr
                .filter(|n| *n > 0 && *n <= i64::MAX as u64)
                .ok_or_else(|| Error::Invalid("Enter a positive pull request number".into()))?;
            if refs.default_branch.is_none() {
                return Err(Error::Invalid(
                    "The first workspace must start a new branch".into(),
                ));
            }
            let mut command = Command::new("gh");
            command
                .current_dir(common)
                .args(["pr", "view", &number.to_string(), "--json", "number"])
                .env("GH_PROMPT_DISABLED", "1");
            let pr = output(discovery::run_command(&mut command, 20).map_err(Error::Failed)?)?;
            let pr: Value = serde_json::from_str(&pr).map_err(|e| Error::Failed(e.to_string()))?;
            if pr["number"].as_u64() != Some(number) {
                return Err(Error::Invalid(
                    "GitHub returned a different pull request".into(),
                ));
            }
            check_git(
                common,
                &["fetch", "origin", &format!("refs/pull/{number}/head")],
            )?;
            args.extend(["-b", &branch, "--", path, "FETCH_HEAD"]);
            expected = branch.clone();
        }
    }
    std::fs::create_dir_all(&parent)?;
    check_git(common, &args)?;
    let listing = git(common, &["worktree", "list", "--porcelain", "-z"])?;
    if !listing.status.success() {
        output(listing)?;
        unreachable!();
    }
    let created = discovery::parse_worktrees(&listing.stdout)
        .into_iter()
        .find(|w| w.path == path && w.branch.as_deref() == Some(&expected))
        .ok_or_else(|| {
            Error::Failed(format!(
                "Git created {expected}, but did not list its worktree"
            ))
        })?;
    let mut settings = owner
        .project_defaults
        .get(&project.path)
        .filter(|s| s.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    settings["worktreeFolder"] = json!(if folder_setting == seed {
        None
    } else {
        Some(folder_setting)
    });
    settings["branchPrefix"] = json!(if prefix == "swarm/" {
        None
    } else {
        Some(prefix)
    });
    Ok((created.path, settings))
}
