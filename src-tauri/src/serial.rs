use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SerialPort {
    pub port_name: String,
    pub description: String,
    pub device_type: String,
}

pub(crate) fn list_serial_ports() -> Vec<SerialPort> {
    let mut ports: Vec<SerialPort> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Ok(entries) = std::fs::read_dir("/dev") {
            let mut found: Vec<SerialPort> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("cu.usb") {
                        let device_type = if name.to_lowercase().contains("modem") {
                            "USB-CDC"
                        } else {
                            "USB-Serial"
                        };
                        Some(SerialPort {
                            port_name: format!("/dev/{}", name),
                            description: name.clone(),
                            device_type: device_type.to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            found.sort_by(|a, b| a.port_name.cmp(&b.port_name));
            ports.extend(found);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/dev") {
            let mut found: Vec<SerialPort> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("ttyUSB") || name.starts_with("ttyACM") {
                        let device_type =
                            if name.starts_with("ttyACM") { "USB-CDC" } else { "USB-Serial" };
                        Some(SerialPort {
                            port_name: format!("/dev/{}", name),
                            description: name.clone(),
                            device_type: device_type.to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            found.sort_by(|a, b| a.port_name.cmp(&b.port_name));
            ports.extend(found);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = crate::hidden_cmd("wmic")
            .args(["path", "Win32_SerialPort", "get", "DeviceID,Description", "/format:csv"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            for line in text.lines().skip(2) {
                let cols: Vec<&str> = line.split(',').collect();
                let port_name = cols
                    .iter()
                    .map(|s| s.trim())
                    .find(|s| s.starts_with("COM"))
                    .unwrap_or("")
                    .to_string();
                let description = cols.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                if !port_name.is_empty() {
                    let device_type = if description.to_lowercase().contains("usb") {
                        "USB-Serial"
                    } else {
                        "COM"
                    };
                    ports.push(SerialPort {
                        port_name,
                        description,
                        device_type: device_type.to_string(),
                    });
                }
            }
        }
    }

    ports
}

#[tauri::command]
pub fn get_serial_ports() -> Vec<SerialPort> {
    list_serial_ports()
}
