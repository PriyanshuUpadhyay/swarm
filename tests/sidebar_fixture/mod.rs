#![allow(dead_code)]
use serde_json::{Value, json};
use std::path::PathBuf;
use swarm::view::{
    choices::{OwnerChoices, ViewState},
    tree::{self, Tree},
};

pub fn session(id: &str, workspace: &str, at: i64) -> Value {
    json!({"id":id,"cwd":workspace,"workspace":workspace,"project":"/repo","project_id":"repository:/repo/.bare","created_at":at,"agents":2,"chair_provider":"codex","chair_log":format!("/logs/{id}.jsonl"),"adapter":"tmux-swarm","title":id})
}
pub fn agent(id: &str, status: &str) -> Value {
    json!({"id":id,"role":if id == "orchestrator" { "chair" } else { "code" },"pane":format!("pane-{id}"),"alive":status != "ended","state":status,"provider":"codex"})
}
pub fn snapshot(sessions: Vec<Value>, agents: Value) -> Value {
    json!({"sessions":sessions,"agents":agents,"projects":[{"id":"repository:/repo/.bare","path":"/repo","common":"/repo/.bare","launch_directory":"/repo/wt/main","workspaces":[{"path":"/repo/wt/main","branch":"main"},{"path":"/repo/wt/feature","branch":"feature"},{"path":"/repo/wt/empty","branch":"empty"}]}]})
}
pub fn owner() -> OwnerChoices {
    let mut owner = OwnerChoices::default();
    owner.project_paths.push("/repo/wt/main".into());
    owner
}
pub fn tree() -> Tree {
    tree::build(
        &snapshot(
            vec![
                session("design", "/repo/wt/main", 1),
                session("review", "/repo/wt/main", 2),
                session("ended", "/repo/wt/main", 3),
                session("notes", "/repo/wt/feature", 4),
            ],
            json!({"design":[agent("orchestrator","working")],"review":[agent("orchestrator","waiting")],"ended":[agent("orchestrator","ended")],"notes":[agent("orchestrator","done")]}),
        ),
        &owner(),
    )
}
pub fn folder_snapshot(sessions: Vec<Value>, agents: Value) -> Value {
    let sessions: Vec<_> = sessions
        .into_iter()
        .map(|mut s| {
            s["project"] = json!("/repo");
            s["project_id"] = json!("folder:/repo");
            s["workspace"] = json!("/repo");
            s["cwd"] = json!("/repo");
            s
        })
        .collect();
    json!({"sessions":sessions,"agents":agents,"projects":[{"id":"folder:/repo","path":"/repo","launch_directory":"/repo","workspaces":[{"path":"/repo"}]}]})
}
pub fn sections(
    snapshot: &Value,
    owner: &OwnerChoices,
    state: &ViewState,
    archive: bool,
) -> Vec<tree::Section> {
    tree::sections(
        &tree::build(snapshot, owner),
        owner,
        state,
        archive,
        100,
        &Default::default(),
    )
}
pub struct ChoicesHome {
    pub root: PathBuf,
}
impl ChoicesHome {
    pub fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("swarm-choice-fixture-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("swarm-home"), "swarm\n").unwrap();
        Self { root }
    }
}
impl Drop for ChoicesHome {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
