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
        // Script de PowerShell optimizado que evita codificar nombres estáticos ("micro-printer").
        // Busca impresoras mediante las propiedades de clase y servicio, 
        // e identifica interfaces compuestas o desconocidas utilizando la clase de hardware 'Class_07',
        // que es el estándar universal para dispositivos de impresión USB.
        let ps_script = r#"
            $devs = Get-CimInstance Win32_PnPEntity -ErrorAction SilentlyContinue
            $results = @()
            foreach ($d in $devs) {
                if ($null -eq $d -or $null -eq $d.PNPDeviceID) { continue }
                
                $id = $d.PNPDeviceID
                $name = $d.Caption
                $class = $d.PNPClass
                if ($null -eq $class) { $class = "" }
                $svc = $d.Service
                if ($null -eq $svc) { $svc = "" }

                $isPrinter = $false
                $isSerial = $false

                if ($class -eq 'Ports' -and $name -match '\(COM\d+\)') {
                    $isSerial = $true
                }
                elseif ($id -match '^USBPRINT\\' -or $svc -eq 'usbprint' -or ($class -eq 'USB' -and $name -match 'print|impresora|pos-|tm-|bixolon|receipt|thermal|termica')) {
                    $isPrinter = $true
                }
                elseif ($id -match '^USB\\') {
                    # Si no está clasificada explícitamente pero es USB, miramos si expone 'Class_07' (USB Printer)
                    # en sus Compatible IDs registrados en el sistema. Esto detecta impresoras crudas ("No especificado")
                    $compat = (Get-PnpDeviceProperty -InstanceId $id -KeyName 'DEVPKEY_Device_CompatibleIds' -ErrorAction SilentlyContinue).Data -join ','
                    if ($compat -match 'Class_07') {
                        $isPrinter = $true
                    }
                }

                if ($isPrinter -or $isSerial) {
                    # ¡AQUÍ ESTÁ EL SECRETO DEL NOMBRE COMERCIAL REAL!
                    # Windows oculta el nombre original que reporta el fabricante (Ej: "micro-printer", "XP-80")
                    # en la propiedad de hardware BusReportedDeviceDesc.
                    # Si existe, sobrescribimos el nombre genérico "Compatibilidad con impresoras USB"
                    if ($id -match '^USB') {
                        $busDesc = (Get-PnpDeviceProperty -InstanceId $id -KeyName 'DEVPKEY_Device_BusReportedDeviceDesc' -ErrorAction SilentlyContinue).Data
                        if ($busDesc -and $busDesc.Trim() -ne '') {
                            $name = $busDesc.Trim()
                        }
                    }

                    $type = if ($isPrinter) { "USB-Printer" } else { "USB-Serial" }
                    $results += [PSCustomObject]@{
                        Class = $class
                        FriendlyName = $name
                        InstanceId = $id
                        Type = $type
                    }
                }
            }
            $results | ConvertTo-Csv -NoTypeInformation | Select-Object -Skip 1
        "#;

        if let Ok(output) = crate::hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                ps_script,
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            // Cada línea procesada por ConvertTo-Csv es "Class","FriendlyName","InstanceId","Type"
            for line in text.lines() {
                if line.trim().is_empty() { continue; }
                let parts: Vec<&str> = line.split("\",\"").collect();
                if parts.len() >= 4 {
                    let friendly_name = parts[1].trim_matches('"');
                    let instance_id = parts[2].trim_matches('"');
                    let dev_type = parts[3].trim_matches('"');

                    if dev_type == "USB-Serial" {
                        if let Some(start) = friendly_name.rfind("(COM") {
                            if let Some(end) = friendly_name[start..].find(')') {
                                let port_name = &friendly_name[start + 1..start + end];
                                ports.push(SerialPort {
                                    port_name: port_name.to_string(),
                                    description: friendly_name.to_string(),
                                    device_type: "USB-Serial".to_string(),
                                });
                            }
                        }
                    } else if dev_type == "USB-Printer" {
                        ports.push(SerialPort {
                            port_name: instance_id.to_string(),
                            description: friendly_name.to_string(),
                            device_type: "USB-Printer".to_string(),
                        });
                    }
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
