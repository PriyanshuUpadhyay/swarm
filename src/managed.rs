//! Every write swarm makes outside its home (ADR 0042): the plan types each writer fills, the
//! digest that apply checks, and the locked, link-following, compare-before-rename file writes.

mod commit;
mod filesystem;
mod links;
mod output;
mod removal;
mod state;

pub use commit::{apply, digest, revert};
use filesystem::write_target;
pub use filesystem::{CHANGED, LOCK_WAIT, with_lock};
pub(crate) use filesystem::{
    json_object, json_text, read_json_object, read_optional, read_text, refuse_read_only,
    remove_file, retried, write_text,
};
use links::{LinkLeaf, link_leaf, link_removal, validate_link, write_link};
use output::link_text;
pub use output::{conflicts_text, diff, plan_json, plan_text};
pub use removal::{Target, revert_plan};
use state::state_in;
pub use state::{Entry, State, list, state};

/// An entry that swarm needs at a place where the file already holds another one (ADR 0036).
/// Swarm never writes over it: the owner removes it, or does without swarm's hooks.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub file: String,
    pub entry: String,
    pub found: String,
    pub wanted: String,
    pub fix: String,
}

/// Why a conflict stops a plan, so the app words `wanted` for its cause. Wire names are open.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// The owner's entry is where setup needs swarm's; `wanted` is swarm's value.
    Taken,
    /// An item swarm wrote has another value now; `wanted` is what swarm wrote.
    Changed,
    /// Swarm's Codex group has a group after it, so removing it moves trust by place.
    Order,
    /// Swarm cannot read or edit the file or the path in it.
    Unreadable,
}

/// What one writer would do to one file: its text now, its planned text, each conflict, and each
/// item that the planned text adds. An entry equal to swarm's stays as it is.
#[derive(Clone, Debug)]
pub struct FilePlan {
    pub path: std::path::PathBuf,
    pub before: String,
    pub after: String,
    pub conflicts: Vec<Conflict>,
    pub edits: Vec<Edit>,
}

/// A link leaf's exact state. Other leaf types are conflicts, not absent links.
#[derive(Clone, Debug, Eq)]
pub enum LinkState {
    Absent,
    Link(std::path::PathBuf),
}

impl PartialEq for LinkState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Absent, Self::Absent) => true,
            (Self::Link(left), Self::Link(right)) => left.as_os_str() == right.as_os_str(),
            _ => false,
        }
    }
}

/// A conflicted leaf stays in conflicts; no operation uses its before/after states.
#[derive(Clone, Debug)]
pub struct LinkPlan {
    pub path: std::path::PathBuf,
    pub before: LinkState,
    pub after: LinkState,
    pub conflicts: Vec<Conflict>,
    pub edits: Vec<Edit>,
}

/// Text writers keep FilePlan; the shared write boundary accepts either kind.
#[derive(Clone, Debug)]
pub enum Plan {
    File(FilePlan),
    Link(LinkPlan),
}

impl From<FilePlan> for Plan {
    fn from(plan: FilePlan) -> Self {
        Self::File(plan)
    }
}

impl From<LinkPlan> for Plan {
    fn from(plan: LinkPlan) -> Self {
        Self::Link(plan)
    }
}

impl Plan {
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::File(plan) => &plan.path,
            Self::Link(plan) => &plan.path,
        }
    }
    pub fn conflicts(&self) -> &[Conflict] {
        match self {
            Self::File(plan) => &plan.conflicts,
            Self::Link(plan) => &plan.conflicts,
        }
    }
    pub fn edits(&self) -> &[Edit] {
        match self {
            Self::File(plan) => &plan.edits,
            Self::Link(plan) => &plan.edits,
        }
    }
    pub fn changes(&self) -> bool {
        match self {
            Self::File(plan) => plan.before != plan.after,
            Self::Link(plan) => plan.before != plan.after,
        }
    }
}

impl FilePlan {
    /// A file that swarm cannot read or edit, as a conflict, so setup writes no file and the
    /// owner sees the fix (owner's choice, 2026-10-01).
    pub fn unreadable(path: std::path::PathBuf, error: String) -> Self {
        let file = path.display().to_string();
        Self {
            conflicts: vec![Conflict {
                kind: ConflictKind::Unreadable,
                file: file.clone(),
                entry: "the whole file".into(),
                found: error,
                wanted: "a file that swarm can read and edit".into(),
                fix: format!("repair {file}, or move it away"),
            }],
            path,
            before: String::new(),
            after: String::new(),
            edits: Vec::new(),
        }
    }
}

/// The places swarm writes. Wire names are open: a later build may add one (A4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A key in a TOML table; `path` is the tables, then the key.
    TomlKey,
    /// A key in a JSON object; `path` is the keys, the last one the item's own.
    JsonKey,
    /// One item of a JSON array; `path` is the keys to the array.
    JsonArrayItem,
    /// The leaf itself; `path` is empty and `wrote` is its exact absolute target.
    Symlink,
    /// A whole file that swarm made; `path` is empty and `wrote` is the SHA-256 of its text.
    File,
}

/// The swarm feature that wrote an item. Wire names are open: a later build may add one (A4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Writer {
    #[serde(rename = "hooks.state")]
    HooksState,
    #[serde(rename = "hooks.guard")]
    HooksGuard,
    #[serde(rename = "launch.trust")]
    LaunchTrust,
    #[serde(rename = "herdr")]
    Herdr,
    #[serde(rename = "skills")]
    Skills,
    #[serde(rename = "import.conductor")]
    ImportConductor,
}

/// One item that a writer adds to a file outside the swarm home.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub writer: Writer,
    /// The write target for text, or the resolved parent plus the leaf for a symlink.
    pub file: std::path::PathBuf,
    pub kind: Kind,
    pub path: Vec<String>,
    /// The value written, as JSON; a TOML value keeps its type (string, integer, float, bool).
    pub wrote: serde_json::Value,
    /// The value the write replaced; None when the place was absent.
    pub before: Option<serde_json::Value>,
    /// How many containers at the end of `path` (tables, objects, the array) the file lacked
    /// before the write, so a revert removes them again once they are empty.
    pub created: u8,
    /// The id of an edit that is always reverted with this one.
    pub with: Option<String>,
    /// An array item that must stay the last item, so a revert refuses it while items follow it:
    /// they would move. The writer sets it; it is not part of the id.
    pub last: bool,
}

impl Edit {
    /// A new item at `path` of `file`, with nothing before it.
    pub fn new(
        writer: Writer,
        file: &std::path::Path,
        kind: Kind,
        path: &[&str],
        wrote: serde_json::Value,
    ) -> Self {
        Self {
            writer,
            file: match kind {
                Kind::Symlink => link_place(file),
                _ => place(file),
            },
            kind,
            path: path.iter().map(|part| part.to_string()).collect(),
            wrote,
            before: None,
            created: 0,
            with: None,
            last: false,
        }
    }

    /// The first 12 hex digits of a hash of the place, so a found item and a recorded one of one
    /// place share an id. An array item adds its value, because two items share one array.
    pub fn id(&self) -> String {
        use sha2::Digest;
        let mut digest = sha2::Sha256::new();
        digest.update(self.file.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(wire(&self.kind).as_bytes());
        for part in &self.path {
            digest.update([0]);
            digest.update(part.as_bytes());
        }
        if self.kind == Kind::JsonArrayItem {
            digest.update([0]);
            digest.update(self.wrote.to_string().as_bytes());
        }
        digest.finalize()[..6]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// The file a write to `file` lands in, with its folder's links resolved too, so two names of one
/// file, or of one missing file, are one place. The nearest folder that exists is resolved and the
/// missing rest joined to it, so a file whose folder the write makes has one place before and after.
fn place(file: &std::path::Path) -> std::path::PathBuf {
    let target = write_target(file).unwrap_or_else(|_| file.to_path_buf());
    link_place(&target)
}

/// Resolve the parent and missing folders without following the leaf.
fn link_place(file: &std::path::Path) -> std::path::PathBuf {
    let mut missing = Vec::new();
    let mut dir = file;
    while let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) {
        missing.push(name);
        if let Ok(real) = std::fs::canonicalize(parent) {
            return missing
                .iter()
                .rev()
                .fold(real, |path, name| path.join(name));
        }
        dir = parent;
    }
    file.to_path_buf()
}

/// The wire name of a `Kind` or `Writer`.
pub fn wire(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => unreachable!("Kind and Writer serialize as strings"),
    }
}

/// The `wrote` of a `Kind::File` item.
pub fn file_hash(text: &str) -> serde_json::Value {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(text.as_bytes())).into()
}

pub(crate) fn changed_error(path: &std::path::Path) -> String {
    format!("swarm: {} {CHANGED}", path.display())
}

#[cfg(test)]
mod tests {
    use super::filesystem::with_lock_within;
    use super::removal::removal;
    use super::*;

    struct LinkFixture {
        root: std::path::PathBuf,
        link: std::path::PathBuf,
        target: std::path::PathBuf,
        store: rusqlite::Connection,
    }

    impl LinkFixture {
        fn new(role: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("swarm-link-{role}-{}", std::process::id()));
            std::fs::create_dir_all(root.join("home/skills")).unwrap();
            std::fs::create_dir_all(root.join("swarm/default-skill")).unwrap();
            let store = crate::store::open(&root.join("swarm/managed.db")).unwrap();
            Self {
                link: root.join("home/skills/skill-link"),
                target: root.join("swarm/default-skill"),
                root,
                store,
            }
        }
        fn plan(&self) -> Plan {
            LinkPlan::create(&self.store, self.link.clone(), self.target.clone())
                .unwrap()
                .into()
        }
        fn edit(&self) -> Edit {
            Edit::new(
                Writer::Skills,
                &self.link,
                Kind::Symlink,
                &[],
                self.target.to_str().unwrap().into(),
            )
        }
    }

    impl Drop for LinkFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    include!("managed/tests/link_ownership.rs");
    include!("managed/tests/link_failures.rs");
    include!("managed/tests/link_identity.rs");
    include!("managed/tests/text.rs");
}
