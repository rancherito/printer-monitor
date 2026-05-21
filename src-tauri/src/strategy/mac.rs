use std::process::Command;
use super::{PrinterInfo, PrinterStrategy};

pub struct MacStrategy;

impl PrinterStrategy for MacStrategy {
    fn list_printers(&self) -> Vec<PrinterInfo> {
        let Ok(out) = Command::new("lpstat").args(["-p", "-d"]).output() else {
            return vec![];
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        parse_lpstat_output(&stdout)
    }

    fn install_network(&self, ip: &str, name: &str) -> Result<String, String> {
        let uri = format!("socket://{ip}:9100");
        let out = Command::new("lpadmin")
            .args(["-p", name, "-E", "-v", &uri, "-m", "drv:///sample.drv/generic.ppd"])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(format!("Impresora '{name}' instalada en {ip}:9100"))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    fn install_usb(&self, port: &str, name: &str) -> Result<String, String> {
        let uri = format!("usb://{port}");
        let out = Command::new("lpadmin")
            .args(["-p", name, "-E", "-v", &uri, "-m", "drv:///sample.drv/generic.ppd"])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(format!("Impresora USB '{name}' instalada en {port}"))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    fn remove_printer(&self, queue_name: &str) -> Result<String, String> {
        let out = Command::new("lpadmin")
            .args(["-x", queue_name])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(format!("Impresora '{queue_name}' eliminada"))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    fn rename_printer(&self, queue_name: &str, new_name: &str) -> Result<String, String> {
        let out = Command::new("lpadmin")
            .args(["-p", queue_name, "-D", new_name])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(format!("Impresora renombrada a '{new_name}'"))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    fn print_test(&self, queue_name: &str, size: &str) -> Result<String, String> {
        let content = test_page_escpos(size);
        let out = Command::new("lp")
            .args(["-d", queue_name, "-"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                if let Some(stdin) = c.stdin.as_mut() { let _ = stdin.write_all(content.as_bytes()); }
                c.wait_with_output()
            })
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(format!("Prueba enviada a '{queue_name}'"))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    fn clear_queue(&self, queue_name: &str) -> Result<String, String> {
        let out = Command::new("cancel")
            .args(["-a", queue_name])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(format!("Cola de '{queue_name}' limpiada"))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }
}

fn parse_lpstat_output(stdout: &str) -> Vec<PrinterInfo> {
    let mut printers = Vec::new();
    for line in stdout.lines() {
        if line.starts_with("printer ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[1].to_string();
                let status = if line.contains("idle") { "Disponible" } else { "Ocupada" };
                printers.push(PrinterInfo {
                    name: name.clone(),
                    queue_name: name,
                    is_default: false,
                    status: status.to_string(),
                    source: "os".to_string(),
                    connection_type: "os".to_string(),
                    address: None,
                });
            }
        }
    }
    printers
}

fn test_page_escpos(size: &str) -> String {
    let width = if size == "58mm" { 32 } else { 48 };
    format!(
        "\x1b@\x1b!0{}\n{}\n\x1dVB",
        "=".repeat(width),
        "  PAGINA DE PRUEBA"
    )
}
