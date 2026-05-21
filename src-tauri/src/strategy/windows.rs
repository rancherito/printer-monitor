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
        let script = format!(
            "Add-PrinterPort -Name 'IP_{ip}' -PrinterHostAddress '{ip}'; \
             Add-Printer -Name '{name}' -PortName 'IP_{ip}' -DriverName 'Generic / Text Only'"
        );
        run_ps(&script)
    }

    fn install_usb(&self, port: &str, name: &str) -> Result<String, String> {
        let script = format!(
            "Add-Printer -Name '{name}' -PortName '{port}' -DriverName 'Generic / Text Only'"
        );
        run_ps(&script)
    }

    fn test_usb_printer(&self, port: &str, size: &str) -> Result<String, String> {
        let width = if size == "58mm" { 32usize } else { 48 };
        let sep = "=".repeat(width);
        let content = format!("{sep}\\n  PRINTER MONITOR - PRUEBA  \\n{sep}\\n\\n\\n");
        // Validar el ciclo completo y propagar errores reales al frontend.
        let script = format!(
            "$ErrorActionPreference='Stop'; \
             $t='__pm_test__'; \
             try {{ \
                 if (-not (Get-PrinterPort -Name '{port}' -ErrorAction SilentlyContinue)) {{ throw 'Puerto no encontrado: {port}' }}; \
                 if (-not (Get-PrinterDriver -Name 'Generic / Text Only' -ErrorAction SilentlyContinue)) {{ throw 'Driver Generic / Text Only no instalado en Windows' }}; \
                 if (Get-Printer -Name $t -ErrorAction SilentlyContinue) {{ Remove-Printer -Name $t -ErrorAction Stop }}; \
                 Add-Printer -Name $t -PortName '{port}' -DriverName 'Generic / Text Only' -ErrorAction Stop | Out-Null; \
                 '{content}' | Out-Printer -Name $t -ErrorAction Stop; \
                 Start-Sleep -Milliseconds 500; \
                 Write-Output 'OK:spooled'; \
             }} finally {{ \
                 if (Get-Printer -Name $t -ErrorAction SilentlyContinue) {{ Remove-Printer -Name $t -ErrorAction SilentlyContinue }} \
             }}"
        );
        run_ps(&script)
    }

    fn remove_printer(&self, queue_name: &str) -> Result<String, String> {
        run_ps(&format!("Remove-Printer -Name '{queue_name}'"))
    }

    fn rename_printer(&self, queue_name: &str, new_name: &str) -> Result<String, String> {
        run_ps(&format!("Rename-Printer -Name '{queue_name}' -NewName '{new_name}'"))
    }

    fn print_test(&self, queue_name: &str, size: &str) -> Result<String, String> {
        let content = format!("{}\nPAGINA DE PRUEBA\n{}\n", "=".repeat(32), "=".repeat(32));
        let _ = size;
        let script = format!(
            "'{content}' | Out-Printer -Name '{queue_name}'"
        );
        run_ps(&script)
    }

    fn clear_queue(&self, queue_name: &str) -> Result<String, String> {
        run_ps(&format!("Get-PrintJob -PrinterName '{queue_name}' | Remove-PrintJob"))
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
