use crate::guards::*;
use crate::api_server;
use crate::printer_cache::{get_or_load_printers, invalidate_printers};
use crate::settings::{
    delete_custom_printer, get_custom_printer, get_custom_printers, insert_custom_printer,
    update_custom_printer_address,
};
use crate::serial::resolve_usb_port;
use crate::strategy::{get_strategy, PrinterInfo};

#[tauri::command]
pub async fn get_printers() -> Vec<PrinterInfo> {
    tokio::task::spawn_blocking(load_printers_with_cache)
        .await
        .unwrap_or_default()
}

fn load_printers_with_cache() -> Vec<PrinterInfo> {
    let mut list = get_or_load_printers(|| get_strategy().list_printers());
    // Agregar solo impresoras realmente gestionadas por la app.
    // Las USB modo sistema viven en la lista del SO para evitar duplicidad visual.
    let app_printers = get_custom_printers().unwrap_or_default();
    for cp in app_printers {
        if cp.connection_type == "usb_system" || cp.connection_type == "usb_direct" {
            continue;
        }
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
pub async fn rename_printer(printer_name: String, new_name: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || rename_printer_blocking(&printer_name, &new_name))
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

fn rename_printer_blocking(printer_name: &str, new_name: &str) -> Result<String, String> {
    guard_non_empty_name(new_name).map_err(String::from)?;
    guard_printer_exists_os(printer_name).map_err(String::from)?;
    let res = get_strategy().rename_printer(printer_name, new_name);
    if res.is_ok() {
        invalidate_printers();
    }
    res
}

#[tauri::command]
pub async fn print_test(printer_name: String, size: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || print_test_blocking(&printer_name, &size))
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

fn print_test_blocking(printer_name: &str, size: &str) -> Result<String, String> {
    // Impresoras del SO: se imprimen por cola del sistema.
    if guard_printer_exists_os(printer_name).is_ok() {
        return get_strategy().print_test(printer_name, size);
    }

    // Impresoras registradas en la app: resolver según tipo de conexión.
    let Some(cp) = get_custom_printer(printer_name).map_err(|e| e.to_string())? else {
        return Err(format!("La impresora '{printer_name}' no existe en el SO ni en la app."));
    };

    match cp.connection_type.as_str() {
        "network" => {
            let ip = cp
                .address
                .split(':')
                .next()
                .ok_or_else(|| "Dirección TCP inválida".to_string())?
                .to_string();
            guard_valid_ip(&ip).map_err(String::from)?;
            guard_port_reachable(&ip, 9100).map_err(String::from)?;
            let content = build_test_escpos(size);
            send_escpos_tcp(&ip, 9100, &content)
        }
        "usb_app" => {
            let Some(port) = resolve_usb_port(&cp.address) else {
                return Err("No se encontró ningún puerto USB disponible para esta impresora.".to_string());
            };
            if port != cp.address {
                let _ = update_custom_printer_address(&cp.alias, &port);
            }
            let data = build_test_escpos(size);
            crate::escpos_print::send_escpos_to_port(&port, &data)
        }
        // Compatibilidad con registros anteriores y modo sistema.
        "usb_system" | "usb_direct" => {
            let port = cp.address;
            guard_usb_port_exists(&port).map_err(String::from)?;
            get_strategy().test_usb_printer(&port, size)
        }
        _ => Err(format!("Tipo de conexión no soportado: {}", cp.connection_type)),
    }
}

#[tauri::command]
pub async fn print_test_pdf_internal(printer_name: String, size: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || print_test_pdf_internal_blocking(&printer_name, &size))
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

fn print_test_pdf_internal_blocking(printer_name: &str, size: &str) -> Result<String, String> {
    // Impresoras del SO: ruta GDI/PDFium→PNG→PrintDocument
    if guard_printer_exists_os(printer_name).is_ok() {
        return api_server::print_internal_test_pdf(printer_name, size);
    }

    // Impresoras App: generar PDF de prueba y enviarlo como ESC/POS
    let Some(cp) = get_custom_printer(printer_name).map_err(|e| e.to_string())? else {
        return Err(format!("La impresora '{printer_name}' no existe en el SO ni en la app."));
    };

    if cp.connection_type == "network" || cp.connection_type == "usb_app" {
        let pdf_bytes = api_server::generate_test_pdf_bytes(size);
        return api_server::print_pdf_bytes_job(&pdf_bytes, printer_name, size);
    }

    Err(format!("Tipo de conexión no soportado para PDF: {}", cp.connection_type))
}

#[tauri::command]
pub async fn print_test_a4_pdf(printer_name: String, size: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || print_test_a4_pdf_blocking(&printer_name, &size))
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

fn print_test_a4_pdf_blocking(printer_name: &str, size: &str) -> Result<String, String> {
    let pdf = api_server::generate_a4_test_pdf_bytes();
    api_server::print_pdf_bytes_job(&pdf, printer_name, size)
}

#[tauri::command]
pub async fn print_test_tcp(ip: String, size: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || print_test_tcp_blocking(&ip, &size))
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

fn print_test_tcp_blocking(ip: &str, size: &str) -> Result<String, String> {
    guard_valid_ip(ip).map_err(String::from)?;
    guard_port_reachable(ip, 9100).map_err(String::from)?;
    let pdf = api_server::generate_test_pdf_bytes(size);
    let escpos = crate::escpos_print::pdf_to_escpos(&pdf, size)?;
    send_escpos_tcp(ip, 9100, &escpos)
}

#[tauri::command]
pub async fn add_network_printer(ip: String, name: String) -> Result<String, String> {
    let res = tokio::task::spawn_blocking(move || add_network_printer_blocking(&ip, &name))
        .await
        .map_err(|e| format!("Join error: {e}"))?;
    if res.is_ok() {
        invalidate_printers();
    }
    res
}

fn add_network_printer_blocking(ip: &str, name: &str) -> Result<String, String> {
    guard_non_empty_name(name).map_err(String::from)?;
    guard_valid_ip(ip).map_err(String::from)?;
    guard_port_reachable(ip, 9100).map_err(String::from)?;
    guard_alias_unique(name).map_err(String::from)?;
    let address = format!("{ip}:9100");
    insert_custom_printer(name, "network", &address).map_err(|e| e.to_string())?;
    get_strategy().install_network(ip, name)
}

#[tauri::command]
pub async fn test_usb_printer(port: String, size: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || test_usb_printer_blocking(&port, &size))
        .await
        .map_err(|e| format!("Join error: {e}"))?
}

fn test_usb_printer_blocking(port: &str, size: &str) -> Result<String, String> {
    guard_usb_port_exists(port).map_err(String::from)?;
    get_strategy().test_usb_printer(port, size)
}

#[tauri::command]
pub async fn add_usb_printer(port: String, name: String, mode: String) -> Result<String, String> {
    let res = tokio::task::spawn_blocking(move || add_usb_printer_blocking(&port, &name, &mode))
        .await
        .map_err(|e| format!("Join error: {e}"))?;
    if res.is_ok() {
        invalidate_printers();
        crate::printer_cache::invalidate_usb();
    }
    res
}

fn add_usb_printer_blocking(port: &str, name: &str, mode: &str) -> Result<String, String> {
    guard_non_empty_name(name).map_err(String::from)?;
    guard_usb_port_exists(port).map_err(String::from)?;

    match mode {
        // Modo sistema: instalar cola del SO, no duplicar en custom_printers.
        "system" => get_strategy().install_usb(port, name),
        // Solo app: no instala cola fija, usa resolución de puerto en cada impresión.
        "app" => {
            guard_alias_unique(name).map_err(String::from)?;
            insert_custom_printer(name, "usb_app", port).map_err(|e| e.to_string())?;
            Ok(format!("Impresora USB '{name}' registrada en modo app."))
        }
        _ => Err("Modo USB inválido. Usa 'system' o 'app'.".to_string()),
    }
}

#[tauri::command]
pub async fn clear_print_queue(printer_name: String) -> Result<String, String> {
    let res = tokio::task::spawn_blocking(move || clear_print_queue_blocking(&printer_name))
        .await
        .map_err(|e| format!("Join error: {e}"))?;
    if res.is_ok() {
        invalidate_printers();
    }
    res
}

fn clear_print_queue_blocking(printer_name: &str) -> Result<String, String> {
    guard_printer_exists_os(printer_name).map_err(String::from)?;
    get_strategy().clear_queue(printer_name)
}

#[tauri::command]
pub async fn remove_custom_printer(alias: String) -> Result<String, String> {
    let res = tokio::task::spawn_blocking(move || remove_custom_printer_blocking(&alias))
        .await
        .map_err(|e| format!("Join error: {e}"))?;
    if res.is_ok() {
        invalidate_printers();
    }
    res
}

fn remove_custom_printer_blocking(alias: &str) -> Result<String, String> {
    delete_custom_printer(alias).map_err(|e| e.to_string())?;
    Ok(format!("Impresora '{alias}' eliminada"))
}

// ─── ESC/POS helpers ──────────────────────────────────────────────────────────

fn build_test_escpos(size: &str) -> Vec<u8> {
    let width = match size { "58mm" => 32usize, _ => 48 };
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
