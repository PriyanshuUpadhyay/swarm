const SWARM_DIR: &str = ".swarm";
const SWARM_DB: &str = "swarm.db";

/// SWARM_HOME when set, else HOME.
pub fn home() -> Result<String, Box<dyn std::error::Error>> {
    Ok(std::env::var("SWARM_HOME").or_else(|_| std::env::var("HOME"))?)
}

pub fn root_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(std::path::PathBuf::from(home()?).join(SWARM_DIR))
}

pub fn runs_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(root_dir()?.join("runs"))
}

pub fn sqlite_db() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(root_dir()?.join(SWARM_DB))
}
