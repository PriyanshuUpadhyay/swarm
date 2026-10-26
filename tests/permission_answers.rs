use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

const SCREEN: &str = "────────────────────────────────────────\n Bash command\n\n touch /tmp/work/probe.txt\n Do you want to proceed?\n ❯ 1. Yes\n   2. Yes, and always allow access to /tmp/work from this project\n   3. Yes, and switch to auto mode · auto mode handles these prompts for you\n   4. No\n\n Esc to cancel · Tab to amend\n";

struct Fixture {
    home: PathBuf,
    session: String,
}

impl Fixture {
    fn new() -> Self {
        let home = std::env::temp_dir().join(format!("swarm-permission-{}", uuid::Uuid::now_v7()));
        let root = home.join(".swarm");
        fs::create_dir_all(root.join("adapters")).unwrap();
        fs::write(root.join("swarm-home"), "swarm\n").unwrap();
        fs::write(home.join("screen"), SCREEN).unwrap();
        fs::write(root.join("adapters/fixture.conf"),
            "self = printf worker-pane\nspawn = true\nring = true\nlist = printf 'worker-pane sh\\n'\nclose = true\ncapture = cat \"$HOME/screen\"\nscreen = cat \"$HOME/screen\"\nkey = printf '%s\\n' \"$SWARM_KEY\" >> \"$HOME/keys\"\n").unwrap();
        let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let session =
            swarm::store::create_session(&connection, "lane", &home, None, Some("fixture"))
                .unwrap();
        swarm::store::add_agent(&connection, &session, "worker", "chat").unwrap();
        swarm::store::set_provider(&connection, &session, "worker", "claude").unwrap();
        swarm::store::set_pane(&connection, &session, "worker", "worker-pane").unwrap();
        Self { home, session }
    }

    fn connection(&self) -> rusqlite::Connection {
        swarm::store::open(&self.home.join(".swarm/swarm.db")).unwrap()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("SWARM_HOME", &self.home)
            .env("SWARM_SESSION_ID", &self.session)
            .env("SWARM_ADAPTER", "fixture")
            .env("PATH", "/usr/bin:/bin")
            .current_dir(&self.home)
            .args(args);
        command
    }

    fn payload(&self) -> Value {
        json!({"hook_event_name":"PermissionRequest", "prompt_id":"native-prompt", "tool_name":"Bash",
        "tool_input":{"command":"touch /tmp/work/probe.txt"},
        "permission_suggestions":[
            {"type":"addDirectories", "directories":["/tmp/work"], "destination":"session"},
            {"type":"setMode", "mode":"acceptEdits", "destination":"session"}
        ]})
    }

    fn hook(&self, payload: &Value) -> RunningHook {
        let mut child = self
            .command(&["hook", "claude"])
            .env("SWARM_AGENT_ID", "worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .unwrap();
        RunningHook(Some(child))
    }

    fn wait_for_request(&self, hook: &mut RunningHook) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.request_file().is_some() {
                return;
            }
            if let Some(status) = hook.0.as_mut().unwrap().try_wait().unwrap() {
                panic!("permission hook exited without waiting: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "hook did not register its permission request"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn request_file(&self) -> Option<PathBuf> {
        let folder = self
            .home
            .join(".swarm/runs")
            .join(&self.session)
            .join("permission");
        fs::read_dir(folder)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("request.json"))
            .find(|path| path.is_file())
    }

    fn prompt(&self) -> Value {
        let output = self.command(&["agents", "--json"]).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let listing: Value = serde_json::from_slice(&output.stdout).unwrap();
        listing["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "worker")
            .unwrap()["prompt"]
            .clone()
    }

    fn answer(&self, prompt: &Value, choice: &str) -> Output {
        self.command(&["answer", "worker", prompt["id"].as_str().unwrap(), choice])
            .output()
            .unwrap()
    }

    fn assert_no_keys(&self) {
        assert!(
            !self.home.join("keys").exists(),
            "an answer sent terminal keys"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

struct RunningHook(Option<Child>);

impl RunningHook {
    fn output(mut self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "hook did not consume the answer");
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = self.0.take().unwrap().wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

impl Drop for RunningHook {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn claude_owner_answers_use_the_waiting_hook_and_never_keys() {
    for (choice, expected) in [
        ("0", json!({"behavior":"allow"})),
        (
            "1",
            json!({"behavior":"allow", "updatedPermissions":[{"type":"addDirectories", "directories":["/tmp/work"], "destination":"session"}]}),
        ),
        (
            "3",
            json!({"behavior":"deny", "message":"The owner denied this request."}),
        ),
    ] {
        let fixture = Fixture::new();
        let mut hook = fixture.hook(&fixture.payload());
        fixture.wait_for_request(&mut hook);
        let prompt = fixture.prompt();
        assert!(
            prompt["id"].as_str().unwrap().contains('.'),
            "request identity missing: {prompt}"
        );
        let state: String = fixture
            .connection()
            .query_row("SELECT state FROM agent WHERE id = 'worker'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(state, "waiting");
        let answer = fixture.answer(&prompt, choice);
        assert!(answer.status.success(), "{answer:?}");
        assert_eq!(hook.output(), swarm::permission::output(expected));
        fixture.assert_no_keys();
        let second = fixture.answer(&prompt, choice);
        assert!(!second.status.success(), "{second:?}");
        assert!(
            String::from_utf8_lossy(&second.stderr).contains("already answered"),
            "{second:?}"
        );
        fixture.assert_no_keys();
    }
}

#[test]
fn a_choice_without_a_hook_form_closes_the_hook_and_uses_keys() {
    let fixture = Fixture::new();
    let mut hook = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut hook);
    let prompt = fixture.prompt();
    let answer = fixture.answer(&prompt, "2");
    assert!(answer.status.success(), "{answer:?}");
    assert_eq!(hook.output(), json!({}));
    assert_eq!(
        fs::read_to_string(fixture.home.join("keys")).unwrap(),
        "3\n"
    );
    assert!(!fixture.answer(&prompt, "2").status.success());
    assert_eq!(
        fs::read_to_string(fixture.home.join("keys")).unwrap(),
        "3\n"
    );
}

#[test]
fn a_permission_without_a_waiting_hook_keeps_the_key_path() {
    let fixture = Fixture::new();
    let prompt = fixture.prompt();
    assert!(fixture.answer(&prompt, "0").status.success());
    assert_eq!(
        fs::read_to_string(fixture.home.join("keys")).unwrap(),
        "1\n"
    );
}

#[test]
fn an_expired_answer_fails_and_the_still_visible_dialog_can_use_keys() {
    let fixture = Fixture::new();
    let mut hook = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut hook);
    let old = fixture.prompt();
    let path = fixture.request_file().unwrap();
    let mut request: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    request["expires_at_ms"] = json!(0);
    fs::write(path, request.to_string()).unwrap();
    assert!(!fixture.answer(&old, "0").status.success());
    fixture.assert_no_keys();
    assert_eq!(hook.output(), json!({}));
    let current = fixture.prompt();
    assert_ne!(old["id"], current["id"]);
    assert!(fixture.answer(&current, "0").status.success());
}

#[test]
fn terminal_completion_closes_the_waiting_hook_without_a_decision() {
    let fixture = Fixture::new();
    let mut hook = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut hook);
    let old = fixture.prompt();
    let mut completed = fixture.payload();
    completed["hook_event_name"] = json!("PostToolUse");
    assert_eq!(fixture.hook(&completed).output(), json!({}));
    assert_eq!(hook.output(), json!({}));
    assert!(!fixture.answer(&old, "0").status.success());
    fixture.assert_no_keys();
}

#[test]
fn an_agent_pane_cannot_answer_a_waiting_hook() {
    let fixture = Fixture::new();
    let mut hook = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut hook);
    let prompt = fixture.prompt();
    let answer = fixture
        .command(&["answer", "worker", prompt["id"].as_str().unwrap(), "0"])
        .env("TMUX_PANE", "%1")
        .output()
        .unwrap();
    assert!(!answer.status.success());
    assert!(String::from_utf8_lossy(&answer.stderr).contains("only the owner"));
    fixture.assert_no_keys();
}

#[test]
fn two_owner_answers_have_exactly_one_winner() {
    let fixture = Fixture::new();
    let mut hook = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut hook);
    let prompt = fixture.prompt();
    let barrier = std::sync::Barrier::new(3);
    let (allow, deny) = std::thread::scope(|scope| {
        let allow = scope.spawn(|| {
            barrier.wait();
            fixture.answer(&prompt, "0")
        });
        let deny = scope.spawn(|| {
            barrier.wait();
            fixture.answer(&prompt, "3")
        });
        barrier.wait();
        (allow.join().unwrap(), deny.join().unwrap())
    });
    assert_ne!(
        allow.status.success(),
        deny.status.success(),
        "allow {allow:?}; deny {deny:?}"
    );
    let behavior = if allow.status.success() {
        "allow"
    } else {
        "deny"
    };
    assert_eq!(
        hook.output()["hookSpecificOutput"]["decision"]["behavior"],
        behavior
    );
    fixture.assert_no_keys();
}

#[test]
fn a_late_answer_cannot_reach_an_identical_new_request() {
    let fixture = Fixture::new();
    let mut first = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut first);
    let old = fixture.prompt();
    assert!(fixture.answer(&old, "0").status.success());
    assert_eq!(
        first.output()["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
    let mut completed = fixture.payload();
    completed["hook_event_name"] = json!("PostToolUse");
    assert_eq!(fixture.hook(&completed).output(), json!({}));
    let mut second = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut second);
    let current = fixture.prompt();
    assert_ne!(old["id"], current["id"]);
    assert!(!fixture.answer(&old, "0").status.success());
    fixture.assert_no_keys();
    assert!(fixture.answer(&current, "3").status.success());
    assert_eq!(
        second.output()["hookSpecificOutput"]["decision"]["behavior"],
        "deny"
    );
    fixture.assert_no_keys();
}

#[test]
fn broken_request_data_fails_without_sending_keys() {
    let fixture = Fixture::new();
    let mut hook = fixture.hook(&fixture.payload());
    fixture.wait_for_request(&mut hook);
    let prompt = fixture.prompt();
    fs::write(fixture.request_file().unwrap(), "broken JSON").unwrap();
    // The listing still shows every agent.
    fixture.prompt();
    assert!(!fixture.answer(&prompt, "0").status.success());
    fixture.assert_no_keys();
}

#[test]
fn questions_and_setup_screens_keep_the_key_path() {
    let fixture = Fixture::new();
    let question = json!({"hook_event_name":"PermissionRequest", "tool_name":"AskUserQuestion", "tool_input":{"questions":[]}});
    assert_eq!(fixture.hook(&question).output(), json!({}));
    assert!(fixture.request_file().is_none());
    fs::write(fixture.home.join("screen"), "────────────────────────────────────────\n Do you trust this folder?\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n\n Press enter to confirm\n").unwrap();
    let prompt = fixture.prompt();
    assert!(fixture.answer(&prompt, "0").status.success());
    assert_eq!(
        fs::read_to_string(fixture.home.join("keys")).unwrap(),
        "1\n"
    );
}
