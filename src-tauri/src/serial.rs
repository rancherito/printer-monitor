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
        use std::process::Command;

        // ── Helpers de parsing ────────────────────────────────────────────────
        fn get_quoted(block: &str, key: &str) -> Option<String> {
            let needle = format!("\"{}\" = \"", key);
            let pos = block.find(&needle)?;
            let rest = &block[pos + needle.len()..];
            Some(rest.split('"').next().unwrap_or("").trim().to_string())
        }

        fn get_num(block: &str, key: &str) -> Option<u32> {
            let needle = format!("\"{}\" = ", key);
            let pos = block.find(&needle)?;
            let rest = &block[pos + needle.len()..];
            rest.split_whitespace().next()?.trim_end_matches(';').parse().ok()
        }

        // Clasifica por nombre cuando la clase USB es 0 (interface-defined)
        fn classify_by_name(name: &str) -> &'static str {
            let n = name.to_lowercase();
            if n.contains("keyboard") || n.contains("teclado")
                || n.contains("mouse") || n.contains("trackpad")
                || n.contains("touchpad") || n.contains(" hid")
            {
                return "USB-HID";
            }
            if n.contains("printer") || n.contains("impresora")
                || n.contains("tm-") || n.contains("bixolon")
                || n.contains("star ") || n.contains("epson")
                || n.contains("citizen") || n.contains("posiflex")
                || n.contains("sewoo") || n.contains("micro-printer")
                || n.contains("thermal") || n.contains("receipt")
                || n.contains("pos")
            {
                return "USB-Printer";
            }
            if n.contains("serial") || n.contains("uart")
                || n.contains("cp210") || n.contains("ch34")
                || n.contains("ft232") || n.contains("pl2303")
            {
                return "USB-Serial";
            }
            "USB"
        }

        // ── Recopilar entradas /dev/cu.usb* ───────────────────────────────────
        let mut dev_entries: Vec<(String, &'static str)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/dev") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("cu.usb") {
                    let dtype = if name.to_lowercase().contains("modem") {
                        "USB-CDC"
                    } else {
                        "USB-Serial"
                    };
                    dev_entries.push((format!("/dev/{}", name), dtype));
                }
            }
        }

        // ── Enumerar dispositivos USB con ioreg ───────────────────────────────
        // ioreg -p IOUSB -l -w 0 recorre el árbol IOKit USB en texto.
        // Cada nodo empieza con "+-o NombreDispositivo@dirección  <class ...>"
        // Las propiedades del nodo aparecen en las líneas siguientes hasta el
        // próximo "+-o".
        if let Ok(output) = Command::new("ioreg")
            .args(["-p", "IOUSB", "-l", "-w", "0"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);

            // Dividir la salida en bloques: uno por cada nodo "+-o ..."
            let mut blocks: Vec<String> = Vec::new();
            let mut cur = String::new();
            let mut capturing = false;

            for line in text.lines() {
                if line.contains("+-o ") && line.contains('@') {
                    if capturing && !cur.is_empty() {
                        blocks.push(std::mem::take(&mut cur));
                    }
                    cur.push_str(line);
                    cur.push('\n');
                    capturing = true;
                } else if capturing {
                    cur.push_str(line);
                    cur.push('\n');
                }
            }
            if capturing && !cur.is_empty() {
                blocks.push(cur);
            }

            // Track which /dev paths ya se representan en la lista
            let mut dev_used: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            for block in &blocks {
                // Obtener nombre del producto (campo "USB Product Name")
                let product = match get_quoted(block, "USB Product Name") {
                    Some(n) if !n.is_empty() => n,
                    _ => continue,
                };

                // Clase del dispositivo (9 = hub → omitir)
                let dev_class = get_num(block, "bDeviceClass").unwrap_or(0);
                if dev_class == 9 {
                    continue;
                }

                // Intentar asociar a una entrada /dev usando el serial USB
                let usb_serial = get_quoted(block, "USB Serial Number")
                    .unwrap_or_default()
                    .to_uppercase();

                let matched_dev = dev_entries.iter().find(|(path, _)| {
                    !usb_serial.is_empty() && path.to_uppercase().contains(&usb_serial)
                });

                let (port_name, device_type) = if let Some((path, dtype)) = matched_dev {
                    dev_used.insert(path.clone());
                    (path.clone(), dtype.to_string())
                } else {
                    let dtype = match dev_class {
                        3 => "USB-HID",           // HID (teclados, ratones)
                        7 => "USB-Printer",        // Printer
                        2 | 10 => "USB-CDC",       // CDC / CDC-Data
                        _ => classify_by_name(&product),
                    };
                    (product.clone(), dtype.to_string())
                };

                ports.push(SerialPort {
                    port_name,
                    description: product,
                    device_type,
                });
            }

            // Añadir entradas /dev/cu.usb* que no quedaron representadas
            for (path, dtype) in &dev_entries {
                if !dev_used.contains(path) {
                    let description = path.trim_start_matches("/dev/").to_string();
                    ports.push(SerialPort {
                        port_name: path.clone(),
                        description,
                        device_type: dtype.to_string(),
                    });
                }
            }
        } else {
            // Fallback: solo listar /dev/cu.usb*
            for (path, dtype) in dev_entries {
                let description = path.trim_start_matches("/dev/").to_string();
                ports.push(SerialPort {
                    port_name: path,
                    description,
                    device_type: dtype.to_string(),
                });
            }
        }

        ports.sort_by(|a, b| a.description.cmp(&b.description));
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
