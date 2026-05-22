use rusqlite::{params, Connection, Result};
use std::sync::Mutex;
use once_cell::sync::Lazy;

static DB: Lazy<Mutex<Connection>> = Lazy::new(|| {
    let conn = open_db().expect("Error abriendo SQLite");
    Mutex::new(conn)
});

fn open_db() -> Result<Connection> {
    let path = dirs_path();
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS custom_printers (
            alias           TEXT PRIMARY KEY,
            connection_type TEXT NOT NULL,
            address         TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS app_config (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;
    Ok(conn)
}

// ─── Configuración del servidor HTTP ─────────────────────────────────────────

pub fn get_server_port() -> u16 {
    let db = DB.lock().unwrap();
    db.query_row(
        "SELECT value FROM app_config WHERE key = 'server_port'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(8001)
}

pub fn set_server_port(port: u16) -> Result<()> {
    let db = DB.lock().unwrap();
    db.execute(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES ('server_port', ?1)",
        params![port.to_string()],
    )?;
    Ok(())
}

fn dirs_path() -> std::path::PathBuf {
    // Usar el directorio de datos del usuario en lugar de temp (que puede limpiarse).
    let base = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("printer-monitor");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("printer_monitor.db")
}

pub fn get_custom_printer_aliases() -> Result<Vec<String>> {
    let db = DB.lock().unwrap();
    let mut stmt = db.prepare("SELECT alias FROM custom_printers")?;
    let aliases = stmt.query_map([], |row| row.get(0))?.collect::<Result<Vec<String>>>()?;
    Ok(aliases)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CustomPrinter {
    pub alias: String,
    pub connection_type: String,
    pub address: String,
}

pub fn get_custom_printer(alias: &str) -> Result<Option<CustomPrinter>> {
    let db = DB.lock().unwrap();
    let mut stmt = db.prepare(
        "SELECT alias, connection_type, address FROM custom_printers WHERE alias = ?1",
    )?;
    let mut rows = stmt.query(params![alias])?;
    if let Some(row) = rows.next()? {
        Ok(Some(CustomPrinter {
            alias: row.get(0)?,
            connection_type: row.get(1)?,
            address: row.get(2)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn get_custom_printers() -> Result<Vec<CustomPrinter>> {
    let db = DB.lock().unwrap();
    let mut stmt = db.prepare("SELECT alias, connection_type, address FROM custom_printers")?;
    let rows = stmt.query_map([], |row| {
        Ok(CustomPrinter {
            alias: row.get(0)?,
            connection_type: row.get(1)?,
            address: row.get(2)?,
        })
    })?;
    rows.collect()
}

pub fn insert_custom_printer(alias: &str, connection_type: &str, address: &str) -> Result<()> {
    let db = DB.lock().unwrap();
    db.execute(
        "INSERT INTO custom_printers (alias, connection_type, address) VALUES (?1, ?2, ?3)",
        params![alias, connection_type, address],
    )?;
    Ok(())
}

pub fn delete_custom_printer(alias: &str) -> Result<()> {
    let db = DB.lock().unwrap();
    db.execute("DELETE FROM custom_printers WHERE alias = ?1", params![alias])?;
    Ok(())
}

pub fn update_custom_printer_address(alias: &str, address: &str) -> Result<()> {
    let db = DB.lock().unwrap();
    db.execute(
        "UPDATE custom_printers SET address = ?1 WHERE alias = ?2",
        params![address, alias],
    )?;
    Ok(())
}
