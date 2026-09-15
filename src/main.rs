use std::env;

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

const USAGE: &str = "usage: swarm init | adapter check <name> | session new <talk_mode> | agent add <agent_id> <role> | spawn <agent_id> <role> [-- <cmd>...] | send <recipient> <kind> | finish | exited | sweep | inbox | ack <seq>";

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
    session_id: i64,
    sender: &str,
    recipient: &str,
    kind: &str,
    body: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let seq = swarm::store::send_message(connection, root, session_id, sender, recipient, kind, body)?;
    if let Some(pane) = swarm::store::pane_of(connection, recipient)? {
        let ring = swarm::adapter::load(root, &adapter_name())
            .and_then(|a| a.run("ring", &[("pane", &pane), ("text", "swarm: new message")]));
        if let Err(error) = ring {
            eprintln!("swarm: ring failed: {error}");
        }
    }
    Ok(seq)
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
        deliver(connection, root, session_id, child, orchestrator, "summary", note)?;
        swarm::store::enqueue_job(connection, child, "summarize")?;
    }
    swarm::store::clear_pane(connection, child)
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
        return swarm::store::add_agent(&connection, session_id()?, agent_id, role);
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
        let home = env::var("SWARM_HOME").or_else(|_| env::var("HOME"))?;
        let vars = [("session_id", session.as_str()), ("agent_id", agent_id), ("home", &home), ("adapter", &adapter_name())];
        let pane = adapter.run("spawn", &vars)?;
        swarm::store::set_pane(&connection, agent_id, &pane)?;
        if !command.is_empty() {
            let exe = env::current_exe()?.to_string_lossy().into_owned();
            let hook = swarm::adapter::shell_line(&[exe, "exited".into()]);
            let line = format!("{}; {hook}", swarm::adapter::shell_line(command));
            adapter.run("ring", &[("pane", &pane), ("text", &line)])?;
        }
        println!("{pane}");
        return Ok(());
    }
    let (session_id, agent_id) = identity()?;
    match args {
        [cmd, recipient, kind] if cmd == "send" => {
            let body = std::io::read_to_string(std::io::stdin())?;
            let seq = deliver(&mut connection, &root, session_id, &agent_id, recipient, kind, &body)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "finish" => {
            let summary = std::io::read_to_string(std::io::stdin())?;
            let orchestrator = swarm::store::orchestrator_of(&connection, session_id)?;
            let seq = deliver(&mut connection, &root, session_id, &agent_id, &orchestrator, "summary", &summary)?;
            println!("{seq}");
            Ok(())
        }
        [cmd] if cmd == "exited" => {
            let pane = swarm::store::pane_of(&connection, &agent_id)?.ok_or("swarm: no pane recorded")?;
            let text = swarm::adapter::load(&root, &adapter_name())?.run("capture", &[("pane", &pane)])?;
            let run_dir = root.join(format!("runs/{session_id}"));
            std::fs::create_dir_all(&run_dir)?;
            std::fs::write(run_dir.join(format!("{agent_id}.log")), text)?;
            let orchestrator = swarm::store::orchestrator_of(&connection, session_id)?;
            let note = format!("agent {agent_id} exited without a summary");
            report_dead(&mut connection, &root, session_id, &agent_id, &orchestrator, &note)
        }
        [cmd] if cmd == "sweep" => {
            let adapter = swarm::adapter::load(&root, &adapter_name())?;
            for (child, pane) in swarm::store::live_children(&connection, session_id, &agent_id)? {
                if adapter.has_pane(&pane)? {
                    continue;
                }
                let note = format!("agent {child} died without a summary");
                report_dead(&mut connection, &root, session_id, &child, &agent_id, &note)?;
                println!("dead {child}");
            }
            Ok(())
        }
        [cmd] if cmd == "inbox" => {
            for m in swarm::store::inbox(&connection, session_id, &agent_id)? {
                println!("{} {} {} {}", m.seq, m.sender_id, m.kind, m.body_path);
            }
            Ok(())
        }
        [cmd, seq] if cmd == "ack" => {
            let seq: i64 = seq.parse().map_err(|_| format!("swarm: bad seq {seq}"))?;
            swarm::store::ack(&connection, seq, &agent_id)
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
