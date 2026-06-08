use serde::{Deserialize, Serialize};
use crate::strategy::get_strategy;
use crate::settings::get_custom_printers;
use crate::serial::get_serial_port_list;
use crate::strategy::PrinterInfo;

#[derive(Serialize, Deserialize, Debug)]
pub struct SystemInfo {
    pub local_ip: String,
    pub port: u16,
    pub is_dev: bool,
    pub printers: Vec<PrinterInfo>,
    pub serial_ports: Vec<String>,
    pub autostart_enabled: bool,
}

#[tauri::command]
pub async fn get_system_info() -> SystemInfo {
    // Las 4 fuentes se ejecutan en paralelo en el pool de bloqueantes para que
    // el tiempo total sea max(tiempos) en lugar de la suma serial.
    let (local_ip, printers, serial_ports, autostart_enabled) = tokio::join!(
        tokio::task::spawn_blocking(|| {
            local_ip_address::local_ip()
                .map(|a| a.to_string())
                .unwrap_or_else(|_| "127.0.0.1".to_string())
        }),
        tokio::task::spawn_blocking(|| {
            let mut list = get_strategy().list_printers();
            list.extend(build_app_printers());
            list
        }),
        tokio::task::spawn_blocking(get_serial_port_list),
        tokio::task::spawn_blocking(get_autostart_status),
    );

    let local_ip = local_ip.unwrap_or_else(|_| "127.0.0.1".to_string());
    let printers = printers.unwrap_or_default();
    let serial_ports = serial_ports.unwrap_or_default();
    let autostart_enabled = autostart_enabled.unwrap_or(false);

    SystemInfo {
        local_ip,
        port: crate::settings::get_server_port(),
        is_dev: cfg!(debug_assertions),
        printers,
        serial_ports,
        autostart_enabled,
    }
}

#[tauri::command]
pub async fn get_autostart_enabled() -> bool {
    tokio::task::spawn_blocking(get_autostart_status)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || set_autostart(enabled))
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

#[tauri::command]
pub fn get_server_port() -> u16 {
    crate::settings::get_server_port()
}

#[tauri::command]
pub fn set_server_port(port: u16) -> Result<(), String> {
    crate::settings::set_server_port(port).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_output_dir() -> String {
    crate::settings::get_output_dir()
        .to_string_lossy()
        .to_string()
}

#[tauri::command]
pub async fn set_output_dir(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::settings::set_output_dir(&path))
        .await
        .map_err(|e| format!("Join error: {e}"))?
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct PrintedFile {
    pub name: String,
    pub path: String,
    pub size_kb: u64,
    pub modified: u64, // unix timestamp ms
}

#[tauri::command]
pub async fn list_printed_files() -> Vec<PrintedFile> {
    tokio::task::spawn_blocking(list_printed_files_blocking)
        .await
        .unwrap_or_default()
}

fn list_printed_files_blocking() -> Vec<PrintedFile> {
    let dir = crate::settings::get_output_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![]; };
    let mut files: Vec<PrintedFile> = entries
        .flatten()
        .filter(|e| {
            e.path().extension().and_then(|x| x.to_str()) == Some("pdf")
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?
                .duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
            Some(PrintedFile {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path().to_string_lossy().to_string(),
                size_kb: meta.len() / 1024,
                modified,
            })
        })
        .collect();
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    files
}

#[tauri::command]
pub async fn open_output_dir() -> Result<(), String> {
    tokio::task::spawn_blocking(open_output_dir_blocking)
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

fn open_output_dir_blocking() -> Result<(), String> {
    let dir = crate::settings::get_output_dir();
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn build_app_printers() -> Vec<PrinterInfo> {
    get_custom_printers()
        .unwrap_or_default()
        .into_iter()
        .map(|cp| PrinterInfo {
            name: cp.alias.clone(),
            queue_name: cp.alias,
            is_default: false,
            status: "App".to_string(),
            source: "app".to_string(),
            connection_type: cp.connection_type,
            address: Some(cp.address),
        })
        .collect()
}

// ─── Autostart helpers ────────────────────────────────────────────────────────

pub fn get_autostart_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        let plist = get_launchagent_path();
        plist.exists()
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let out = Command::new("reg")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "CentroDeAyudaCodicore"])
            .output();
        out.map(|o| o.status.success()).unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        let path = get_xdg_autostart_path();
        path.exists()
    }
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path = get_launchagent_path();
        if enabled {
            write_launchagent(&path)?;
        } else {
            let _ = std::fs::remove_file(&path);
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy();
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        if enabled {
            Command::new("reg")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["add", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                       "/v", "CentroDeAyudaCodicore", "/d", &format!("\"{}\" --autostart", exe_str), "/f"])
                .output()
                .map_err(|e| e.to_string())?;
        } else {
            Command::new("reg")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                       "/v", "CentroDeAyudaCodicore", "/f"])
                .output()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let path = get_xdg_autostart_path();
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=Centro de Ayuda Codicore\nExec={}\nHidden=false\n",
                exe.display()
            );
            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
        } else {
            let _ = std::fs::remove_file(&path);
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn get_launchagent_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home)
        .join("Library/LaunchAgents/com.codicore.centro-de-ayuda.plist")
}

#[cfg(target_os = "macos")]
fn write_launchagent(path: &std::path::Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.codicore.centro-de-ayuda</string>
  <key>ProgramArguments</key><array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><false/>
</dict>
</plist>"#,
        exe.display()
    );
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn get_xdg_autostart_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".config/autostart/centro-de-ayuda-codicore.desktop")
}

// ─── First-launch detection ──────────────────────────────────────────────────

fn initialized_flag_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("centro-de-ayuda-codicore")
        .join(".initialized")
}

/// Devuelve `true` si es la primera vez que se ejecuta la app (no existe la
/// bandera de inicialización en el directorio de datos del usuario).
pub fn is_first_launch() -> bool {
    !initialized_flag_path().exists()
}

/// Escribe la bandera de primera ejecución para que no vuelva a activarse el
/// autoarranque automáticamente en lanzamientos posteriores.
pub fn mark_initialized() {
    let _ = std::fs::write(initialized_flag_path(), b"1");
}
