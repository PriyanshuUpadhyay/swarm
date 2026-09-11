use std::env;

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    swarm::store::open(&swarm::paths::sqlite_db()?)?;

    Ok(())
}

const USAGE: &str = "usage: swarm init | send <recipient> <kind> | inbox | ack <seq>";

fn identity() -> Result<(i64, String), String> {
    let read = |name: &str| env::var(name).map_err(|_| format!("swarm: {name} not set"));
    let session_id = read("SWARM_SESSION_ID")?.parse().map_err(|_| "swarm: bad SWARM_SESSION_ID")?;
    Ok((session_id, read("SWARM_AGENT_ID")?))
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.first().map(String::as_str) == Some("init") {
        return init();
    }
    let (session_id, agent_id) = identity()?;
    let root = swarm::paths::root_dir()?;
    let mut connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
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
