use std::env;

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    swarm::store::open(&swarm::paths::sqlite_db()?)?;

    Ok(())
}

const USAGE: &str = "usage: swarm init | send <recipient> <kind> | inbox | ack <seq>";

fn identity() -> Result<(String, String), String> {
    let read = |name: &str| env::var(name).map_err(|_| format!("swarm: {name} not set"));
    Ok((read("SWARM_SESSION_ID")?, read("SWARM_AGENT_ID")?))
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.first().map(String::as_str) == Some("init") {
        return init();
    }
    let (_session_id, _agent_id) = identity()?;
    Err(USAGE.into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let Some(first_arg) = args.get(1) else {
        eprintln!("usage: swarm init");
        std::process::exit(1);
    };

    if first_arg != "init" {
        eprintln!("Illegal usage of the tool. Only init is available");
        std::process::exit(1);
    }

    init()?;

    Ok(())
}
