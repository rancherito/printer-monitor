#[tauri::command]
pub fn get_serial_ports() -> Vec<String> {
    get_serial_port_list()
}

pub fn get_serial_port_list() -> Vec<String> {
    // COM / USB-to-serial ports (CH340, FTDI, CP210x, etc.)
    let mut ports: Vec<String> = serialport::available_ports()
        .unwrap_or_default()
        .iter()
        .map(|p| p.port_name.clone())
        .collect();

    // Windows: también listar puertos USB del subsistema de impresión (USB001, USB002…)
    #[cfg(target_os = "windows")]
    ports.extend(get_usb_print_ports_windows());

    // Linux: también listar nodos /dev/usb/lp*
    #[cfg(target_os = "linux")]
    ports.extend(get_usb_print_ports_linux());

    ports.sort();
    ports.dedup();
    ports
}

pub fn resolve_usb_port(current_or_saved: &str) -> Option<String> {
    let ports = get_serial_port_list();
    if ports.iter().any(|p| p == current_or_saved) {
        return Some(current_or_saved.to_string());
    }

    if current_or_saved.starts_with("USB") {
        return ports.into_iter().find(|p| p.starts_with("USB"));
    }
    if current_or_saved.to_uppercase().starts_with("COM") {
        return ports.into_iter().find(|p| p.to_uppercase().starts_with("COM"));
    }
    if current_or_saved.starts_with("/dev/usb/lp") {
        return ports.into_iter().find(|p| p.starts_with("/dev/usb/lp"));
    }

    ports.into_iter().next()
}

/// Devuelve los puertos USB del Print Monitor de Windows (USB001, USB002, etc.).
/// Estos puertos son creados automáticamente por Windows al conectar una impresora USB.
#[cfg(target_os = "windows")]
fn get_usb_print_ports_windows() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let out = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "(Get-PrinterPort | Where-Object { $_.Name -like 'USB*' }).Name",
        ])
        .output()
        .ok();
    match out {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && l.starts_with("USB"))
            .collect(),
        _ => Vec::new(),
    }
}

/// Devuelve los nodos de impresora USB disponibles en Linux (/dev/usb/lp0, lp1…).
#[cfg(target_os = "linux")]
fn get_usb_print_ports_linux() -> Vec<String> {
    (0..8)
        .map(|i| format!("/dev/usb/lp{i}"))
        .filter(|p| std::path::Path::new(p).exists())
        .collect()
}
