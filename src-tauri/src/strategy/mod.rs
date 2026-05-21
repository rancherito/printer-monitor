use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PrinterInfo {
    pub name: String,
    pub queue_name: String,
    pub is_default: bool,
    pub status: String,
    pub source: String,          // "os" | "app"
    pub connection_type: String, // "os" | "network" | "usb_direct"
    pub address: Option<String>,
}

/// Trait que encapsula todas las operaciones dependientes del SO.
pub trait PrinterStrategy: Send + Sync {
    fn list_printers(&self) -> Vec<PrinterInfo>;
    fn install_network(&self, ip: &str, name: &str) -> Result<String, String>;
    fn install_usb(&self, port: &str, name: &str) -> Result<String, String>;
    fn remove_printer(&self, queue_name: &str) -> Result<String, String>;
    fn rename_printer(&self, queue_name: &str, new_name: &str) -> Result<String, String>;
    fn print_test(&self, queue_name: &str, size: &str) -> Result<String, String>;
    fn clear_queue(&self, queue_name: &str) -> Result<String, String>;
}

pub mod mac;
pub mod windows;
pub mod linux;

/// Selecciona la estrategia concreta según el SO en tiempo de compilación.
pub fn get_strategy() -> Box<dyn PrinterStrategy> {
    #[cfg(target_os = "macos")]
    return Box::new(mac::MacStrategy);
    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsStrategy);
    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxStrategy);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    panic!("Plataforma no soportada");
}
