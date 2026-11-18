/// The chat worktree part of `lifecycle_routes_child` (ADR 0146, ADR 0147), in its environment.
async fn chat_worktrees(fixture: &Fixture, root: &Path, connection: &rusqlite::Connection) {
    // ADR 0146: a new chat on a hub's main checkout runs in its own new worktree, after setup.
    let hub = root.join("hub");
    std::fs::create_dir(&hub).unwrap();
    let sh = |script: &str| {
        let status = std::process::Command::new("/bin/sh")
            .args(["-c", script])
            .current_dir(&hub)
            .status()
            .unwrap();
        assert!(status.success(), "{script}");
    };

    sh("git init -q --bare .bare && git -C .bare symbolic-ref HEAD refs/heads/main \
        && git --git-dir=.bare worktree add -q --orphan -b main wt/main \
        && echo a > wt/main/a.txt && git -C wt/main add a.txt && git -C wt/main commit -q -m a \
        && echo setup-ran >> .bare/info/exclude");

    // Setup output is ignored, as `node_modules` is; a file git sees would keep the worktree.
    let cli = r#"#!/bin/sh
printf '%s\n' "$PWD|$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
'session new') /usr/bin/uuidgen | /usr/bin/tr A-Z a-z ;;
'launch orchestrator') printf 'fixture-pane\n' ;;
esac
"#;
    std::fs::write(&fixture.app.cli, cli).unwrap();

    let hub_path = hub.to_string_lossy().into_owned();
    let main = hub.join("wt/main").to_string_lossy().into_owned();
    choices_store::update(&fixture.app.root, None, |owner, _| {
        lifecycle::remember(owner, &hub_path, &hub_path);
        owner
            .project_defaults
            .insert(hub_path.clone(), json!({"setup_script":"touch setup-ran"}));
        Ok(())
    })
    .unwrap();

    let started = start_chat(fixture, json!({"cwd":main}))
        .await
        .unwrap_or_else(|e| panic!("{e:?} {}", action_calls(fixture)));

    let chat = started["workspace"].as_str().unwrap().to_owned();
    let branch = lifecycle::check_git(&chat, &["branch", "--show-current"]).unwrap();
    assert!(swarm::task::unnamed_chat(&branch), "{chat} {branch}");
    assert!(Path::new(&chat).join("setup-ran").is_file());
    assert!(swarm::task::records(&fixture.app.root).iter().any(|task| {
        task.path == chat && task.session.as_deref() == started["session"].as_str()
    }));

    let calls = action_calls(fixture);
    assert!(calls.lines().last().unwrap().contains(&format!("{chat}|launch orchestrator chat")), "{calls}");

    // The fake CLI makes no session row, so the test adds the one `swarm session new` would.
    let chat_session = |started: &Value, cwd: &str| {
        let id = started["session"].as_str().unwrap().to_owned();
        connection
            .execute(
                "INSERT INTO session (id, talk_mode, cwd, adapter) VALUES (?1, 'lane', ?2, 'tmux-swarm')",
                (&id, cwd),
            )
            .unwrap();
        swarm::store::add_agent(connection, &id, "orchestrator", "chat").unwrap();
        id
    };

    let id = chat_session(&started, &chat);
    let send = json!({"session":id,"agent":"orchestrator","text":"Fix the login redirect bug now please"});
    let _ = call_action(fixture, "/api/chat/send", send).await.unwrap();

    assert_eq!(
        lifecycle::check_git(&chat, &["branch", "--show-current"]).unwrap(),
        "swarm/fix-login-redirect-bug-now"
    );
    assert_eq!(
        choices_store::load(&fixture.app.root, true).unwrap().owner.names[&chat],
        "fix-login-redirect-bug-now"
    );

    // An archived chat that got no prompt takes its empty worktree with it; one with a commit stays.
    if !root.join("bin/wt").is_file() {
        eprintln!("skipped: archive of a chat worktree, because Worktrunk is not installed");
    } else {
        let mut folders = Vec::new();
        for work in [false, true] {
            let started = start_chat(fixture, json!({"cwd":main})).await.unwrap();

            let folder = started["workspace"].as_str().unwrap().to_owned();
            if work {
                sh(&format!("cd {folder} && echo b > b.txt && git add b.txt && git commit -q -m b"));
            }

            let id = chat_session(&started, &folder);
            let _ = call_action(fixture, "/api/sessions/archive", json!({"ids":[id]}))
                .await
                .unwrap();
            folders.push(folder);
        }
        assert!(!Path::new(&folders[0]).exists());
        assert!(Path::new(&folders[1]).is_dir());

        // A live chat at the top of an unused worktree keeps it when a chat in a subfolder of it is
        // archived.
        let first = start_chat(fixture, json!({"cwd":main})).await.unwrap();

        let shared = first["workspace"].as_str().unwrap().to_owned();
        let second = start_chat(fixture, json!({"cwd":shared})).await.unwrap();

        assert_eq!(second["workspace"], json!(shared));
        std::fs::create_dir(Path::new(&shared).join("sub")).unwrap();
        let id = chat_session(&first, &format!("{shared}/sub"));
        chat_session(&second, &shared);
        let _ = call_action(fixture, "/api/sessions/archive", json!({"ids":[id]}))
            .await
            .unwrap();
        assert!(Path::new(&shared).is_dir());
    }

    // New chat in main keeps the chat in main.
    let kept = start_chat(fixture, json!({"cwd":main,"place":{"kind":"here"}}))
        .await
        .unwrap();
    assert_eq!(kept["workspace"], json!(main));

    // ADR 0147: a first prompt names the new worktree when it is made, and goes to the chair.
    let id = uuid::Uuid::new_v4().to_string();
    let named = hub.join("wt/fix-flaky-smoke-test").to_string_lossy().into_owned();
    chat_session(&json!({"session": id}), &named);

    let cli = format!(
        r#"#!/bin/sh
printf '%s\n' "$PWD|$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
'session new') printf '%s\n' '{id}' ;;
'launch orchestrator') printf 'fixture-pane\n' ;;
esac
if [ "$1" = type ]; then cat > "$SWARM_HOME/typed"; fi
"#
    );
    std::fs::write(&fixture.app.cli, &cli).unwrap();

    let prompt = "Fix the flaky smoke test";
    let query = HashMap::from([("cwd".into(), main.clone()), ("prompt".into(), prompt.into())]);
    let preview = new_chat_preview(State(fixture.app.clone()), Query(query))
        .await
        .unwrap()
        .0;
    assert_eq!(
        (&preview["branch"], &preview["folder"], &preview["setup"]),
        (&json!("swarm/fix-flaky-smoke-test"), &json!(named), &json!("touch setup-ran"))
    );

    let started = start_chat(fixture, json!({"cwd":main,"prompt":prompt}))
        .await
        .unwrap_or_else(|e| panic!("{e:?} {}", action_calls(fixture)));
    assert_eq!(started["workspace"], json!(named));
    assert_eq!(
        lifecycle::check_git(&named, &["branch", "--show-current"]).unwrap(),
        "swarm/fix-flaky-smoke-test"
    );
    assert!(action_calls(fixture).ends_with(&format!("{named}|type orchestrator\n")));
    let home = fixture.app.root.parent().unwrap();
    assert_eq!(std::fs::read_to_string(home.join("typed")).unwrap(), prompt);

    // ADR 0147: place `new` in a git project that is no hub still starts the chat in a new worktree.
    let clone = root.join("clone");
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "git init -q -b main clone && cd clone && echo a > a.txt && git add a.txt && git -c user.name=t -c user.email=t@t commit -q -m a"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());

    let clone_path = clone.to_string_lossy().into_owned();
    choices_store::update(&fixture.app.root, None, |owner, _| {
        lifecycle::remember(owner, &clone_path, &clone_path);
        Ok(())
    })
    .unwrap();
    std::fs::write(&fixture.app.cli, cli.replace(&format!("printf '%s\\n' '{id}'"), "/usr/bin/uuidgen | /usr/bin/tr A-Z a-z")).unwrap();

    let body = json!({"cwd":clone_path,"prompt":"Add a retry","place":{"kind":"new"}});
    let started = start_chat(fixture, body)
        .await
        .unwrap_or_else(|e| panic!("{e:?} {}", action_calls(fixture)));
    let made = started["workspace"].as_str().unwrap();
    assert_eq!(
        lifecycle::check_git(made, &["branch", "--show-current"]).unwrap(),
        "swarm/add-retry"
    );
    assert!(action_calls(fixture).contains(&format!("{made}|launch orchestrator chat")));

    // Two starts with one prompt at once wait for each other's worktree, so each gets a name.
    let body = json!({"cwd":clone_path,"prompt":"Add a cache","place":{"kind":"new"}});
    let (first, second) = tokio::join!(start_chat(fixture, body.clone()), start_chat(fixture, body));
    let mut branches: Vec<_> = [first, second]
        .iter()
        .map(|started| {
            let made = started.as_ref().unwrap_or_else(|e| panic!("{e:?}"))["workspace"].as_str().unwrap();
            lifecycle::check_git(made, &["branch", "--show-current"]).unwrap()
        })
        .collect();
    branches.sort();
    assert_eq!(branches, ["swarm/add-cache", "swarm/add-cache-2"]);

    // A free branch whose folder is taken, such as a worktree another tool made, takes the next name.
    std::fs::create_dir(Path::new(made).with_file_name("add-docs")).unwrap();
    let body = json!({"cwd":clone_path,"prompt":"Add docs","place":{"kind":"new"}});
    let started = start_chat(fixture, body).await.unwrap_or_else(|e| panic!("{e:?}"));
    let docs = started["workspace"].as_str().unwrap();
    assert_eq!(
        lifecycle::check_git(docs, &["branch", "--show-current"]).unwrap(),
        "swarm/add-docs-2"
    );
}
