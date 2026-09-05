const SWARM_DIR: &str = ".swarm";
const SWARM_DB: &str = "swarm.db";

pub fn root_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let swarm_home = std::env::var("SWARM_HOME");
    let home = std::env::var("HOME")?;

    let mut root_path = std::path::PathBuf::new();

    root_path.push(swarm_home.unwrap_or(home.to_string()));
    root_path.push(SWARM_DIR);

    Ok(root_path)
}

pub fn runs_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(root_dir()?.join("runs"))
}

pub fn sqlite_db() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(root_dir()?.join(SWARM_DB))
}
