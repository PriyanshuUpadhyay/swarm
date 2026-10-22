//! CLI dispatch.

use super::*;

pub(crate) fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(done) = commands_info(args)? {
        return Ok(done);
    }
    let root = swarm::paths::root_dir()?;
    if let [cmd, sub, name] = args
        && cmd == "adapter"
        && sub == "check"
    {
        swarm::adapter::load(&root, name)?;
        // Which verbs a deployed file takes over, because a parse alone says nothing: the file
        // that stopped every chat parsed cleanly and answered `self` with the wrong line.
        let path = root.join("adapters").join(format!("{name}.conf"));
        match std::fs::read_to_string(&path) {
            Ok(text) => match swarm::adapter::overrides(name, &text)?.join(" ") {
                named if named.is_empty() => println!("ok {name}: shipped"),
                named => println!("ok {name}: shipped, with {named} from {}", path.display()),
            },
            Err(_) => println!("ok {name}: shipped"),
        }
        return Ok(());
    }
    // A sandbox that denies the Herdr socket usually denies this database too, so the three calls
    // that start an orchestrator run check the socket first, and the caller reads that reason, not
    // a database error.
    let starts_a_run = match args {
        [cmd, ..] if cmd == "launch" => true,
        [cmd, sub, ..] => (cmd == "session" && sub == "new") || (cmd == "agent" && sub == "add"),
        _ => false,
    };
    if starts_a_run && adapter_name() == "herdr" {
        let status = run_tool("herdr", &["status"])?;
        if let Some(reason) = swarm::bus::socket_refusal(&String::from_utf8_lossy(&status.stderr)) {
            return Err(reason.into());
        }
    }
    let mut connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    if let Some(done) = commands_session(args, &root, &mut connection)? {
        return Ok(done);
    }
    if let Some(done) = commands_launch(args, &root, &mut connection)? {
        return Ok(done);
    }
    if let Some(done) = commands_pane(args, &root, &mut connection)? {
        return Ok(done);
    }
    if let Some(done) = answer(args, &root, &mut connection)? {
        return Ok(done);
    }
    if let [cmd, agent_id] = args
        && cmd == "interrupt"
    {
        let pane = swarm::store::require_pane(&connection, &session_id()?, agent_id)?;
        swarm::adapter::load(&root, &adapter_name())?.run("interrupt", &[("pane", &pane)])?;
        return Ok(());
    }
    if let [cmd, agent_id, key] = args
        && cmd == "key"
    {
        // The app's pull-back of a queued message. Escape and C-c would stop the agent's turn.
        if in_agent_pane() {
            return Err(
                "swarm: only the owner presses keys in an agent's pane; the Swarm app sends them"
                    .into(),
            );
        }
        if !["Up", "C-u"].contains(&key.as_str()) {
            return Err(format!("swarm: key {key:?} is not allowed; only Up and C-u").into());
        }
        let pane = swarm::store::require_pane(&connection, &session_id()?, agent_id)?;
        swarm::adapter::load(&root, &adapter_name())?
            .run("key", &[("pane", &pane), ("key", key)])?;
        return Ok(());
    }
    if let [cmd, agent_id, rest @ ..] = args
        && cmd == "clear-box"
    {
        if in_agent_pane() {
            return Err(
                "swarm: only the owner clears an agent's input box; the Swarm app does it".into(),
            );
        }
        let dequeue = match rest {
            [] => false,
            [flag] if flag == "--dequeue" => true,
            _ => return Err("swarm: usage: swarm clear-box <agent> [--dequeue]".into()),
        };
        let session_id = session_id()?;
        if swarm::store::provider_of(&connection, &session_id, agent_id)?.as_deref() != Some("pi") {
            return Err("swarm: clear-box reads only pi's input box".into());
        }
        let pane = swarm::store::require_pane(&connection, &session_id, agent_id)?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        println!("{}", clear_box(&adapter, &pane, dequeue)?);
        return Ok(());
    }
    let (session_id, agent_id) = identity()?;
    match args {
        [cmd, recipient, kind] if cmd == "send" => {
            let body = std::io::read_to_string(std::io::stdin())?;
            let seq = deliver(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                recipient,
                kind,
                &body,
            )?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "finish" => {
            let summary = std::io::read_to_string(std::io::stdin())?;
            check_wrap_up_reply(&connection, &session_id, &agent_id, &summary)?;
            let orchestrator = swarm::store::orchestrator_of(&connection, &session_id)?;
            let seq = deliver(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                &orchestrator,
                "summary",
                &summary,
            )?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "exited" => {
            // A `run` pane can end before swarm stores it. This runs inside that pane, so the
            // adapter's `self` names it. An id the session does not hold stays an error.
            let pane = match swarm::store::pane_of(&connection, &session_id, &agent_id) {
                Ok(None) => own_pane()?,
                _ => swarm::store::require_pane(&connection, &session_id, &agent_id)?,
            };
            let text =
                swarm::adapter::load(&root, &adapter_name())?.run("capture", &[("pane", &pane)])?;
            let run_dir = root.join(format!("runs/{session_id}"));
            std::fs::create_dir_all(&run_dir)?;
            let log = run_dir.join(format!("{agent_id}.log"));
            swarm::store::write_atomic(&log, &text)?;
            let note = format!(
                "agent {agent_id} exited without a summary; screen log: {}",
                log.display()
            );
            report_dead(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                &note,
            )
        }
        [cmd, rest @ ..] if cmd == "sweep" => {
            let every = match rest {
                [] => None,
                [flag, secs] if flag == "--every" => {
                    Some(secs.parse::<u64>().ok().filter(|s| *s > 0).ok_or(USAGE)?)
                }
                _ => return Err(USAGE.into()),
            };
            if let Some(refusal) = swarm::bus::sweep_refusal(&adapter_name(), every.is_some()) {
                return Err(refusal.into());
            }
            let adapter = swarm::adapter::load(&root, &adapter_name())?;
            loop {
                let mut lines = Vec::new();
                let pass = sweep_once(
                    &mut connection,
                    &root,
                    &adapter,
                    &session_id,
                    &agent_id,
                    &mut lines,
                );
                lines.iter().for_each(|line| println!("{line}"));
                match pass {
                    Ok(()) => {}
                    Err(error) if every.is_some() => eprintln!("swarm: sweep skipped: {error}"),
                    Err(error) => return Err(error),
                }
                let Some(secs) = every else { return Ok(()) };
                std::thread::sleep(std::time::Duration::from_secs(secs));
            }
        }
        [cmd] if cmd == "inbox" => {
            for m in swarm::store::inbox(&connection, &session_id, &agent_id)? {
                println!("{} {} {} {}", m.seq, m.sender_id, m.kind, m.body_path);
            }
            Ok(())
        }
        [cmd, seq] if cmd == "ack" => {
            let seq: i64 = seq.parse().map_err(|_| format!("swarm: bad seq {seq}"))?;
            ack(
                &mut connection,
                &root,
                &adapter_name(),
                &session_id,
                &agent_id,
                seq,
            )
        }
        _ => Err(USAGE.into()),
    }
}
