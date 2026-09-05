use std::env;

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    swarm::store::open(&swarm::paths::sqlite_db()?)?;

    Ok(())
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
