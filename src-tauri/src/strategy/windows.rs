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
        let esc_port   = ps_escape_arg(port);
        let esc_name   = ps_escape_arg(name);
        let driver     = find_or_install_generic_driver()?;
        let esc_driver = ps_escape_arg(&driver);
        // Idempotent: if a queue with this name already exists on this exact port,
        // remove it first so re-registration works cleanly without duplicates.
        // If it exists on a different port, Add-Printer returns a clear Windows error.
        let script = format!(
            "$p = Get-Printer -Name '{esc_name}' -EA SilentlyContinue; \
             if ($p -and $p.PortName -eq '{esc_port}') {{ Remove-Printer -Name '{esc_name}' -EA SilentlyContinue }}; \
             Add-Printer -Name '{esc_name}' -PortName '{esc_port}' -DriverName '{esc_driver}' -EA Stop"
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

/// Finds or installs a raw/passthrough printer driver suitable for ESC/POS.
/// Works on all Windows 10 / Windows 11 machines without internet access.
///
/// Strategy:
///   1. "Generic / Text Only" already installed → use it immediately.
///   2. Install from the Windows DriverStore (FileRepository) — the INF *and* its
///      binary files are always present there on Win10/11. This is the reliable path
///      that works even on factory-fresh machines with no printers ever installed.
///   3. Fallback to %windir%\inf\prnge001.inf / prngeneric.inf.
///   4. Any already-installed driver containing "Generic".
///   5. Known Microsoft pass-through drivers.
///   6. First non-PDF/XPS/Fax/OneNote driver found.
pub(crate) fn find_or_install_generic_driver() -> Result<String, String> {
    const PREFERRED: &str = "Generic / Text Only";

    // --- 1. Already installed? ---
    let installed = get_installed_driver_names();
    if installed.iter().any(|d| d == PREFERRED) {
        return Ok(PREFERRED.to_string());
    }

    // --- 2 + 3. Install from DriverStore then %windir%\inf ---
    // IMPORTANT: backtick (`) is the PowerShell line-continuation character.
    // This script contains NO backslash continuations so it runs correctly on
    // every PowerShell 5.1 / 7.x version shipped with Windows 10 and 11.
    let script = r#"$name = 'Generic / Text Only'
$dsInf = Get-Item "$env:windir\System32\DriverStore\FileRepository\prnge001.inf_*\prnge001.inf" `
    -EA SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
$candidates = [System.Collections.ArrayList]@()
if ($dsInf) { [void]$candidates.Add($dsInf.FullName) }
[void]$candidates.Add("$env:windir\inf\prnge001.inf")
[void]$candidates.Add("$env:windir\inf\prngeneric.inf")
foreach ($inf in $candidates) {
    if (-not (Test-Path $inf -EA SilentlyContinue)) { continue }
    try { Add-PrinterDriver -Name $name -InfPath $inf -EA Stop 2>&1 | Out-Null } catch {}
    if (Get-PrinterDriver -Name $name -EA SilentlyContinue) { Write-Output 'OK'; exit }
    try {
        pnputil /add-driver "$inf" /install 2>&1 | Out-Null
        Add-PrinterDriver -Name $name -EA Stop 2>&1 | Out-Null
    } catch {}
    if (Get-PrinterDriver -Name $name -EA SilentlyContinue) { Write-Output 'OK'; exit }
}
Write-Output 'FAIL'"#;

    if let Ok(out) = run_ps_output(script) {
        if String::from_utf8_lossy(&out.stdout).contains("OK") {
            return Ok(PREFERRED.to_string());
        }
    }

    // Re-read after install attempt (driver may now be present)
    let installed = get_installed_driver_names();

    // --- 4. Any installed driver containing "Generic" ---
    if let Some(d) = installed.iter().find(|d| d.to_ascii_lowercase().contains("generic")) {
        return Ok(d.clone());
    }

    // --- 5. Known Microsoft pass-through drivers ---
    for candidate in &[
        "Microsoft IPP Class Driver",
        "Microsoft enhanced Point and Print compatibility driver",
        "Microsoft Software Printer Driver",
    ] {
        if installed.iter().any(|d| d == candidate) {
            return Ok(candidate.to_string());
        }
    }

    // --- 6. First driver that isn't clearly PDF/XPS/Fax/OneNote ---
    let skip = ["pdf", "xps", "fax", "onenote", "virtual print class"];
    if let Some(d) = installed.iter().find(|d| {
        let lower = d.to_ascii_lowercase();
        !skip.iter().any(|s| lower.contains(s))
    }) {
        return Ok(d.clone());
    }

    Err("No se encontró ningún controlador de impresora compatible. \
         Verifica que el servicio de cola de impresión (Spooler) esté activo.".to_string())
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
