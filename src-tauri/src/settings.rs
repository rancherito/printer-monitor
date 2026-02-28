use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::net::TcpListener;
use tauri::Manager;
use std::path::PathBuf;
use std::sync::OnceLock;

pub static DB_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init_db_path(app: &tauri::AppHandle) {
    let path = app.path()
        .app_data_dir()
        .expect("No se pudo obtener app_data_dir")
        .join("settings.db");
    let _ = DB_PATH.set(path);
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub port_dev: u16,
    pub port_prod: u16,
    pub active_port: u16,
    pub is_dev: bool,
    pub extra: std::collections::HashMap<String, String>,
}

fn db_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("No se pudo obtener app_data_dir")
        .join("settings.db")
}

pub(crate) fn open_db_global() -> Result<Connection, String> {
    if let Some(path) = DB_PATH.get() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("No se pudo crear directorio de datos: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("No se pudo abrir la BD: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS custom_printers (
                alias TEXT PRIMARY KEY,
                connection_type TEXT NOT NULL,
                address TEXT NOT NULL
            );",
        )
        .map_err(|e| format!("No se pudo inicializar la BD: {e}"))?;
        Ok(conn)
    } else {
        Err("DB_PATH no inicializado".to_string())
    }
}

pub(crate) fn open_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    init_db_path(app);
    open_db_global()
}

pub(crate) fn db_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

pub(crate) fn db_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map(|_| ())
    .map_err(|e| format!("Error al guardar configuración: {e}"))
}

fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn find_free_port(start: u16) -> u16 {
    (start..=65535).find(|&p| port_is_free(p)).unwrap_or(start)
}

/// Puerto activo cacheado para que todas las llamadas devuelvan
/// el mismo valor aunque el servidor ya esté vinculado al puerto.
static ACTIVE_PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

pub(crate) fn resolve_port(app: &tauri::AppHandle) -> u16 {
    if let Some(&p) = ACTIVE_PORT.get() {
        return p;
    }
    let port = if cfg!(debug_assertions) {
        find_free_port(9002)
    } else {
        let conn = match open_db(app) {
            Ok(c) => c,
            Err(_) => {
                let p = find_free_port(9003);
                let _ = ACTIVE_PORT.set(p);
                return p;
            }
        };
        let preferred: u16 = db_get(&conn, "port_prod")
            .and_then(|v| v.parse().ok())
            .unwrap_or(9003);
        let active = if port_is_free(preferred) {
            preferred
        } else {
            find_free_port(preferred + 1)
        };
        let _ = db_set(&conn, "port_prod", &active.to_string());
        active
    };
    let _ = ACTIVE_PORT.set(port);
    port
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let is_dev = cfg!(debug_assertions);
    let active_port = resolve_port(&app);
    let conn = open_db(&app)?;
    let port_dev: u16 = 9002;
    let port_prod: u16 = db_get(&conn, "port_prod")
        .and_then(|v| v.parse().ok())
        .unwrap_or(9003);
    let mut extra = std::collections::HashMap::new();
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings WHERE key != 'port_prod'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    for row in rows.flatten() {
        extra.insert(row.0, row.1);
    }
    Ok(AppSettings { port_dev, port_prod, active_port, is_dev, extra })
}

#[tauri::command]
pub fn set_setting(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    if key == "port_dev" {
        return Err("El puerto de desarrollo es fijo (9002) y no se puede modificar".to_string());
    }
    if key == "port_prod" {
        value
            .parse::<u16>()
            .map_err(|_| "El valor de 'port_prod' debe ser un número de puerto válido (1-65535)".to_string())?;
    }
    let conn = open_db(&app)?;
    db_set(&conn, &key, &value)
}

#[tauri::command]
pub fn get_app_port(app: tauri::AppHandle) -> u16 {
    resolve_port(&app)
}
