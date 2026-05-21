use std::process::Command;
use std::process::Output;
use std::os::windows::process::CommandExt;
use super::{PrinterInfo, PrinterStrategy};

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct WindowsStrategy;

impl PrinterStrategy for WindowsStrategy {
    fn list_printers(&self) -> Vec<PrinterInfo> {
        let script = "Get-Printer | Select-Object Name,PrinterStatus,Default | ConvertTo-Json";
        let Ok(out) = run_ps_output(script) else {
            return vec![];
        };
        parse_powershell_json(&String::from_utf8_lossy(&out.stdout))
    }

    fn install_network(&self, ip: &str, name: &str) -> Result<String, String> {
        let esc_ip   = ps_escape_arg(ip);
        let esc_name = ps_escape_arg(name);
        let script = format!(
            "Add-PrinterPort -Name 'IP_{esc_ip}' -PrinterHostAddress '{esc_ip}'; \
             Add-Printer -Name '{esc_name}' -PortName 'IP_{esc_ip}' -DriverName 'Generic / Text Only'"
        );
        run_ps(&script)
    }

    fn install_usb(&self, port: &str, name: &str) -> Result<String, String> {
        let esc_port = ps_escape_arg(port);
        let esc_name = ps_escape_arg(name);
        let script = format!(
            "Add-Printer -Name '{esc_name}' -PortName '{esc_port}' -DriverName 'Generic / Text Only'"
        );
        run_ps(&script)
    }

    fn test_usb_printer(&self, port: &str, size: &str) -> Result<String, String> {
        let pdf = crate::api_server::generate_test_pdf_bytes(size);
        let escpos = crate::escpos_print::pdf_to_escpos(&pdf, size)?;
        if port.to_ascii_uppercase().starts_with("USB") {
            crate::escpos_print::send_escpos_to_usb_spooler_port(port, &escpos)?;
        } else {
            crate::escpos_print::send_escpos_to_port(port, &escpos)?;
        }
        Ok(format!("PDF de prueba enviado al puerto {port} [{size}]"))
    }

    fn remove_printer(&self, queue_name: &str) -> Result<String, String> {
        let esc = ps_escape_arg(queue_name);
        run_ps(&format!("Remove-Printer -Name '{esc}'"))
    }

    fn rename_printer(&self, queue_name: &str, new_name: &str) -> Result<String, String> {
        let esc_queue   = ps_escape_arg(queue_name);
        let esc_newname = ps_escape_arg(new_name);
        run_ps(&format!("Rename-Printer -Name '{esc_queue}' -NewName '{esc_newname}'"))
    }

    fn print_test(&self, queue_name: &str, size: &str) -> Result<String, String> {
        // Redirigir siempre a la ruta PDF para controlar el ancho de papel.
        crate::api_server::print_internal_test_pdf(queue_name, size)
    }

    fn clear_queue(&self, queue_name: &str) -> Result<String, String> {
        let esc = ps_escape_arg(queue_name);
        run_ps(&format!("Get-PrintJob -PrinterName '{esc}' | Remove-PrintJob"))
    }
}

fn run_ps(script: &str) -> Result<String, String> {
    let out = run_ps_output(script)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn run_ps_output(script: &str) -> Result<Output, String> {
    Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
        .output()
        .map_err(|e| e.to_string())
}

/// Escapa comillas simples y elimina caracteres de control para interpolación
/// segura dentro de strings delimitados por '' en PowerShell.
fn ps_escape_arg(input: &str) -> String {
    input
        .replace('\'', "''")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

fn parse_powershell_json(json: &str) -> Vec<PrinterInfo> {
    let json = json.trim();
    // Normalizar: puede venir como objeto o array
    let arr_json = if json.starts_with('[') {
        json.to_string()
    } else {
        format!("[{json}]")
    };
    let Ok(values) = serde_json::from_str::<serde_json::Value>(&arr_json) else {
        return vec![];
    };
    let Some(arr) = values.as_array() else { return vec![]; };
    arr.iter().filter_map(|v| {
        let name = v["Name"].as_str()?.to_string();
        let is_default = v["Default"].as_bool().unwrap_or(false);
        let status_code = v["PrinterStatus"].as_u64().unwrap_or(0);
        let status = if status_code == 3 { "Disponible" } else { "Ocupada" }.to_string();
        Some(PrinterInfo {
            name: name.clone(),
            queue_name: name,
            is_default,
            status,
            source: "os".to_string(),
            connection_type: "os".to_string(),
            address: None,
        })
    }).collect()
}
