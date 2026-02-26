use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::net::TcpListener;
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

/// Crea un `Command` sin ventana de consola en Windows (flag CREATE_NO_WINDOW).
/// Evita que aparezcan ventanas CMD al lanzar subprocesos desde la app.
/// En otros SO es equivalente a `std::process::Command::new`.
#[cfg(target_os = "windows")]
fn hidden_cmd(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = std::process::Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SerialPort {
    /// Ruta completa o nombre del puerto: "/dev/cu.usbserial-1420", "COM3"
    pub port_name: String,
    /// Descripción legible (p.ej. nombre del chip o dispositivo)
    pub description: String,
    /// Categoría: "USB-Serial" | "USB-CDC" | "COM"
    pub device_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkDevice {
    pub ip: String,
    pub hostname: Option<String>,
    pub is_reachable: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BluetoothDevice {
    pub name: String,
    pub address: String,
    pub is_connected: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub port_dev: u16,
    pub port_prod: u16,
    pub active_port: u16,
    pub is_dev: bool,
    /// Pares clave/valor adicionales (clave → valor como String)
    pub extra: std::collections::HashMap<String, String>,
}

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

/// Obtiene las impresoras del sistema usando lpstat (macOS/Linux) o wmic (Windows)
#[tauri::command]
fn get_printers() -> Vec<PrinterInfo> {
    let mut printers: Vec<PrinterInfo> = Vec::new();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::process::Command;

        let default_printer = Command::new("lpstat")
            .args(["-d"])
            .output()
            .ok()
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout).to_string();
                out.split(':').last().map(|s| s.trim().to_string())
            })
            .unwrap_or_default();

        if let Ok(output) = Command::new("lpstat").args(["-p"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.starts_with("printer ") {
                    let parts: Vec<&str> = line.splitn(3, ' ').collect();
                    if parts.len() >= 2 {
                        let name = parts[1].to_string();
                        let status = if line.contains("idle") {
                            "Disponible".to_string()
                        } else if line.contains("disabled") {
                            "Deshabilitada".to_string()
                        } else {
                            "Imprimiendo".to_string()
                        };
                        let is_default = name == default_printer;
                        printers.push(PrinterInfo { name, is_default, status });
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = hidden_cmd("wmic")
            .args(["printer", "get", "Name,Default,PrinterStatus", "/format:csv"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(2) {
                let cols: Vec<&str> = line.split(',').collect();
                if cols.len() >= 4 {
                    let is_default = cols[1].trim().eq_ignore_ascii_case("TRUE");
                    let name = cols[2].trim().to_string();
                    let status_code = cols[3].trim();
                    let status = match status_code {
                        "3" => "Disponible".to_string(),
                        "4" => "Imprimiendo".to_string(),
                        "5" => "Calentando".to_string(),
                        _ => "Desconocido".to_string(),
                    };
                    if !name.is_empty() {
                        printers.push(PrinterInfo { name, is_default, status });
                    }
                }
            }
        }
    }

    printers
}

/// Obtiene la IP local de la máquina
#[tauri::command]
fn get_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "No disponible".to_string())
}

// ─── Puertos serie / USB-COM ──────────────────────────────────────────────────

/// Detecta adaptadores USB-serial y puertos COM conectados al sistema
fn list_serial_ports() -> Vec<SerialPort> {
    let mut ports: Vec<SerialPort> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // macOS: USB-serial aparece como /dev/cu.usb*
        // cu.usbserial-* → adaptadores FTDI, CH340, CP2102…
        // cu.usbmodem*   → USB CDC ACM (Arduino, etc.)
        if let Ok(entries) = std::fs::read_dir("/dev") {
            let mut found: Vec<SerialPort> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("cu.usb") {
                        let device_type = if name.to_lowercase().contains("modem") {
                            "USB-CDC"
                        } else {
                            "USB-Serial"
                        };
                        Some(SerialPort {
                            port_name: format!("/dev/{}", name),
                            description: name.clone(),
                            device_type: device_type.to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            found.sort_by(|a, b| a.port_name.cmp(&b.port_name));
            ports.extend(found);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // ttyUSB* = adaptadores USB-serial (CH340, CP2102, FTDI)
        // ttyACM* = USB CDC ACM (Arduino, módems)
        if let Ok(entries) = std::fs::read_dir("/dev") {
            let mut found: Vec<SerialPort> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("ttyUSB") || name.starts_with("ttyACM") {
                        let device_type = if name.starts_with("ttyACM") { "USB-CDC" } else { "USB-Serial" };
                        Some(SerialPort {
                            port_name: format!("/dev/{}", name),
                            description: name.clone(),
                            device_type: device_type.to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            found.sort_by(|a, b| a.port_name.cmp(&b.port_name));
            ports.extend(found);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = hidden_cmd("wmic")
            .args(["path", "Win32_SerialPort", "get", "DeviceID,Description", "/format:csv"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            for line in text.lines().skip(2) {
                // CSV: Node, Description, DeviceID  (WMIC ordena los campos alfabéticamente)
                let cols: Vec<&str> = line.split(',').collect();
                // Buscamos el campo que empiece con "COM"
                let port_name = cols.iter()
                    .map(|s| s.trim())
                    .find(|s| s.starts_with("COM"))
                    .unwrap_or("")
                    .to_string();
                let description = cols.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                if !port_name.is_empty() {
                    let device_type = if description.to_lowercase().contains("usb") {
                        "USB-Serial"
                    } else {
                        "COM"
                    };
                    ports.push(SerialPort {
                        port_name,
                        description,
                        device_type: device_type.to_string(),
                    });
                }
            }
        }
    }

    ports
}

#[tauri::command]
fn get_serial_ports() -> Vec<SerialPort> {
    list_serial_ports()
}

// ─── Configuración SQLite ────────────────────────────────────────────────────

/// Devuelve la ruta al fichero SQLite dentro del directorio de datos de la app
fn db_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("No se pudo obtener app_data_dir")
        .join("settings.db")
}

/// Abre (o crea) la BD y garantiza que la tabla `settings` existe
fn open_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let path = db_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear directorio de datos: {e}"))?;
    }
    let conn = Connection::open(&path)
        .map_err(|e| format!("No se pudo abrir la BD: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("No se pudo inicializar la BD: {e}"))?;
    Ok(conn)
}

/// Lee un valor de la BD; devuelve `None` si la clave no existe
fn db_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// Escribe (o sobreescribe) un par clave/valor en la BD
fn db_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map(|_| ())
    .map_err(|e| format!("Error al guardar configuración: {e}"))
}

/// Comprueba si un puerto TCP está libre en 127.0.0.1
fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Busca el primer puerto libre comenzando desde `start`
fn find_free_port(start: u16) -> u16 {
    (start..=65535)
        .find(|&p| port_is_free(p))
        .unwrap_or(start)
}

/// Devuelve el puerto activo para el modo actual:
/// - Dev : siempre empieza en 9002; si está ocupado busca el siguiente libre.
///          El puerto de desarrollo es fijo y NO se persiste en la BD.
/// - Prod: lee el preferido de la BD (por defecto 9003); si está ocupado
///          busca el siguiente libre y guarda el nuevo valor.
fn resolve_port(app: &tauri::AppHandle) -> u16 {
    if cfg!(debug_assertions) {
        // Dev: fijo en 9002, nunca se almacena
        return find_free_port(9002);
    }

    // Prod: leer de BD y persistir si cambia
    let conn = match open_db(app) {
        Ok(c) => c,
        Err(_) => return find_free_port(9003),
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
}

/// Comando: devuelve la configuración de la app.
/// `port_dev` siempre es 9002 (fijo, no se almacena en BD).
/// `port_prod` se lee de la BD (por defecto 9003).
#[tauri::command]
fn get_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let is_dev = cfg!(debug_assertions);
    let active_port = resolve_port(&app);

    let conn = open_db(&app)?;

    // port_dev es siempre 9002 — no se guarda en SQLite
    let port_dev: u16 = 9002;
    let port_prod: u16 = db_get(&conn, "port_prod")
        .and_then(|v| v.parse().ok())
        .unwrap_or(9003);

    // Pares extra: cualquier clave que no sea port_prod
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

/// Comando: guarda un par clave/valor de configuración en la BD.
/// `port_dev` está bloqueado (es fijo en 9002 y no se persiste).
/// `port_prod` requiere un u16 válido.
#[tauri::command]
fn set_setting(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
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

/// Renombra una impresora en el sistema operativo.
/// macOS / Linux (CUPS): cambia la descripción visible con `lpadmin -p <queue> -D "<nuevo>"`.
/// Windows: usa `Rename-Printer` de PowerShell.
#[tauri::command]
fn rename_printer(printer_name: String, new_name: String) -> Result<String, String> {
    use std::process::Command;

    let name = new_name.trim().to_string();
    if name.is_empty() {
        return Err("El nuevo nombre no puede estar vacío".to_string());
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let result = Command::new("lpadmin")
        .args(["-p", &printer_name, "-D", &name])
        .output()
        .map_err(|e| format!("No se pudo ejecutar lpadmin: {e}"))
        .and_then(|o| {
            if o.status.success() {
                Ok(format!("Impresora renombrada a \u{ab}{}\u{bb}", name))
            } else {
                Err(format!("Error al renombrar: {}", String::from_utf8_lossy(&o.stderr).trim()))
            }
        });

    #[cfg(target_os = "windows")]
    let result = {
        let script = format!(
            "Rename-Printer -Name '{}' -NewName '{}'",
            printer_name.replace('\'', "''"),
            name.replace('\'', "''")
        );
        hidden_cmd("powershell")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
            .output()
            .map_err(|e| format!("No se pudo ejecutar PowerShell: {e}"))
            .and_then(|o| {
                if o.status.success() {
                    Ok(format!("Impresora renombrada a \u{ab}{}\u{bb}", name))
                } else {
                    Err(format!("Error al renombrar: {}", String::from_utf8_lossy(&o.stderr).trim()))
                }
            })
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let result: Result<String, String> = Err("Sistema operativo no soportado".to_string());

    result
}

/// Devuelve el puerto activo según el modo de ejecución
#[tauri::command]
fn get_app_port(app: tauri::AppHandle) -> u16 {
    resolve_port(&app)
}

// ─── Autostart ───────────────────────────────────────────────────────────────

#[tauri::command]
fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Error al verificar autostart: {e}"))
}

#[tauri::command]
fn set_autostart_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| format!("Error al activar autostart: {e}"))
    } else {
        manager.disable().map_err(|e| format!("Error al desactivar autostart: {e}"))
    }
}

// ─── Equipos en red ──────────────────────────────────────────────────────────

/// Escanea la subred local y devuelve los hosts que responden a ping
#[tauri::command]
fn scan_network() -> Vec<NetworkDevice> {
    use std::net::IpAddr;
    use std::process::Command;
    use std::sync::{Arc, Mutex};
    use std::thread;

    let local_ip = local_ip_address::local_ip().ok();
    let base = match local_ip {
        Some(IpAddr::V4(v4)) => {
            let octets = v4.octets();
            format!("{}.{}.{}.", octets[0], octets[1], octets[2])
        }
        _ => return vec![],
    };

    let devices = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for i in 1u8..=254 {
        let base = base.clone();
        let devices = Arc::clone(&devices);
        let handle = thread::spawn(move || {
            let ip = format!("{}{}", base, i);
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            let reachable = Command::new("ping")
                .args(["-c", "1", "-W", "1", &ip])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            #[cfg(target_os = "windows")]
            let reachable = hidden_cmd("ping")
                .args(["-n", "1", "-w", "500", &ip])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if reachable {
                // Intento resolución inversa básica con `host` o `nslookup`
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                let hostname = Command::new("host")
                    .arg(&ip)
                    .output()
                    .ok()
                    .and_then(|o| {
                        let s = String::from_utf8_lossy(&o.stdout).to_string();
                        // "1.1.168.192.in-addr.arpa domain name pointer myhost."
                        s.split("pointer ")
                            .nth(1)
                            .map(|h| h.trim().trim_end_matches('.').to_string())
                    });
                #[cfg(target_os = "windows")]
                let hostname = None;

                devices.lock().unwrap().push(NetworkDevice {
                    ip,
                    hostname,
                    is_reachable: true,
                });
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.join();
    }

    let mut result = devices.lock().unwrap().clone();
    result.sort_by(|a, b| {
        let a_last: u8 = a.ip.split('.').last().and_then(|n| n.parse().ok()).unwrap_or(0);
        let b_last: u8 = b.ip.split('.').last().and_then(|n| n.parse().ok()).unwrap_or(0);
        a_last.cmp(&b_last)
    });
    result
}

// ─── Bluetooth ───────────────────────────────────────────────────────────────

/// Devuelve los dispositivos Bluetooth conocidos/emparejados usando herramientas del SO
#[tauri::command]
fn get_bluetooth_devices() -> Vec<BluetoothDevice> {
    use std::process::Command;

    let mut devices: Vec<BluetoothDevice> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // system_profiler SPBluetoothDataType -json (disponible en macOS 12+)
        if let Ok(output) = Command::new("system_profiler")
            .args(["SPBluetoothDataType", "-json"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(bt_arr) = json
                    .get("SPBluetoothDataType")
                    .and_then(|v| v.as_array())
                {
                    for bt_entry in bt_arr {
                        // Dispositivos en "device_connected" y "device_not_connected"
                        for (key, connected) in [
                            ("device_connected", true),
                            ("device_not_connected", false),
                        ] {
                            if let Some(list) = bt_entry.get(key).and_then(|v| v.as_array()) {
                                for dev in list {
                                    if let Some(obj) = dev.as_object() {
                                        for (name, info) in obj {
                                            let address = info
                                                .get("device_address")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("—")
                                                .to_string();
                                            devices.push(BluetoothDevice {
                                                name: name.clone(),
                                                address,
                                                is_connected: connected,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "Get-PnpDevice -Class Bluetooth | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Csv -NoTypeInformation",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            for line in text.lines().skip(1) {
                let cols: Vec<&str> = line.splitn(3, ',').collect();
                if cols.len() >= 3 {
                    let name = cols[0].trim_matches('"').to_string();
                    let address = cols[1].trim_matches('"').to_string();
                    let status = cols[2].trim_matches('"');
                    let is_connected = status.eq_ignore_ascii_case("OK");
                    if !name.is_empty() {
                        devices.push(BluetoothDevice { name, address, is_connected });
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("bluetoothctl").args(["devices"]).output() {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            for line in text.lines() {
                // "Device AA:BB:CC:DD:EE:FF DeviceName"
                let parts: Vec<&str> = line.splitn(3, ' ').collect();
                if parts.len() == 3 && parts[0] == "Device" {
                    let address = parts[1].to_string();
                    let name = parts[2].to_string();
                    // Comprobamos si está conectado
                    let is_connected = Command::new("bluetoothctl")
                        .args(["info", &address])
                        .output()
                        .map(|o| {
                            String::from_utf8_lossy(&o.stdout).contains("Connected: yes")
                        })
                        .unwrap_or(false);
                    devices.push(BluetoothDevice { name, address, is_connected });
                }
            }
        }
    }

    devices
}

// ─── System Info ─────────────────────────────────────────────────────────────

/// Devuelve toda la info del sistema en un solo comando
#[tauri::command]
fn get_system_info(app: tauri::AppHandle) -> SystemInfo {
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
        network_devices: vec![],   // se carga bajo demanda desde la UI
        bluetooth_devices: vec![], // se carga bajo demanda desde la UI
    }
}

/// Genera un PDF de prueba en memoria y lo imprime con `lp` (macOS/Linux) o `print` (Windows)
#[tauri::command]
fn print_test(printer_name: String, size: String) -> Result<String, String> {
    use std::io::Write;
    use std::process::Command;

    // Dimensiones en puntos PDF (1 pt = 1/72 inch)
    // A4: 595 x 842 pt
    // Térmica 50mm: 142 x 200 pt (~50mm ancho, recibo corto)
    // Térmica 80mm: 227 x 200 pt (~80mm ancho, recibo corto)
    let (page_width, page_height, label) = match size.as_str() {
        "thermal_50mm" => (142u32, 200u32, "Térmica 50mm"),
        "thermal_80mm" => (227u32, 200u32, "Térmica 80mm"),
        _ => (595u32, 842u32, "A4"),
    };

    // Genera un PDF mínimo válido en memoria
    let title = format!("Página de prueba — {}", label);
    let body_lines = vec![
        format!("Printer Monitor — Prueba de impresión"),
        format!("Impresora: {}", printer_name),
        format!("Formato:   {}", label),
        format!("Fecha:     {}", chrono::Local::now().format("%d/%m/%Y %H:%M:%S")),
        String::from(""),
        String::from("Si ves este texto, la impresora"),
        String::from("funciona correctamente. ✓"),
    ];

    let pdf_bytes = build_test_pdf(page_width, page_height, &title, &body_lines);

    // Escribe el PDF en un archivo temporal
    let tmp_path = std::env::temp_dir().join(format!("pm_test_{}.pdf", printer_name.replace(' ', "_")));
    {
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("No se pudo crear archivo temporal: {e}"))?;
        f.write_all(&pdf_bytes)
            .map_err(|e| format!("No se pudo escribir PDF: {e}"))?;
    }

    // Envía a la impresora
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let output = Command::new("lp")
            .args(["-d", &printer_name, tmp_path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| format!("Error al ejecutar lp: {e}"))?;

        let _ = std::fs::remove_file(&tmp_path);

        if output.status.success() {
            Ok(format!("Trabajo enviado a «{}» ({})", printer_name, label))
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            Err(format!("Error de impresión: {}", err.trim()))
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = hidden_cmd("cmd")
            .args(["/C", "print", &format!("/D:{}", printer_name), tmp_path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| format!("Error al ejecutar print: {e}"))?;

        let _ = std::fs::remove_file(&tmp_path);

        if output.status.success() {
            Ok(format!("Trabajo enviado a «{}» ({})", printer_name, label))
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            Err(format!("Error de impresión: {}", err.trim()))
        }
    }
}

/// Construye un PDF mínimo válido (PDF 1.4) sin dependencias externas
fn build_test_pdf(width: u32, height: u32, title: &str, lines: &[String]) -> Vec<u8> {
    // Construimos el PDF manualmente siguiendo la especificación mínima
    let mut objects: Vec<String> = Vec::new();

    // Objeto 1: Catálogo
    objects.push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj".to_string());

    // Objeto 2: Pages
    objects.push("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj".to_string());

    // Construir contenido de la página
    let font_size_title = if width < 200 { 9 } else { 14 };
    let font_size_body  = if width < 200 { 7 } else { 11 };
    let margin          = if width < 200 { 8.0 } else { 50.0 };
    let line_height     = (font_size_body as f32) * 1.6;
    let start_y         = (height as f32) - margin - (font_size_title as f32) - 10.0;

    let mut stream = String::new();
    stream.push_str("BT\n");
    // Título
    stream.push_str(&format!("/F1 {} Tf\n", font_size_title));
    stream.push_str(&format!("{} {} Td\n", margin, start_y));
    stream.push_str(&format!("({}) Tj\n", escape_pdf_string(title)));
    // Línea separadora (guiones)
    let sep_count = ((width as f32 - margin * 2.0) / (font_size_body as f32 * 0.5)) as usize;
    let separator = "-".repeat(sep_count.min(60));
    stream.push_str(&format!("/F1 {} Tf\n", font_size_body));
    stream.push_str(&format!("0 -{} Td\n", line_height * 1.2));
    stream.push_str(&format!("({}) Tj\n", separator));
    // Líneas de cuerpo
    for line in lines {
        stream.push_str(&format!("0 -{} Td\n", line_height));
        stream.push_str(&format!("({}) Tj\n", escape_pdf_string(line)));
    }
    stream.push_str("ET\n");

    let stream_bytes = stream.as_bytes().len();

    // Objeto 4: Contenido de la página
    let content_obj = format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj",
        stream_bytes, stream
    );
    objects.push(content_obj);

    // Objeto 5: Fuente
    objects.push(
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj"
            .to_string(),
    );

    // Objeto 3: Page (usa recursos y contenido)
    let page_obj = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj",
        width, height
    );
    // Insertamos Page en posición correcta (índice 2)
    objects.insert(2, page_obj);

    // Ensamblamos el PDF
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");

    let mut offsets: Vec<usize> = Vec::new();
    for obj in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(obj.as_bytes());
        pdf.push(b'\n');
    }

    // xref
    let xref_offset = pdf.len();
    let xref_count = objects.len() + 1;
    let mut xref = format!("xref\n0 {}\n", xref_count);
    xref.push_str("0000000000 65535 f \n");
    for &off in &offsets {
        xref.push_str(&format!("{:010} 00000 n \n", off));
    }
    pdf.extend_from_slice(xref.as_bytes());

    // trailer
    let trailer = format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        xref_count, xref_offset
    );
    pdf.extend_from_slice(trailer.as_bytes());

    pdf
}

fn escape_pdf_string(s: &str) -> String {
    // Convierte caracteres UTF-8 a ASCII con escape básico para PDF strings
    s.chars()
        .map(|c| match c {
            '(' => r"\(".to_string(),
            ')' => r"\)".to_string(),
            '\\' => r"\\".to_string(),
            c if c.is_ascii() => c.to_string(),
            // Para caracteres no-ASCII, usa '?' como fallback
            _ => "?".to_string(),
        })
        .collect()
}

// ─── Watcher de impresoras y puertos USB/COM ──────────────────────────────────

/// Lanza un hilo de fondo que detecta cambios en la lista de impresoras y
/// puertos USB/COM cada 2 segundos y emite el evento `printers-updated`.
/// El estado inicial lo sirve `get_system_info`; el watcher solo emite
/// cuando detecta una diferencia respecto al snapshot anterior.
fn start_printer_watcher(handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        // Inicializar snapshot con el estado actual para no emitir en el arranque
        let mut prev_printers = get_printers();
        let mut prev_serial = list_serial_ports();

        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));

            let printers = get_printers();
            let serial = list_serial_ports();

            if printers != prev_printers || serial != prev_serial {
                let _ = handle.emit(
                    "printers-updated",
                    serde_json::json!({
                        "printers": printers,
                        "serial_ports": serial,
                    }),
                );
                prev_printers = printers;
                prev_serial = serial;
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            start_printer_watcher(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_printers,
            get_local_ip,
            get_app_port,
            get_system_info,
            get_settings,
            set_setting,
            rename_printer,
            print_test,
            get_autostart_enabled,
            set_autostart_enabled,
            scan_network,
            get_bluetooth_devices,
            get_serial_ports,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
