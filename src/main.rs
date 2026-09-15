use std::env;

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    swarm::store::open(&swarm::paths::sqlite_db()?)?;

    Ok(())
}

const USAGE: &str = "usage: swarm init | adapter check <name> | session new <talk_mode> | agent add <agent_id> <role> | spawn <agent_id> <role> | send <recipient> <kind> | inbox | ack <seq>";

fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("swarm: {name} not set"))
}

fn session_id() -> Result<i64, String> {
    env_var("SWARM_SESSION_ID")?.parse().map_err(|_| "swarm: bad SWARM_SESSION_ID".to_string())
}

fn identity() -> Result<(i64, String), String> {
    Ok((session_id()?, env_var("SWARM_AGENT_ID")?))
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
    if let [cmd, agent_id, role] = args && cmd == "spawn" {
        let session_id = session_id()?;
        swarm::store::add_agent(&connection, session_id, agent_id, role)?;
        let adapter = swarm::adapter::load(&root, &env::var("SWARM_ADAPTER").unwrap_or("tmux".into()))?;
        let session = session_id.to_string();
        let pane = adapter.run("spawn", &[("session_id", &session), ("agent_id", agent_id)])?;
        swarm::store::set_pane(&connection, agent_id, &pane)?;
        println!("{pane}");
        return Ok(());
    }
    let (session_id, agent_id) = identity()?;
    match args {
        [cmd, recipient, kind] if cmd == "send" => {
            let body = std::io::read_to_string(std::io::stdin())?;
            let seq = swarm::store::send_message(
                &mut connection, &root, session_id, &agent_id, recipient, kind, &body,
            )?;
            println!("{seq}");
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
