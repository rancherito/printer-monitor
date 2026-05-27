use std::net::TcpStream;
use std::time::Duration;
use regex::Regex;
use crate::settings::get_custom_printer_aliases;
use crate::serial::get_serial_port_list;

pub enum GuardError {
    Validation(String),
}

impl From<GuardError> for String {
    fn from(e: GuardError) -> Self {
        match e {
            GuardError::Validation(msg) => msg,
        }
    }
}

/// El alias/nombre no puede ser vacío ni solo espacios.
pub fn guard_non_empty_name(name: &str) -> Result<(), GuardError> {
    if name.trim().is_empty() {
        Err(GuardError::Validation("El nombre no puede estar vacío.".into()))
    } else {
        Ok(())
    }
}

/// La cadena debe ser una IPv4 válida.
pub fn guard_valid_ip(ip: &str) -> Result<(), GuardError> {
    let re = Regex::new(r"^\d{1,3}(\.\d{1,3}){3}$").unwrap();
    if re.is_match(ip.trim()) {
        Ok(())
    } else {
        Err(GuardError::Validation(format!("IP inválida: '{ip}'")))
    }
}

/// Intenta conectar TCP a IP:port con timeout de 3 segundos.
pub fn guard_port_reachable(ip: &str, port: u16) -> Result<(), GuardError> {
    let addr = format!("{ip}:{port}");
    TcpStream::connect_timeout(
        &addr.parse().map_err(|_| GuardError::Validation(format!("Dirección inválida: {addr}")))?,
        Duration::from_secs(3),
    )
    .map(|_| ())
    .map_err(|_| GuardError::Validation(format!("No se puede conectar a {addr}")))
}

/// Verifica que la impresora exista en el SO.
pub fn guard_printer_exists_os(queue_name: &str) -> Result<(), GuardError> {
    use crate::strategy::get_strategy;
    let exists = get_strategy()
        .list_printers()
        .iter()
        .any(|p| p.queue_name == queue_name);
    if exists {
        Ok(())
    } else {
        Err(GuardError::Validation(format!("La impresora '{queue_name}' no existe en el SO.")))
    }
}

/// El alias no debe existir ya en custom_printers de SQLite.
pub fn guard_alias_unique(alias: &str) -> Result<(), GuardError> {
    let aliases = get_custom_printer_aliases().unwrap_or_default();
    if aliases.contains(&alias.to_string()) {
        Err(GuardError::Validation(format!("Ya existe una impresora con el alias '{alias}'.")))
    } else {
        Ok(())
    }
}

/// El puerto serie debe existir en la lista de puertos disponibles.
pub fn guard_usb_port_exists(port: &str) -> Result<(), GuardError> {
    // Device interface paths (\\?\USB#...) can be opened directly — always accepted.
    if port.starts_with("\\\\?\\") {
        return Ok(());
    }
    let ports = get_serial_port_list();
    if ports.contains(&port.to_string()) {
        Ok(())
    } else {
        Err(GuardError::Validation(format!("El puerto '{port}' no existe o no está disponible.")))
    }
}
