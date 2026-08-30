use std::env;

const SWARM_DIR: &str = ".swarm";

fn init() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;

    let swarm_home_env: Result<String, env::VarError> = std::env::var("SWARM_HOME");

    let swarm_home: &str = &swarm_home_env.unwrap_or(home);

    let mut swarm_directory = std::path::PathBuf::new();

    swarm_directory.push(swarm_home);
    swarm_directory.push(SWARM_DIR);
    swarm_directory.push("runs");

    println!("{}", swarm_directory.display());

    std::fs::create_dir_all(&swarm_directory)?;

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

    println!("yay init");
    init()?;

    Ok(())
}
