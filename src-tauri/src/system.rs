use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri_plugin_autostart::ManagerExt;

use crate::bluetooth::BluetoothDevice;
use crate::network::NetworkDevice;
use crate::printers::{get_printers, PrinterInfo};
use crate::serial::{list_serial_ports, SerialPort};
use crate::settings::resolve_port;

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub local_ip: String,
    pub port: u16,
    pub is_dev: bool,
    pub printers: Vec<PrinterInfo>,
    pub serial_ports: Vec<SerialPort>,
    pub autostart_enabled: bool,
    pub network_devices: Vec<NetworkDevice>,
    pub bluetooth_devices: Vec<BluetoothDevice>,
}

/// Devuelve la IP local del equipo.
#[tauri::command]
pub fn get_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "No disponible".to_string())
}

/// Devuelve toda la información del sistema en una sola llamada.
#[tauri::command]
pub fn get_system_info(app: tauri::AppHandle) -> SystemInfo {
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let port = resolve_port(&app);
    let is_dev = cfg!(debug_assertions);
    SystemInfo {
        local_ip: get_local_ip(),
        port,
        is_dev,
        printers: get_printers(),
        serial_ports: list_serial_ports(),
        autostart_enabled,
        network_devices: vec![],
        bluetooth_devices: vec![],
    }
}

#[tauri::command]
pub fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Error al verificar autostart: {e}"))
}

#[tauri::command]
pub fn set_autostart_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| format!("Error al activar autostart: {e}"))
    } else {
        manager.disable().map_err(|e| format!("Error al desactivar autostart: {e}"))
    }
}

/// Hilo de fondo que detecta cambios en impresoras y puertos USB/COM
/// cada 2 segundos y emite el evento `printers-updated`.
pub(crate) fn start_printer_watcher(handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut prev_printers = get_printers();
        let mut prev_serial = list_serial_ports();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let printers = get_printers();
            let serial = list_serial_ports();
            if printers != prev_printers || serial != prev_serial {
                let _ = handle.emit(
                    "printers-updated",
                    serde_json::json!({ "printers": printers, "serial_ports": serial }),
                );
                prev_printers = printers;
                prev_serial = serial;
            }
        }
    });
}
