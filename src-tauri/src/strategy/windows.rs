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
        let driver   = find_or_install_generic_driver()?;
        let script = format!(
            "Add-PrinterPort -Name 'IP_{esc_ip}' -PrinterHostAddress '{esc_ip}'; \
             Add-Printer -Name '{esc_name}' -PortName 'IP_{esc_ip}' -DriverName '{driver}'"
        );
        run_ps(&script)
    }

    fn install_usb(&self, port: &str, name: &str) -> Result<String, String> {
        let esc_port = ps_escape_arg(port);
        let esc_name = ps_escape_arg(name);
        let driver   = find_or_install_generic_driver()?;
        let script = format!(
            "Add-Printer -Name '{esc_name}' -PortName '{esc_port}' -DriverName '{driver}'"
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

/// Tries to return a usable raw/generic printer driver name.
/// Priority:
///   1. "Generic / Text Only"  — attempt to install it from the Windows driver store
///   2. Any already-installed driver whose name contains "Generic"
///   3. "Microsoft IPP Class Driver"  (passes raw data reasonably well)
///   4. "Microsoft enhanced Point and Print compatibility driver"
///   5. First installed driver that isn't obviously PDF/XPS/Fax/OneNote
fn find_or_install_generic_driver() -> Result<String, String> {
    const PREFERRED: &str = "Generic / Text Only";

    // --- 1. Is it already installed? ---
    let installed = get_installed_driver_names();
    if installed.iter().any(|d| d == PREFERRED) {
        return Ok(PREFERRED.to_string());
    }

    // --- 2. Try to install from built-in Windows INF files (present on all Win10/11) ---
    // Windows 10/11 ships it as prnge001.inf; older Windows used prngeneric.inf.
    let inf_candidates = [
        r"%windir%\inf\prnge001.inf",
        r"%windir%\inf\prngeneric.inf",
    ];
    for inf in &inf_candidates {
        // Expand the %windir% variable via PowerShell
        let try_install = run_ps_output(&format!(
            r#"$inf = [System.Environment]::ExpandEnvironmentVariables('{inf}'); \
               if (Test-Path $inf) {{ \
                   Add-PrinterDriver -Name '{PREFERRED}' -InfPath $inf -EA Stop 2>&1; \
                   $ok = (Get-PrinterDriver -Name '{PREFERRED}' -EA SilentlyContinue) -ne $null; \
                   Write-Output $ok \
               }} else {{ Write-Output False }}"#
        ));
        if let Ok(out) = try_install {
            if String::from_utf8_lossy(&out.stdout).trim().ends_with("True") {
                return Ok(PREFERRED.to_string());
            }
        }
    }

    // --- 3. Try pnputil to stage the INF, then Add-PrinterDriver ---
    for inf in &inf_candidates {
        let try_pnp = run_ps_output(&format!(
            r#"$inf = [System.Environment]::ExpandEnvironmentVariables('{inf}'); \
               if (Test-Path $inf) {{ \
                   pnputil /add-driver $inf /install 2>&1 | Out-Null; \
                   Add-PrinterDriver -Name '{PREFERRED}' -EA Stop 2>&1; \
                   Write-Output ((Get-PrinterDriver -Name '{PREFERRED}' -EA SilentlyContinue) -ne $null) \
               }} else {{ Write-Output False }}"#
        ));
        if let Ok(out) = try_pnp {
            if String::from_utf8_lossy(&out.stdout).trim().ends_with("True") {
                return Ok(PREFERRED.to_string());
            }
        }
    }

    // --- 3. Any installed driver whose name contains "Generic" ---
    if let Some(d) = installed.iter().find(|d| d.to_ascii_lowercase().contains("generic")) {
        return Ok(d.clone());
    }

    // --- 4. Known Microsoft raw-pass-through drivers ---
    for candidate in &[
        "Microsoft IPP Class Driver",
        "Microsoft enhanced Point and Print compatibility driver",
        "Microsoft Software Printer Driver",
    ] {
        if installed.iter().any(|d| d == candidate) {
            return Ok(candidate.to_string());
        }
    }

    // --- 5. First driver that isn't clearly PDF/XPS/Fax/OneNote ---
    let skip = ["pdf", "xps", "fax", "onenote", "virtual print class"];
    if let Some(d) = installed.iter().find(|d| {
        let lower = d.to_ascii_lowercase();
        !skip.iter().any(|s| lower.contains(s))
    }) {
        return Ok(d.clone());
    }

    Err("No se encontró ningún controlador de impresora compatible. \
         Instala 'Generic / Text Only' desde Configuración → Impresoras → Agregar impresora.".to_string())
}

fn get_installed_driver_names() -> Vec<String> {
    let Ok(out) = run_ps_output(
        "(Get-PrinterDriver -EA SilentlyContinue).Name"
    ) else {
        return vec![];
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
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
