use crate::guards::*;
use crate::settings::{
    delete_custom_printer, get_custom_printers, insert_custom_printer,
};
use crate::strategy::{get_strategy, PrinterInfo};

#[tauri::command]
pub fn get_printers() -> Vec<PrinterInfo> {
    let mut list = get_strategy().list_printers();
    // Agregar impresoras registradas por la app
    let app_printers = get_custom_printers().unwrap_or_default();
    for cp in app_printers {
        list.push(PrinterInfo {
            name: cp.alias.clone(),
            queue_name: cp.alias,
            is_default: false,
            status: "App".to_string(),
            source: "app".to_string(),
            connection_type: cp.connection_type,
            address: Some(cp.address),
        });
    }
    list
}

#[tauri::command]
pub fn rename_printer(printer_name: String, new_name: String) -> Result<String, String> {
    guard_non_empty_name(&new_name).map_err(String::from)?;
    guard_printer_exists_os(&printer_name).map_err(String::from)?;
    get_strategy().rename_printer(&printer_name, &new_name)
}

#[tauri::command]
pub fn print_test(printer_name: String, size: String) -> Result<String, String> {
    guard_printer_exists_os(&printer_name).map_err(String::from)?;
    get_strategy().print_test(&printer_name, &size)
}

#[tauri::command]
pub fn print_test_tcp(ip: String, size: String) -> Result<String, String> {
    guard_valid_ip(&ip).map_err(String::from)?;
    guard_port_reachable(&ip, 9100).map_err(String::from)?;
    let content = build_test_escpos(&size);
    send_escpos_tcp(&ip, 9100, &content)
}

#[tauri::command]
pub fn add_network_printer(ip: String, name: String) -> Result<String, String> {
    guard_non_empty_name(&name).map_err(String::from)?;
    guard_valid_ip(&ip).map_err(String::from)?;
    guard_port_reachable(&ip, 9100).map_err(String::from)?;
    guard_alias_unique(&name).map_err(String::from)?;
    let address = format!("{ip}:9100");
    insert_custom_printer(&name, "network", &address).map_err(|e| e.to_string())?;
    get_strategy().install_network(&ip, &name)
}

#[tauri::command]
pub fn add_usb_printer(port: String, name: String) -> Result<String, String> {
    guard_non_empty_name(&name).map_err(String::from)?;
    guard_usb_port_exists(&port).map_err(String::from)?;
    guard_alias_unique(&name).map_err(String::from)?;
    insert_custom_printer(&name, "usb_direct", &port).map_err(|e| e.to_string())?;
    get_strategy().install_usb(&port, &name)
}

#[tauri::command]
pub fn clear_print_queue(printer_name: String) -> Result<String, String> {
    guard_printer_exists_os(&printer_name).map_err(String::from)?;
    get_strategy().clear_queue(&printer_name)
}

#[tauri::command]
pub fn remove_custom_printer(alias: String) -> Result<String, String> {
    delete_custom_printer(&alias).map_err(|e| e.to_string())?;
    Ok(format!("Impresora '{alias}' eliminada"))
}

// ─── ESC/POS helpers ──────────────────────────────────────────────────────────

fn build_test_escpos(size: &str) -> Vec<u8> {
    let width = if size == "58mm" { 32usize } else { 48 };
    let mut data = Vec::new();
    data.extend_from_slice(b"\x1b@"); // init
    data.extend_from_slice("=".repeat(width).as_bytes());
    data.extend_from_slice(b"\n  PAGINA DE PRUEBA\n");
    data.extend_from_slice("=".repeat(width).as_bytes());
    data.extend_from_slice(b"\n\x1dVB"); // cut
    data
}

fn send_escpos_tcp(ip: &str, port: u16, data: &[u8]) -> Result<String, String> {
    use std::io::Write;
    use std::net::TcpStream;
    use std::time::Duration;
    let addr = format!("{ip}:{port}");
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|_| "Dirección inválida".to_string())?,
        Duration::from_secs(5),
    )
    .map_err(|e| e.to_string())?;
    stream.write_all(data).map_err(|e| e.to_string())?;
    Ok(format!("Datos enviados a {addr}"))
}
