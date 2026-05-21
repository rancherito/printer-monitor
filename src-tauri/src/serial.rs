#[tauri::command]
pub fn get_serial_ports() -> Vec<String> {
    get_serial_port_list()
}

pub fn get_serial_port_list() -> Vec<String> {
    serialport::available_ports()
        .unwrap_or_default()
        .iter()
        .map(|p| p.port_name.clone())
        .collect()
}
