use std::env;

const RERING_AFTER_SECS: i64 = 60;

/// The ring is typed into the recipient's prompt, so it names the next step. An agent that did
/// not load the swarm skill otherwise takes the bare ring as the whole task and waits.
fn ring_text(root: &std::path::Path) -> String {
    format!("swarm: new message. Run swarm inbox, read each body at {}/<body_path>, then swarm ack <seq>.", root.display())
}

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    let adapters = swarm::paths::root_dir()?.join("adapters");
    std::fs::create_dir_all(&adapters)?;
    let shipped = [("tmux", include_str!("../adapters/tmux.conf")), ("herdr", include_str!("../adapters/herdr.conf"))];
    for (name, text) in shipped {
        let file = adapters.join(format!("{name}.conf"));
        if !file.exists() {
            std::fs::write(file, text)?;
        }
    }
    swarm::store::open(&swarm::paths::sqlite_db()?)?;

    Ok(())
}

const USAGE: &str = "usage: swarm init | adapter check <name> | session new <talk_mode> | agent add <agent_id> <role> | spawn <agent_id> <role> [-- <cmd>...] | close <agent_id> | send <recipient> <kind> | finish | exited | sweep [--every <secs>] | drain | inbox | ack <seq>";

fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("swarm: {name} not set"))
}

fn session_id() -> Result<i64, String> {
    env_var("SWARM_SESSION_ID")?.parse().map_err(|_| "swarm: bad SWARM_SESSION_ID".to_string())
}

fn adapter_name() -> String {
    env::var("SWARM_ADAPTER").unwrap_or("tmux".into())
}

fn identity() -> Result<(i64, String), String> {
    Ok((session_id()?, env_var("SWARM_AGENT_ID")?))
}

/// Store the message, then ring the recipient's pane when it has one. The bell is a hint (R9),
/// so a ring failure only warns.
fn deliver(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: i64,
    sender: &str,
    recipient: &str,
    kind: &str,
    body: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let (recipient, kind) = swarm::store::route(connection, session_id, sender, recipient, kind)?;
    let seq = swarm::store::send_message(connection, root, session_id, sender, &recipient, &kind, body)?;
    if let Some(pane) = swarm::store::pane_of(connection, session_id, &recipient)? {
        if !swarm::store::has_rung_unread(connection, session_id, &recipient)? {
            connection.execute(
                "UPDATE message SET rung_at = unixepoch(), rings = 1 WHERE seq = ?1",
                [seq],
            )?;
            let ring = swarm::adapter::load(root, adapter_name)
                .and_then(|a| a.run("ring", &[("pane", &pane), ("text", &ring_text(root))]));
            if let Err(error) = ring {
                eprintln!("swarm: ring failed: {error}");
            }
        }
    }
    Ok(seq)
}

fn add_agent(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    adapter_name: &str,
    session_id: i64,
    agent_id: &str,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pane = if role == "orchestrator" {
        Some(swarm::adapter::load(root, adapter_name)?.run("self", &[])?)
    } else {
        None
    };
    swarm::store::add_agent(connection, session_id, agent_id, role)?;
    if let Some(pane) = pane {
        if !pane.is_empty() {
            swarm::store::set_pane(connection, session_id, agent_id, &pane)?;
        }
    }
    Ok(())
}

/// A child ended without `swarm finish`: send a fallback summary in its name and queue a
/// summarize job, unless it already sent one (R19). Then forget its pane.
fn report_dead(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session_id: i64,
    child: &str,
    orchestrator: &str,
    note: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !swarm::store::has_summary(connection, session_id, child)? {
        deliver(connection, root, &adapter_name(), session_id, child, orchestrator, "summary", note)?;
        swarm::store::enqueue_job(connection, session_id, child, "summarize")?;
    }
    swarm::store::clear_pane(connection, session_id, child)
}

/// One sweep pass: report each child of `agent_id` whose pane is gone.
fn sweep_once(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    adapter: &swarm::adapter::Adapter,
    session_id: i64,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for (child, pane) in swarm::store::live_children(connection, session_id, agent_id)? {
        if adapter.has_pane(&pane)? {
            if swarm::store::rering_due(connection, session_id, &child, RERING_AFTER_SECS)? {
                connection.execute(
                    "UPDATE message SET rung_at = unixepoch(), rings = rings + 1
                     WHERE session_id = ?1 AND recipient_id = ?2
                       AND seq NOT IN (SELECT message_seq FROM read_mark WHERE agent_id = ?2)",
                    (session_id, &child),
                )?;
                match adapter.run("ring", &[("pane", &pane), ("text", &ring_text(root))]) {
                    Ok(_) => eprintln!("swarm: re-ringed {child}"),
                    Err(error) => eprintln!("swarm: re-ring failed for {child}: {error}"),
                }
            }
            continue;
        }
        let note = format!("agent {child} died without a summary");
        report_dead(connection, root, session_id, &child, agent_id, &note)?;
        println!("dead {child}");
    }
    Ok(())
}

/// Feed the agent's captured log to the summarizer shell command and return its output.
/// The log is removed only after a successful run, so a retry still has its input.
fn summarize_log(log: &std::path::Path, summarizer: &str) -> Result<String, Box<dyn std::error::Error>> {
    let input = std::fs::File::open(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let output = std::process::Command::new("sh").arg("-c").arg(summarizer).stdin(input).output()?;
    if !output.status.success() {
        return Err(format!("summarizer failed: {}", String::from_utf8_lossy(&output.stderr).trim()).into());
    }
    std::fs::remove_file(log)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.first().map(String::as_str) == Some("init") {
        return init();
    }
    let root = swarm::paths::root_dir()?;
    if let [cmd, sub, name] = args && cmd == "adapter" && sub == "check" {
        swarm::adapter::load(&root, name)?;
        println!("ok {name}");
        return Ok(());
    }
    let mut connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    if let [cmd, sub, talk_mode] = args && cmd == "session" && sub == "new" {
        println!("{}", swarm::store::create_session(&connection, talk_mode)?);
        return Ok(());
    }
    if let [cmd, sub, agent_id, role] = args && cmd == "agent" && sub == "add" {
        return add_agent(&connection, &root, &adapter_name(), session_id()?, agent_id, role);
    }
    if let [cmd, agent_id, role, rest @ ..] = args && cmd == "spawn" {
        let command = match rest {
            [] => rest,
            [dash, command @ ..] if dash == "--" => command,
            _ => return Err(USAGE.into()),
        };
        let session_id = session_id()?;
        swarm::store::add_agent(&connection, session_id, agent_id, role)?;
        let adapter = swarm::adapter::load(&root, &adapter_name())?;
        let session = session_id.to_string();
        let home = swarm::paths::home()?;
        let vars = [("session_id", session.as_str()), ("agent_id", agent_id), ("home", &home), ("adapter", &adapter_name())];
        let pane = adapter.run("spawn", &vars)?;
        swarm::store::set_pane(&connection, session_id, agent_id, &pane)?;
        if !command.is_empty() {
            let exe = env::current_exe()?.to_string_lossy().into_owned();
            let hook = swarm::adapter::shell_line(&[exe, "exited".into()]);
            let line = format!("{}; {hook}", swarm::adapter::shell_line(command));
            adapter.run("ring", &[("pane", &pane), ("text", &line)])?;
        }
        println!("{pane}");
        return Ok(());
    }
    if let [cmd] = args && cmd == "drain" {
        let summarizer = env_var("SWARM_SUMMARIZER")?;
        while let Some(job_id) = swarm::store::claim_next(&connection)? {
            let (session, agent, kind, attempts) = swarm::store::job(&connection, job_id)?;
            if kind != "summarize" {
                swarm::store::park_job(&connection, job_id)?;
                println!("parked {job_id} unknown kind {kind}");
                continue;
            }
            let log = root.join(format!("runs/{session}/{agent}.log"));
            match summarize_log(&log, &summarizer) {
                Ok(summary) => {
                    let orchestrator = swarm::store::orchestrator_of(&connection, session)?;
                    deliver(&mut connection, &root, &adapter_name(), session, &agent, &orchestrator, "summary", &summary)?;
                    swarm::store::finish_job(&connection, job_id)?;
                    println!("done {job_id}");
                }
                Err(error) if attempts < 3 => {
                    swarm::store::release_job(&connection, job_id, 30)?;
                    println!("retry {job_id}: {error}");
                }
                Err(error) => {
                    swarm::store::park_job(&connection, job_id)?;
                    println!("parked {job_id}: {error}");
                }
            }
        }
        return Ok(());
    }
    if let [cmd, child] = args && cmd == "close" {
        let session_id = session_id()?;
        let pane = swarm::store::pane_of(&connection, session_id, child)?.ok_or("swarm: no pane recorded")?;
        swarm::adapter::load(&root, &adapter_name())?.run("close", &[("pane", &pane)])?;
        return swarm::store::clear_pane(&connection, session_id, child);
    }
    let (session_id, agent_id) = identity()?;
    match args {
        [cmd, recipient, kind] if cmd == "send" => {
            let body = std::io::read_to_string(std::io::stdin())?;
            let seq = deliver(&mut connection, &root, &adapter_name(), session_id, &agent_id, recipient, kind, &body)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "finish" => {
            let summary = std::io::read_to_string(std::io::stdin())?;
            let orchestrator = swarm::store::orchestrator_of(&connection, session_id)?;
            let seq = deliver(&mut connection, &root, &adapter_name(), session_id, &agent_id, &orchestrator, "summary", &summary)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "exited" => {
            let pane = swarm::store::pane_of(&connection, session_id, &agent_id)?.ok_or("swarm: no pane recorded")?;
            let text = swarm::adapter::load(&root, &adapter_name())?.run("capture", &[("pane", &pane)])?;
            let run_dir = root.join(format!("runs/{session_id}"));
            std::fs::create_dir_all(&run_dir)?;
            swarm::store::write_atomic(&run_dir.join(format!("{agent_id}.log")), &text)?;
            let orchestrator = swarm::store::orchestrator_of(&connection, session_id)?;
            let note = format!("agent {agent_id} exited without a summary");
            report_dead(&mut connection, &root, session_id, &agent_id, &orchestrator, &note)
        }
        [cmd, rest @ ..] if cmd == "sweep" => {
            let every = match rest {
                [] => None,
                [flag, secs] if flag == "--every" => Some(secs.parse::<u64>().ok().filter(|s| *s > 0).ok_or(USAGE)?),
                _ => return Err(USAGE.into()),
            };
            let adapter = swarm::adapter::load(&root, &adapter_name())?;
            loop {
                match sweep_once(&mut connection, &root, &adapter, session_id, &agent_id) {
                    Ok(()) => {}
                    Err(error) if every.is_some() => eprintln!("swarm: sweep skipped: {error}"),
                    Err(error) => return Err(error),
                }
                let Some(secs) = every else { return Ok(()) };
                std::thread::sleep(std::time::Duration::from_secs(secs));
            }
        }
        [cmd] if cmd == "inbox" => {
            for m in swarm::store::inbox(&connection, session_id, &agent_id)? {
                println!("{} {} {} {}", m.seq, m.sender_id, m.kind, m.body_path);
            }
            Ok(())
        }
        [cmd, seq] if cmd == "ack" => {
            let seq: i64 = seq.parse().map_err(|_| format!("swarm: bad seq {seq}"))?;
            swarm::store::ack(&connection, session_id, seq, &agent_id)
        }
        _ => Err(USAGE.into()),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(error) = run(&args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_rerings_a_child_once_for_old_unread_messages() {
        let root = std::env::temp_dir().join(format!("swarm-sweep-test-{}", std::process::id()));
        let ring_log = root.join("rings");
        std::fs::create_dir_all(&root).unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane").unwrap();
        swarm::store::add_agent(&connection, session, "orchestrator", "orchestrator").unwrap();
        swarm::store::add_agent(&connection, session, "child", "coder").unwrap();
        swarm::store::set_pane(&connection, session, "child", "%2").unwrap();
        swarm::store::send_message(&mut connection, &root, session, "orchestrator", "child", "ask", "one").unwrap();
        swarm::store::send_message(&mut connection, &root, session, "orchestrator", "child", "ask", "two").unwrap();
        connection.execute("UPDATE message SET created_at = unixepoch() - 61", []).unwrap();

        let adapter = swarm::adapter::parse(
            "fake",
            &format!("self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = echo %2\nclose = true\ncapture = true\n", ring_log.display()),
        )
        .unwrap();
        sweep_once(&mut connection, &root, &adapter, session, "orchestrator").unwrap();
        sweep_once(&mut connection, &root, &adapter, session, "orchestrator").unwrap();
        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);
        connection.execute("UPDATE message SET rung_at = unixepoch() - 61", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, session, "orchestrator").unwrap();

        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
        connection.execute("UPDATE message SET rung_at = unixepoch() - 61", []).unwrap();
        sweep_once(&mut connection, &root, &adapter, session, "orchestrator").unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
    }

    #[test]
    fn deliver_rings_only_when_no_unread_is_rung() {
        let root = std::env::temp_dir().join(format!("swarm-deliver-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let ring_log = root.join("rings");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            format!(
                "self = true\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
                ring_log.display()
            ),
        )
        .unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane").unwrap();
        swarm::store::add_agent(&connection, session, "orchestrator", "orchestrator").unwrap();
        swarm::store::add_agent(&connection, session, "child", "coder").unwrap();
        swarm::store::set_pane(&connection, session, "child", "%2").unwrap();

        let seq1 = deliver(&mut connection, &root, "fake", session, "orchestrator", "child", "ask", "first").unwrap();
        let seq2 = deliver(&mut connection, &root, "fake", session, "orchestrator", "child", "ask", "second").unwrap();

        let ring = format!("%2:{}\n", ring_text(&root));
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring);

        swarm::store::ack(&connection, session, seq1, "child").unwrap();
        swarm::store::ack(&connection, session, seq2, "child").unwrap();

        deliver(&mut connection, &root, "fake", session, "orchestrator", "child", "ask", "third").unwrap();
        assert_eq!(std::fs::read_to_string(&ring_log).unwrap(), ring.repeat(2));
    }

    #[test]
    fn orchestrator_agent_add_records_self_pane_and_child_finish_rings_it() {
        let root = std::env::temp_dir().join(format!("swarm-chair-test-{}", std::process::id()));
        let adapters = root.join("adapters");
        let ring_log = root.join("rings");
        std::fs::create_dir_all(&adapters).unwrap();
        std::fs::write(
            adapters.join("fake.conf"),
            format!(
                "self = printf '%s' '%9'\nspawn = true\nring = printf '%s\\n' \"$SWARM_PANE:$SWARM_TEXT\" >> '{}'\nlist = true\nclose = true\ncapture = true\n",
                ring_log.display()
            ),
        )
        .unwrap();
        let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let session = swarm::store::create_session(&connection, "lane").unwrap();

        add_agent(&connection, &root, "fake", session, "orchestrator", "orchestrator").unwrap();
        swarm::store::add_agent(&connection, session, "child", "coder").unwrap();
        deliver(&mut connection, &root, "fake", session, "child", "orchestrator", "summary", "done").unwrap();

        assert_eq!(swarm::store::pane_of(&connection, session, "orchestrator").unwrap().as_deref(), Some("%9"));
        assert_eq!(std::fs::read_to_string(ring_log).unwrap(), format!("%9:{}\n", ring_text(&root)));
    }
}
