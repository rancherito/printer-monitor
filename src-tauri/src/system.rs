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
pub fn get_system_info() -> SystemInfo {
    let local_ip = local_ip_address::local_ip()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let os_printers = get_strategy().list_printers();
    let app_printers = build_app_printers();
    let mut printers = os_printers;
    printers.extend(app_printers);

    SystemInfo {
        local_ip,
        port: crate::settings::get_server_port(),
        is_dev: cfg!(debug_assertions),
        printers,
        serial_ports: get_serial_port_list(),
        autostart_enabled: get_autostart_status(),
    }
}

#[tauri::command]
pub fn get_autostart_enabled() -> bool {
    get_autostart_status()
}

#[tauri::command]
pub fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    set_autostart(enabled)
}

#[tauri::command]
pub fn get_server_port() -> u16 {
    crate::settings::get_server_port()
}

#[tauri::command]
pub fn set_server_port(port: u16) -> Result<(), String> {
    crate::settings::set_server_port(port).map_err(|e| e.to_string())
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
            .args(["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "PrinterMonitor"])
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
                       "/v", "PrinterMonitor", "/d", &format!("\"{}\" --autostart", exe_str), "/f"])
                .output()
                .map_err(|e| e.to_string())?;
        } else {
            Command::new("reg")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                       "/v", "PrinterMonitor", "/f"])
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
                "[Desktop Entry]\nType=Application\nName=Printer Monitor\nExec={}\nHidden=false\n",
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
        .join("Library/LaunchAgents/com.codicore.printer-monitor.plist")
}

#[cfg(target_os = "macos")]
fn write_launchagent(path: &std::path::Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.codicore.printer-monitor</string>
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
    std::path::PathBuf::from(home).join(".config/autostart/printer-monitor.desktop")
}

// ─── First-launch detection ──────────────────────────────────────────────────

fn initialized_flag_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("printer-monitor")
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
