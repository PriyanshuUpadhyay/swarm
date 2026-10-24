// A launch or spawn names a model; swarm checks it against the provider's model list first.

use super::*;

/// `codex debug models` for the two fake accounts: each lists its own models.
const ACCOUNT_MODELS: &str = r#"case "$CODEX_HOME" in
  */.codex-work) echo '{"models":[{"slug":"gpt-6-luna","visibility":"list"},{"slug":"gpt-6-sol","visibility":"list"}]}' ;;
  */.codex-spare) echo '{"models":[{"slug":"gpt-6-luna","visibility":"list"},{"slug":"gpt-6-ghost","visibility":"list"}]}' ;;
  *) echo '{"models":[{"slug":"gpt-6-luna","visibility":"list"}]}' ;;
esac"#;

struct Fixture {
    home: PathBuf,
    env: Vec<(&'static str, String)>,
    cwd: String,
}

impl Fixture {
    /// A chair session on a fake adapter whose `spawn` verb touches `$HOME/spawned`, a `claude`
    /// that holds `"claude-opus-5-5"` and `"sonnet"` as quoted strings, and a `codex` with accounts `work`
    /// and `spare` whose `debug models` runs `codex_debug`.
    fn new(name: &str, codex_debug: &str) -> Self {
        let home = scratch(name);
        tool(&home, "claude", "# \"claude-opus-5-5\" \"sonnet\"\ntrue");
        let codex = format!(
            "if [ \"$1\" = debug ]; then\n{codex_debug}\nexit 0\nfi\n{}",
            include_str!("../fixtures/accounts/work-app-server.sh")
        );
        tool(&home, "codex", &codex);
        std::fs::create_dir_all(home.join(".codex-work")).unwrap();
        std::fs::create_dir_all(home.join(".codex-spare")).unwrap();
        std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
        std::fs::write(
            home.join(".swarm/adapters/fake.conf"),
            "self = printf chair\nspawn = touch \"$HOME/spawned\"; printf pane\nring = true\n\
             list = true\nclose = true\ncapture = true\n",
        )
        .unwrap();
        let session = swarm(
            &home,
            &[("SWARM_ADAPTER", "fake")],
            &["session", "new", "lane"],
        );
        assert!(session.status.success(), "{}", stderr(&session));
        let session = String::from_utf8(session.stdout)
            .unwrap()
            .trim()
            .to_string();
        let env = vec![
            ("SWARM_ADAPTER", "fake".to_string()),
            ("SWARM_SESSION_ID", session),
            ("SWARM_AGENT_ID", "orchestrator".to_string()),
        ];
        let cwd = home.join("repo");
        std::fs::create_dir_all(&cwd).unwrap();
        let fixture = Fixture {
            home,
            env,
            cwd: cwd.to_string_lossy().into_owned(),
        };
        let added = fixture.run(&["agent", "add", "orchestrator", "orchestrator"]);
        assert!(added.status.success(), "{}", stderr(&added));
        fixture
    }

    fn run(&self, args: &[&str]) -> Output {
        let env: Vec<(&str, &str)> = self
            .env
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        swarm(&self.home, &env, args)
    }

    /// `swarm launch` or `swarm spawn` of the seat `seat` on `provider` with `model`.
    fn start(&self, verb: &str, provider: &str, model: &str, account: Option<&str>) -> Output {
        let mut args = vec![verb, "seat"];
        match verb {
            "launch" => {
                args.extend(["chat", "--provider", provider, "--model", model]);
                if let Some(account) = account {
                    args.extend(["--account", account]);
                }
                args.extend(["--cwd", &self.cwd]);
            }
            _ => {
                args.push("coder");
                if let Some(account) = account {
                    args.extend(["--provider", provider, "--account", account]);
                }
                let flag = if provider == "codex" { "-m" } else { "--model" };
                args.extend(["--", provider, flag, model]);
            }
        }
        self.run(&args)
    }

    fn seat_listed(&self) -> bool {
        let output = self.run(&["agents", "--json"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        list["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|agent| agent["id"] == "seat")
    }

    fn adapter_spawned(&self) -> bool {
        self.home.join("spawned").exists()
    }

    fn assert_started(&self, output: &Output, what: &str) {
        let text = stderr(output);
        assert!(output.status.success(), "{what}: {text}");
        assert!(self.adapter_spawned(), "{what}: {text}");
        assert!(self.seat_listed(), "{what}: {text}");
        assert!(!text.contains("is not in the"), "{what}: {text}");
    }

    fn assert_refused(&self, output: &Output, what: &str) -> String {
        let text = stderr(output);
        assert!(!output.status.success(), "{what}: {text}");
        assert!(!self.adapter_spawned(), "{what}: adapter ran: {text}");
        assert!(!self.seat_listed(), "{what}: row left: {text}");
        text
    }
}

#[test]
fn model_check_refuses_a_codex_model_that_the_account_does_not_list() {
    for verb in ["launch", "spawn"] {
        let fixture = Fixture::new(&format!("model-check-refuse-{verb}"), ACCOUNT_MODELS);
        // gpt-6-ghost is in spare's list only, so the check must read the picked account's list.
        let output = fixture.start(verb, "codex", "gpt-6-ghost", Some("work"));
        let text = fixture.assert_refused(&output, verb);
        assert!(
            text.contains(
                "swarm: model gpt-6-ghost is not in the codex model list for account work"
            ),
            "{verb}: {text}"
        );
        assert!(text.contains("fixes that might work:"), "{verb}: {text}");
        let accounts = text
            .lines()
            .find(|line| line.contains("another account"))
            .unwrap_or_else(|| panic!("{verb}: no another-account line: {text}"));
        assert!(accounts.contains("--account spare"), "{verb}: {text}");
        assert!(!accounts.contains("work"), "{verb}: {text}");
        let models = text
            .lines()
            .find(|line| line.contains("another model"))
            .unwrap_or_else(|| panic!("{verb}: no another-model line: {text}"));
        assert!(models.contains("gpt-6-luna, gpt-6-sol"), "{verb}: {text}");
        assert!(!models.contains("gpt-6-ghost"), "{verb}: {text}");
    }
}

#[test]
fn model_check_lets_a_codex_model_that_the_account_lists_start() {
    for verb in ["launch", "spawn"] {
        let fixture = Fixture::new(&format!("model-check-listed-{verb}"), ACCOUNT_MODELS);
        let output = fixture.start(verb, "codex", "gpt-6-sol", Some("work"));
        fixture.assert_started(&output, verb);
    }
    // The model that work refuses is listed for spare.
    let fixture = Fixture::new("model-check-listed-spare", ACCOUNT_MODELS);
    let output = fixture.start("launch", "codex", "gpt-6-ghost", Some("spare"));
    fixture.assert_started(&output, "spare");
}

#[test]
fn model_check_goes_on_when_codex_cannot_print_its_list() {
    for (name, debug) in [
        ("garbage", "echo 'not json'"),
        ("empty", "true"),
        ("failing", "echo 'no list' >&2; exit 1"),
    ] {
        for verb in ["launch", "spawn"] {
            let fixture = Fixture::new(&format!("model-check-{name}-{verb}"), debug);
            let output = fixture.start(verb, "codex", "gpt-6-ghost", Some("work"));
            fixture.assert_started(&output, &format!("{name} {verb}"));
        }
    }
}

#[test]
fn model_check_passes_a_claude_full_id_that_the_binary_holds_and_an_alias() {
    for verb in ["launch", "spawn"] {
        for model in ["claude-opus-5-5", "haiku"] {
            let fixture = Fixture::new(
                &format!("model-check-claude-{verb}-{model}"),
                ACCOUNT_MODELS,
            );
            let output = fixture.start(verb, "claude", model, None);
            fixture.assert_started(&output, &format!("{verb} {model}"));
        }
    }
}

#[test]
fn model_check_refuses_a_claude_id_that_is_neither_an_alias_nor_in_the_binary() {
    for verb in ["launch", "spawn"] {
        let fixture = Fixture::new(
            &format!("model-check-claude-unknown-{verb}"),
            ACCOUNT_MODELS,
        );
        let output = fixture.start(verb, "claude", "claude-opus-9-9", None);
        let text = fixture.assert_refused(&output, verb);
        assert!(
            text.contains(
                "swarm: model claude-opus-9-9 is not in the claude model list for the CLI's own login"
            ),
            "{verb}: {text}"
        );
        let models = text
            .lines()
            .find(|line| line.contains("another model"))
            .unwrap_or_else(|| panic!("{verb}: no another-model line: {text}"));
        assert!(models.contains("opus"), "{verb}: {text}");
    }
}

/// A `claude` file without the quoted alias `"sonnet"` is a wrapper or a fake, so its list cannot
/// be read and the launch goes on.
#[test]
fn model_check_goes_on_when_the_claude_file_holds_no_model_names() {
    for verb in ["launch", "spawn"] {
        let fixture = Fixture::new(&format!("model-check-claude-plain-{verb}"), ACCOUNT_MODELS);
        tool(&fixture.home, "claude", "true");
        let output = fixture.start(verb, "claude", "claude-opus-9-9", None);
        fixture.assert_started(&output, verb);
    }
}
