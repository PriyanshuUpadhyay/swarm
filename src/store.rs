use std::path::Path;

pub fn open(path: &Path) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let connection = rusqlite::Connection::open(path)?;

    connection.execute_batch("PRAGMA journal_mode=WAL;")?;

    Ok(connection)
}
