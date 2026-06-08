//! Cache en memoria con TTL para los listados de impresoras y USB.
//!
//! Reduce la frecuencia de spawns de PowerShell en refreshes consecutivos
//! dentro de la ventana de validez.

use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::serial::UsbDevice;
use crate::strategy::PrinterInfo;

const PRINTER_TTL: Duration = Duration::from_secs(5);
const USB_TTL: Duration = Duration::from_secs(10);

static PRINTER_CACHE: RwLock<Option<(Instant, Vec<PrinterInfo>)>> = RwLock::new(None);
static USB_CACHE: RwLock<Option<(Instant, Vec<UsbDevice>)>> = RwLock::new(None);

/// Lee la cache de impresoras si está vigente, o ejecuta `loader` y la
/// almacena. Devuelve una copia del vector.
pub fn get_or_load_printers<F: FnOnce() -> Vec<PrinterInfo>>(loader: F) -> Vec<PrinterInfo> {
    {
        let guard = PRINTER_CACHE.read();
        if let Some((ts, data)) = guard.as_ref() {
            if ts.elapsed() < PRINTER_TTL {
                return data.clone();
            }
        }
    }
    let data = loader();
    *PRINTER_CACHE.write() = Some((Instant::now(), data.clone()));
    data
}

/// Invalida la cache de impresoras. Llamar después de cualquier mutación
/// (add/remove/rename/clear).
pub fn invalidate_printers() {
    *PRINTER_CACHE.write() = None;
}

/// Lee la cache de USB si está vigente, o ejecuta `loader` y la almacena.
pub fn get_or_load_usb<F: FnOnce() -> Vec<UsbDevice>>(loader: F) -> Vec<UsbDevice> {
    {
        let guard = USB_CACHE.read();
        if let Some((ts, data)) = guard.as_ref() {
            if ts.elapsed() < USB_TTL {
                return data.clone();
            }
        }
    }
    let data = loader();
    *USB_CACHE.write() = Some((Instant::now(), data.clone()));
    data
}

/// Invalida la cache de USB. Llamar tras `add_usb_printer`.
pub fn invalidate_usb() {
    *USB_CACHE.write() = None;
}
