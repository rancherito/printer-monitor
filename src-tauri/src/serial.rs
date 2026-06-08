use crate::printer_cache;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct UsbDevice {
    pub display_name: String,
    pub port: String,
}

#[tauri::command]
pub async fn get_serial_ports() -> Vec<String> {
    tokio::task::spawn_blocking(get_serial_port_list)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn get_usb_devices() -> Vec<UsbDevice> {
    tokio::task::spawn_blocking(get_usb_devices_list)
        .await
        .unwrap_or_default()
}

fn get_usb_devices_list() -> Vec<UsbDevice> {
    printer_cache::get_or_load_usb(get_usb_devices_uncached)
}

fn get_usb_devices_uncached() -> Vec<UsbDevice> {
    #[cfg(target_os = "windows")]
    return get_usb_devices_windows();

    #[cfg(not(target_os = "windows"))]
    {
        get_serial_port_list()
            .into_iter()
            .filter(|p| !p.to_ascii_uppercase().starts_with("COM"))
            .map(|p| UsbDevice { display_name: p.clone(), port: p })
            .collect()
    }
}

#[cfg(target_os = "windows")]
fn get_usb_devices_windows() -> Vec<UsbDevice> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Walk HKLM\...\Enum\USBPRINT: every instance that has a USB# port.
    // Get the parent USB device via DEVPKEY_Device_Parent; only include it when
    // the parent Status is OK (physically connected). Display name comes from
    // the Windows spooler queue (if installed) or the parent USB FriendlyName.
    let script = r#"
$spoolerMap = @{}
Get-WmiObject Win32_Printer -EA SilentlyContinue |
    Where-Object { $_.PortName -like 'USB*' } |
    ForEach-Object { if (-not $spoolerMap[$_.PortName]) { $spoolerMap[$_.PortName] = $_.Name } }

$r = @()
$enumBase = 'HKLM:\SYSTEM\CurrentControlSet\Enum\USBPRINT'
if (Test-Path $enumBase) {
    Get-ChildItem $enumBase -EA SilentlyContinue | ForEach-Object {
        $modelKey = $_
        Get-ChildItem $modelKey.PSPath -EA SilentlyContinue | ForEach-Object {
            $instName = $_.PSChildName
            if ($instName -match '&(USB\d+)$') {
                $portNum = $Matches[1]
                $instanceId = 'USBPRINT\' + $modelKey.PSChildName + '\' + $instName
                $parentId = (Get-PnpDeviceProperty -InstanceId $instanceId -KeyName DEVPKEY_Device_Parent -EA SilentlyContinue).Data
                if (-not $parentId) { return }
                $parentDev = Get-PnpDevice -InstanceId $parentId -EA SilentlyContinue
                if (-not $parentDev -or $parentDev.Status -ne 'OK') { return }
                # Always use the hardware FriendlyName so the user sees the physical
                # device name regardless of whether a spooler queue already exists.
                $displayName = if ($parentDev.FriendlyName) { $parentDev.FriendlyName } else { $portNum }
                $r += [PSCustomObject]@{ port = $portNum; display_name = $displayName }
            }
        }
    }
}
$r | ConvertTo-Json -Compress
"#;

    let out = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
        .output()
        .ok();

    match out {
        Some(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let json = stdout.trim();
            let arr_json = if json.starts_with('[') {
                json.to_string()
            } else if json.starts_with('{') {
                format!("[{json}]")
            } else {
                return vec![];
            };
            serde_json::from_str::<Vec<UsbDevice>>(&arr_json).unwrap_or_default()
        }
        _ => vec![],
    }
}

pub fn get_serial_port_list() -> Vec<String> {
    // COM / USB-to-serial ports (CH340, FTDI, CP210x, etc.)
    let mut ports: Vec<String> = serialport::available_ports()
        .unwrap_or_default()
        .iter()
        .map(|p| p.port_name.clone())
        .collect();

    // Windows: también listar puertos USB del subsistema de impresión (USB001, USB002…)
    #[cfg(target_os = "windows")]
    ports.extend(get_usb_print_ports_windows());

    // Linux: también listar nodos /dev/usb/lp*
    #[cfg(target_os = "linux")]
    ports.extend(get_usb_print_ports_linux());

    ports.sort();
    ports.dedup();
    ports
}

pub fn resolve_usb_port(current_or_saved: &str) -> Option<String> {
    // Device interface paths are used directly — no need to re-resolve.
    if current_or_saved.starts_with("\\\\?\\") {
        return Some(current_or_saved.to_string());
    }

    let ports = get_serial_port_list();
    if ports.iter().any(|p| p == current_or_saved) {
        return Some(current_or_saved.to_string());
    }

    if current_or_saved.starts_with("USB") {
        return ports.into_iter().find(|p| p.starts_with("USB"));
    }
    if current_or_saved.to_ascii_uppercase().starts_with("COM") {
        return ports.into_iter().find(|p| p.to_ascii_uppercase().starts_with("COM"));
    }
    if current_or_saved.starts_with("/dev/usb/lp") {
        return ports.into_iter().find(|p| p.starts_with("/dev/usb/lp"));
    }

    ports.into_iter().next()
}

/// Devuelve los puertos USB del Print Monitor de Windows (USB001, USB002, etc.).
/// Estos puertos son creados automáticamente por Windows al conectar una impresora USB.
#[cfg(target_os = "windows")]
fn get_usb_print_ports_windows() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let out = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "(Get-PrinterPort | Where-Object { $_.Name -like 'USB*' }).Name",
        ])
        .output()
        .ok();
    match out {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && l.starts_with("USB"))
            .collect(),
        _ => Vec::new(),
    }
}

/// Devuelve los nodos de impresora USB disponibles en Linux (/dev/usb/lp0, lp1…).
#[cfg(target_os = "linux")]
fn get_usb_print_ports_linux() -> Vec<String> {
    (0..8)
        .map(|i| format!("/dev/usb/lp{i}"))
        .filter(|p| std::path::Path::new(p).exists())
        .collect()
}
