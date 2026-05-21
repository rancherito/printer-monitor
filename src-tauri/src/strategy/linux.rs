use std::process::Command;
use super::{PrinterInfo, PrinterStrategy};

/// Estrategia Linux (Ubuntu 22.04+) — usa CUPS/lpadmin igual que macOS.
pub struct LinuxStrategy;

impl PrinterStrategy for LinuxStrategy {
    fn list_printers(&self) -> Vec<PrinterInfo> {
        let Ok(out) = Command::new("lpstat").args(["-p", "-d"]).output() else {
            return vec![];
        };
        parse_lpstat(&String::from_utf8_lossy(&out.stdout))
    }

    fn install_network(&self, ip: &str, name: &str) -> Result<String, String> {
        let uri = format!("socket://{ip}:9100");
        run_lpadmin(&["-p", name, "-E", "-v", &uri, "-m", "drv:///sample.drv/generic.ppd"])
    }

    fn install_usb(&self, port: &str, name: &str) -> Result<String, String> {
        let uri = format!("usb://{port}");
        run_lpadmin(&["-p", name, "-E", "-v", &uri, "-m", "drv:///sample.drv/generic.ppd"])
    }

    fn remove_printer(&self, queue_name: &str) -> Result<String, String> {
        run_lpadmin(&["-x", queue_name])
    }

    fn rename_printer(&self, queue_name: &str, new_name: &str) -> Result<String, String> {
        run_lpadmin(&["-p", queue_name, "-D", new_name])
    }

    fn print_test(&self, queue_name: &str, size: &str) -> Result<String, String> {
        let width = if size == "58mm" { 32usize } else { 48 };
        let content = format!("\x1b@{}\nPAGINA DE PRUEBA\n\x1dVB", "=".repeat(width));
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
            Ok(format!("Cola limpiada"))
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }
}

fn run_lpadmin(args: &[&str]) -> Result<String, String> {
    let out = Command::new("lpadmin")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn parse_lpstat(stdout: &str) -> Vec<PrinterInfo> {
    stdout.lines()
        .filter(|l| l.starts_with("printer "))
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            parts.get(1).map(|name| {
                let status = if l.contains("idle") { "Disponible" } else { "Ocupada" };
                PrinterInfo {
                    name: name.to_string(),
                    queue_name: name.to_string(),
                    is_default: false,
                    status: status.to_string(),
                    source: "os".to_string(),
                    connection_type: "os".to_string(),
                    address: None,
                }
            })
        })
        .collect()
}
