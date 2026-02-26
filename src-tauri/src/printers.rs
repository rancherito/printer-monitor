use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PrinterInfo {
    /// Nombre visible para el usuario (descripción/display name del SO).
    /// En macOS/Linux: valor de `printer-info` / "Description:" de CUPS.
    /// En Windows: nombre de la impresora (que ya es el display name).
    pub name: String,
    /// Nombre interno de la cola CUPS (`lpadmin -p <queue_name>`).
    /// Es el identificador que requieren comandos como `lp -d` o `lpadmin -p`.
    /// En Windows coincide con `name`. En macOS puede diferir si el usuario
    /// renombró la impresora en Preferencias del Sistema sin cambiar la cola.
    pub queue_name: String,
    pub is_default: bool,
    pub status: String,
}

/// Obtiene las impresoras instaladas en el sistema.
///
/// macOS/Linux: usa `lpstat -p`. El formato de salida varía según el locale del SO:
///   - Inglés:  "printer NAME is idle ..."
///   - Español: "la impresora NAME está inactiva ..."
/// La estrategia robusta es buscar el token "printer" o "impresora" en cada
/// línea y tomar el token inmediatamente siguiente como nombre de cola.
///
/// Windows: usa `Get-Printer` (PowerShell moderno) con `wmic` como fallback
/// para sistemas con Windows < 10 21H1 donde wmic todavía funciona.
#[tauri::command]
pub fn get_printers() -> Vec<PrinterInfo> {
    let mut printers: Vec<PrinterInfo> = Vec::new();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::process::Command;

        // Impresora predeterminada — último token después de ":"
        // EN: "system default destination: NAME"
        // ES: "destino por omisión del sistema: NAME"
        let default_printer = Command::new("lpstat")
            .args(["-d"])
            .output()
            .ok()
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout).to_string();
                out.split(':').last().map(|s| s.trim().to_string())
            })
            .unwrap_or_default();

        // Usamos `lpstat -l -p` (formato largo) para obtener tanto el nombre interno
        // de la cola CUPS (queue_name) como el nombre visible del SO (display name /
        // "Description:"). macOS muestra el display name en Preferencias del Sistema
        // → Impresoras y escáneres; los comandos lp/lpadmin requieren el queue_name.
        // Formato de salida (varía por locale del SO):
        //   EN: "printer QUEUE_NAME is idle. ...\n\tDescription: Display Name\n\t..."
        //   ES: "la impresora QUEUE_NAME está inactiva.\n\tDescripción: Nombre visible\n\t..."
        if let Ok(output) = Command::new("lpstat").args(["-l", "-p"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);

            struct Entry {
                queue_name: String,
                description: Option<String>,
                status: String,
                is_default: bool,
            }

            let mut entries: Vec<Entry> = Vec::new();
            let mut current: Option<Entry> = None;

            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let tokens: Vec<&str> = trimmed.split_whitespace().collect();

                // ¿Línea de cabecera de impresora?
                // Cubre: "printer NAME ..." / "impresora NAME ..." / "la impresora NAME ..."
                let name_idx = tokens
                    .iter()
                    .position(|t| {
                        t.eq_ignore_ascii_case("printer")
                            || t.eq_ignore_ascii_case("impresora")
                    })
                    .map(|i| i + 1);

                if let Some(idx) = name_idx {
                    // Guardar la entrada anterior antes de comenzar una nueva
                    if let Some(entry) = current.take() {
                        entries.push(entry);
                    }
                    if let Some(&queue) = tokens.get(idx) {
                        let line_lower = trimmed.to_lowercase();
                        let status = if line_lower.contains("idle")
                            || line_lower.contains("inactiva")
                            || line_lower.contains("en espera")
                            || line_lower.contains("habilitada")
                        {
                            "Disponible".to_string()
                        } else if line_lower.contains("disabled")
                            || line_lower.contains("deshabilitada")
                        {
                            "Deshabilitada".to_string()
                        } else if line_lower.contains("printing")
                            || line_lower.contains("imprimiendo")
                        {
                            "Imprimiendo".to_string()
                        } else {
                            "Disponible".to_string()
                        };
                        let is_default = queue == default_printer;
                        current = Some(Entry {
                            queue_name: queue.to_string(),
                            description: None,
                            status,
                            is_default,
                        });
                    }
                } else if let Some(ref mut entry) = current {
                    // Línea de detalle (indentada). Buscamos la de descripción/display name.
                    // La clave varía por locale pero siempre tiene "descri" (EN/ES/IT/FR/PT)
                    // o "schrij" (NL). Ej: "Description:", "Descripción:", "Descrizione:"
                    if entry.description.is_none() {
                        if let Some(colon_pos) = trimmed.find(':') {
                            let key = trimmed[..colon_pos].trim().to_lowercase();
                            if key.contains("descri") || key.contains("schrij") {
                                let val = trimmed[colon_pos + 1..].trim().to_string();
                                if !val.is_empty() {
                                    entry.description = Some(val);
                                }
                            }
                        }
                    }
                }
            }
            // Guardar la última entrada
            if let Some(entry) = current {
                entries.push(entry);
            }

            for entry in entries {
                // Si CUPS no tiene descripción configurada, mostramos el queue_name
                let name = entry
                    .description
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| entry.queue_name.clone());
                printers.push(PrinterInfo {
                    name,
                    queue_name: entry.queue_name,
                    is_default: entry.is_default,
                    status: entry.status,
                });
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // ── Primario: Get-Printer (disponible desde Windows 8 / Server 2012) ──
        let ps_ok = crate::hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "Get-Printer | Select-Object -Property Name,Default,PrinterStatus | ConvertTo-Csv -NoTypeInformation",
            ])
            .output()
            .map(|out| {
                if !out.status.success() {
                    return false;
                }
                let text = String::from_utf8_lossy(&out.stdout);
                let mut found = false;
                for line in text.lines().skip(1) {
                    let cols: Vec<&str> =
                        line.split(',').map(|c| c.trim().trim_matches('"')).collect();
                    if cols.len() >= 3 {
                        let name = cols[0].to_string();
                        if name.is_empty() {
                            continue;
                        }
                        let is_default = cols[1].eq_ignore_ascii_case("True");
                        let ps = cols[2].to_lowercase();
                        let status = if ps.contains("normal")
                            || ps.contains("idle")
                            || ps.contains("ready")
                        {
                            "Disponible".to_string()
                        } else if ps.contains("offline") || ps.contains("error") {
                            "Sin conexión".to_string()
                        } else if ps.contains("paused") {
                            "Pausada".to_string()
                        } else if ps.contains("print") {
                            "Imprimiendo".to_string()
                        } else {
                            "Disponible".to_string()
                        };
                        // En Windows el nombre de impresora ES el nombre visible;
                        // queue_name y name son idénticos.
                        printers.push(PrinterInfo { name: name.clone(), queue_name: name, is_default, status });
                        found = true;
                    }
                }
                found
            })
            .unwrap_or(false);

        // ── Fallback: wmic (Windows < 10 21H1) ──────────────────────────────
        if !ps_ok {
            if let Ok(output) = crate::hidden_cmd("wmic")
                .args(["printer", "get", "Name,Default,PrinterStatus", "/format:csv"])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines().skip(2) {
                    let cols: Vec<&str> = line.split(',').collect();
                    if cols.len() >= 4 {
                        let is_default = cols[1].trim().eq_ignore_ascii_case("TRUE");
                        let name = cols[2].trim().to_string();
                        if name.is_empty() {
                            continue;
                        }
                        let status = match cols[3].trim() {
                            "3" => "Disponible".to_string(),
                            "4" => "Imprimiendo".to_string(),
                            "5" => "Calentando".to_string(),
                            _ => "Disponible".to_string(),
                        };
                        printers.push(PrinterInfo { name: name.clone(), queue_name: name, is_default, status });
                    }
                }
            }
        }
    }

    printers
}

/// Renombra una impresora. En macOS/Linux cambia la descripción (lpadmin -D).
/// En Windows usa Rename-Printer de PowerShell.
#[tauri::command]
pub fn rename_printer(printer_name: String, new_name: String) -> Result<String, String> {
    use std::process::Command;

    let name = new_name.trim().to_string();
    if name.is_empty() {
        return Err("El nuevo nombre no puede estar vacío".to_string());
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let result = Command::new("lpadmin")
        .args(["-p", &printer_name, "-D", &name])
        .output()
        .map_err(|e| format!("No se pudo ejecutar lpadmin: {e}"))
        .and_then(|o| {
            if o.status.success() {
                Ok(format!("Impresora renombrada a \u{ab}{}\u{bb}", name))
            } else {
                Err(format!(
                    "Error al renombrar: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ))
            }
        });

    #[cfg(target_os = "windows")]
    let result = {
        let script = format!(
            "Rename-Printer -Name '{}' -NewName '{}'",
            printer_name.replace('\'', "''"),
            name.replace('\'', "''")
        );
        crate::hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ])
            .output()
            .map_err(|e| format!("No se pudo ejecutar PowerShell: {e}"))
            .and_then(|o| {
                if o.status.success() {
                    Ok(format!("Impresora renombrada a \u{ab}{}\u{bb}", name))
                } else {
                    Err(format!(
                        "Error al renombrar: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    ))
                }
            })
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let result: Result<String, String> = Err("Sistema operativo no soportado".to_string());

    result
}

/// Registra una impresora de red en el sistema operativo.
#[tauri::command]
pub fn add_network_printer(ip: String, name: String) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Verificar que CUPS está disponible (lpstat debe poder ejecutarse).
        // lpstat -v devuelve exit code 1 cuando no hay impresoras aún — eso es normal.
        // Solo fallamos si el binario no existe (IO error = CUPS no instalado).
        Command::new("lpstat")
            .arg("-v")
            .output()
            .map_err(|_| "El sistema de impresión CUPS no está disponible. Verifica en Preferencias del Sistema → Impresoras y Escáneres.".to_string())?;

        // Detectar el mejor driver genérico disponible con lpinfo -m.
        // macOS 12+ no admite -m raw; si no encontramos ninguno, omitimos -m.
        let driver_flag: String = Command::new("lpinfo")
            .arg("-m")
            .output()
            .ok()
            .and_then(|out| {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                for candidate in &[
                    "Generic-PDF_Printer",
                    "Generic PostScript Printer",
                    "generic.ppd",
                    "drv:///sample.drv/generic",
                    "Generic",
                ] {
                    if let Some(line) =
                        text.lines().find(|l| l.to_lowercase().contains(&candidate.to_lowercase()))
                    {
                        let model = line.split_whitespace().next().unwrap_or("").to_string();
                        if !model.is_empty() {
                            return Some(format!(" -m '{}'", model.replace("'", "\\'")));
                        }
                    }
                }
                None
            })
            .unwrap_or_default();

        // URI JetDirect confirmado por el usuario (puerto 9100, protocolo Socket)
        let uri = format!("socket://{}:9100", ip);
        let safe_name = name.replace("'", "\\'");
        let lpadmin_cmd = format!(
            "/usr/sbin/lpadmin -p '{}' -v '{}' -E{}",
            safe_name, uri, driver_flag
        );

        // Escribir el comando a un .sh temporal.
        // Ejecutar como archivo (no inline) evita el error:
        //   "shell-init: getcwd: cannot access parent directories"
        // que ocurre cuando el shell de osascript hereda el CWD del sandbox de Tauri.
        let script_path = "/private/tmp/cups_add_printer.sh";
        std::fs::write(
            script_path,
            format!("#!/bin/sh\nexport HOME=/private/tmp\ncd /private/tmp\n{}\n", lpadmin_cmd),
        )
        .map_err(|e| format!("Error al crear script temporal: {}", e))?;
        let _ = Command::new("chmod").args(["+x", script_path]).output();

        println!("🖨️ Ejecutando: {}", lpadmin_cmd);

        let output = Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "do shell script \"{}\" with administrator privileges",
                    script_path
                ),
            ])
            .output()
            .map_err(|e| format!("Error al ejecutar osascript: {}", e))?;

        let _ = std::fs::remove_file(script_path);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stdout.is_empty() { println!("📤 stdout: {}", stdout); }
        if !stderr.is_empty() { println!("📤 stderr: {}", stderr); }

        let has_shell_init_warn =
            stderr.contains("shell-init") || stderr.contains("getcwd");
        let has_real_error = stderr.contains("lpadmin:")
            || stderr.contains("Unable to")
            || stderr.contains("no se ha podido");

        if output.status.success() || (has_shell_init_warn && !has_real_error) {
            let verify = Command::new("lpstat").args(["-p", &name]).output();
            if verify.map(|o| o.status.success()).unwrap_or(false) {
                Ok(format!("Impresora '{}' agregada correctamente ({}:9100)", name, ip))
            } else {
                Ok(format!(
                    "Impresora '{}' registrada. Si no aparece en la lista, pulsa Actualizar.",
                    name
                ))
            }
        } else if stderr.contains("User cancelled") || stderr.contains("cancelado") {
            Err("Operación cancelada por el usuario.".to_string())
        } else if has_real_error {
            let msg = stderr
                .lines()
                .find(|l| {
                    l.contains("lpadmin:")
                        || l.contains("Unable to")
                        || l.contains("no se ha podido")
                })
                .unwrap_or(stderr.trim());
            Err(format!("Error de CUPS: {}", msg.trim()))
        } else if !stderr.is_empty() {
            Err(format!("Error: {}", stderr.trim()))
        } else {
            Err("Error desconocido al agregar impresora.".to_string())
        }
    }

    #[cfg(target_os = "windows")]
    {
        let port_name = format!("IP_{}", ip.replace('.', "_"));
        let temp_dir = std::env::temp_dir();
        let result_file = temp_dir.join("printer_add_result.txt");
        let result_path = result_file.to_string_lossy().replace('\\', "/");

        let elevated_script = format!(
            "$out = '{result}'; \
             try {{ \
                 $sp = Get-Service Spooler -EA Stop; \
                 if ($sp.Status -ne 'Running') {{ Start-Service Spooler -EA Stop }}; \
                 Add-PrinterPort -Name '{port}' -PrinterHostAddress '{ip}' -EA SilentlyContinue; \
                 $drivers = @('Generic / Text Only','Generic Text Only','HP LaserJet 1020','Microsoft XPS Document Writer'); \
                 $drv = $drivers | Where-Object {{ Get-PrinterDriver -Name $_ -EA SilentlyContinue }} | Select-Object -First 1; \
                 if (-not $drv) {{ throw 'No se encontró un driver genérico compatible. Instala un driver de impresora primero.' }}; \
                 Add-Printer -Name '{name}' -DriverName $drv -PortName '{port}' -EA Stop; \
                 Set-Content -Path $out -Value 'OK' \
             }} catch {{ \
                 Set-Content -Path $out -Value $_.Exception.Message \
             }}",
            result = result_path,
            port = port_name,
            ip = ip,
            name = name.replace('\'', "''"),
        );

        let script_file = temp_dir.join("printer_add_script.ps1");
        std::fs::write(&script_file, &elevated_script)
            .map_err(|e| format!("Error al crear script temporal: {}", e))?;
        let script_path = script_file.to_string_lossy().to_string();

        let _ = std::fs::remove_file(&result_file);
        let launch = crate::hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &format!(
                    "Start-Process powershell -Verb RunAs -Wait \
                     -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File \"{}\"'",
                    script_path
                ),
            ])
            .output()
            .map_err(|e| format!("Error al lanzar proceso elevado: {}", e))?;

        let _ = std::fs::remove_file(&script_file);

        let result_text = std::fs::read_to_string(&result_file).unwrap_or_default();
        let result_text = result_text.trim();
        let _ = std::fs::remove_file(&result_file);

        let launch_stderr = String::from_utf8_lossy(&launch.stderr);
        if launch_stderr.contains("cancelled")
            || launch_stderr.contains("cancelado")
            || launch_stderr.contains("The operation was canceled")
        {
            return Err("Operación cancelada por el usuario.".to_string());
        }

        if result_text == "OK" {
            Ok(format!("Impresora '{}' agregada correctamente en {}:9100", name, ip))
        } else if result_text.contains("already exists") || result_text.contains("ya existe") {
            Err("Ya existe una impresora o puerto con ese nombre. Usa otro nombre.".to_string())
        } else if result_text.contains("PrivilegeNotHeld")
            || result_text.contains("Access")
            || result_text.contains("Privilege")
        {
            Err("Permisos insuficientes. Acepta el diálogo UAC cuando se solicite.".to_string())
        } else if result_text.contains("Spooler") || result_text.contains("spooler") {
            Err("El servicio de cola de impresión (Spooler) no está activo. Inícialo desde Servicios de Windows.".to_string())
        } else if !result_text.is_empty() {
            Err(format!("Error al agregar impresora: {}", result_text))
        } else {
            Err("No se pudo agregar la impresora. Acepta el diálogo de permisos (UAC) cuando aparezca.".to_string())
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let uri = format!("socket://{}:9100", ip);
        let output = Command::new("pkexec")
            .args(["lpadmin", "-p", &name, "-v", &uri, "-E"])
            .output()
            .map_err(|e| format!("Error al agregar impresora: {}", e))?;
        if output.status.success() {
            Ok(format!("Impresora '{}' agregada correctamente", name))
        } else {
            Err(format!(
                "Error al agregar impresora: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
}

/// Genera un PDF de prueba y lo envía a la impresora indicada.
#[tauri::command]
pub fn print_test(printer_name: String, size: String) -> Result<String, String> {
    use std::io::Write;
    use std::process::Command;

    let (page_width, page_height, label) = match size.as_str() {
        "thermal_50mm" => (142u32, 200u32, "Térmica 50mm"),
        "thermal_80mm" => (227u32, 200u32, "Térmica 80mm"),
        _ => (595u32, 842u32, "A4"),
    };

    let title = format!("Página de prueba — {}", label);
    let body_lines = vec![
        "Printer Monitor — Prueba de impresión".to_string(),
        format!("Impresora: {}", printer_name),
        format!("Formato:   {}", label),
        format!("Fecha:     {}", chrono::Local::now().format("%d/%m/%Y %H:%M:%S")),
        String::new(),
        "Si ves este texto, la impresora".to_string(),
        "funciona correctamente.".to_string(),
    ];

    let pdf_bytes = build_test_pdf(page_width, page_height, &title, &body_lines);
    let tmp_path = std::env::temp_dir()
        .join(format!("pm_test_{}.pdf", printer_name.replace(' ', "_")));

    {
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("No se pudo crear archivo temporal: {e}"))?;
        f.write_all(&pdf_bytes)
            .map_err(|e| format!("No se pudo escribir PDF: {e}"))?;
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let output = Command::new("lp")
            .args(["-d", &printer_name, tmp_path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| format!("Error al ejecutar lp: {e}"))?;
        let _ = std::fs::remove_file(&tmp_path);
        if output.status.success() {
            Ok(format!("Trabajo enviado a «{}» ({})", printer_name, label))
        } else {
            Err(format!(
                "Error de impresión: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = crate::hidden_cmd("cmd")
            .args([
                "/C",
                "print",
                &format!("/D:{}", printer_name),
                tmp_path.to_str().unwrap_or(""),
            ])
            .output()
            .map_err(|e| format!("Error al ejecutar print: {e}"))?;
        let _ = std::fs::remove_file(&tmp_path);
        if output.status.success() {
            Ok(format!("Trabajo enviado a «{}» ({})", printer_name, label))
        } else {
            Err(format!(
                "Error de impresión: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

// ─── ESC/POS directo a USB ───────────────────────────────────────────────────

/// Envía una página de prueba ESC/POS directamente al dispositivo serie/USB indicado
/// **sin** pasar por CUPS. Apto para impresoras térmicas conectadas por USB que aparecen
/// como puerto serie (`/dev/cu.usbmodem*`, `ttyUSB*`, `COM*`).
///
/// El tamaño determina la anchura del ticket:
/// - `thermal_50mm` → 32 caracteres por línea (rollo 58 mm, área imprimible ≈ 48 mm)
/// - `thermal_80mm` → 48 caracteres por línea (rollo 80 mm, área imprimible ≈ 72 mm)
#[tauri::command]
pub fn print_test_usb(port_name: String, size: String) -> Result<String, String> {
    let (chars_per_line, label) = match size.as_str() {
        "thermal_80mm" => (48usize, "Termica 80mm"),
        _ => (32usize, "Termica 50mm"),
    };

    let data = build_escpos_test(&port_name, chars_per_line, label);
    write_to_serial_port(&port_name, &data)
}

/// Construye el payload ESC/POS de la página de prueba.
fn build_escpos_test(port_name: &str, chars_per_line: usize, label: &str) -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();

    // ESC @ — Inicializar impresora
    d.extend_from_slice(&[0x1B, 0x40]);

    // --- Título centrado, doble ancho + doble alto ---
    // ESC a 1 — Alineación centrada
    d.extend_from_slice(&[0x1B, 0x61, 0x01]);
    // ESC ! 0x30 — Doble ancho + doble alto
    d.extend_from_slice(&[0x1B, 0x21, 0x30]);
    d.extend_from_slice(b"PRUEBA OK\n");
    // ESC ! 0x00 — Fuente normal
    d.extend_from_slice(&[0x1B, 0x21, 0x00]);
    d.extend_from_slice(format!("{}\n", label).as_bytes());

    // --- Cuerpo alineado a la izquierda ---
    // ESC a 0 — Alineación izquierda
    d.extend_from_slice(&[0x1B, 0x61, 0x00]);

    let sep = "-".repeat(chars_per_line);
    d.extend_from_slice(format!("{}\n", sep).as_bytes());

    // Nombre corto del puerto (sin ruta completa)
    let port_short = port_name.rsplit('/').next().unwrap_or(port_name);
    d.extend_from_slice(format!("Puerto: {}\n", port_short).as_bytes());
    d.extend_from_slice(b"Printer Monitor\n");
    d.extend_from_slice(
        format!(
            "Fecha: {}\n",
            chrono::Local::now().format("%d/%m/%Y %H:%M")
        )
        .as_bytes(),
    );
    d.extend_from_slice(b"Si ves esto, funciona!\n");
    d.extend_from_slice(format!("{}\n", sep).as_bytes());

    // ESC d 5 — Avanzar 5 líneas
    d.extend_from_slice(&[0x1B, 0x64, 0x05]);

    // GS V A 0 — Corte total
    d.extend_from_slice(&[0x1D, 0x56, 0x41, 0x00]);

    d
}

/// Abre el dispositivo serie y escribe los bytes.
///
/// macOS/Linux: abre el archivo de dispositivo directamente y (opcionalmente)
/// configura el puerto con `stty` en modo raw antes de escribir.
/// Windows: usa la ruta `\\.\COMx` para puertos con número > 9.
fn write_to_serial_port(port_name: &str, data: &[u8]) -> Result<String, String> {
    use std::io::Write;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::fs::OpenOptions;
        use std::process::Command;

        // Intentar configurar el puerto en modo raw.
        // Si falla (p.ej. dispositivo USB-CDC que no necesita baud rate), se ignora.
        let stty_flag = if cfg!(target_os = "macos") { "-f" } else { "-F" };
        let _ = Command::new("stty")
            .args([stty_flag, port_name, "raw", "9600", "-echo", "cs8", "-cstopb", "-parenb"])
            .output();

        let mut file = OpenOptions::new()
            .write(true)
            .open(port_name)
            .map_err(|e| format!("No se pudo abrir {}: {}", port_name, e))?;

        file.write_all(data)
            .map_err(|e| format!("Error al enviar datos al puerto: {}", e))?;

        let short = port_name.rsplit('/').next().unwrap_or(port_name);
        return Ok(format!("Prueba enviada a {}", short));
    }

    #[cfg(target_os = "windows")]
    {
        // COM1..COM9 → "COM1"; COM10+ necesitan prefijo "\\\\.\\COM10"
        let path = if port_name.starts_with("COM") {
            format!("\\\\.\\{}", port_name)
        } else {
            port_name.to_string()
        };

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| format!("No se pudo abrir {}: {}", port_name, e))?;

        file.write_all(data)
            .map_err(|e| format!("Error al enviar datos al puerto: {}", e))?;

        return Ok(format!("Prueba enviada a {}", port_name));
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Sistema operativo no soportado".to_string())
}

// ─── Helpers PDF ─────────────────────────────────────────────────────────────

fn build_test_pdf(width: u32, height: u32, title: &str, lines: &[String]) -> Vec<u8> {
    let mut objects: Vec<String> = Vec::new();
    objects.push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj".to_string());
    objects.push("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj".to_string());

    let font_size_title = if width < 200 { 9 } else { 14 };
    let font_size_body = if width < 200 { 7 } else { 11 };
    let margin = if width < 200 { 8.0f32 } else { 50.0 };
    let line_height = (font_size_body as f32) * 1.6;
    let start_y = (height as f32) - margin - (font_size_title as f32) - 10.0;

    let mut stream = String::new();
    stream.push_str("BT\n");
    stream.push_str(&format!("/F1 {} Tf\n", font_size_title));
    stream.push_str(&format!("{} {} Td\n", margin, start_y));
    stream.push_str(&format!("({}) Tj\n", escape_pdf_string(title)));
    let sep_count =
        ((width as f32 - margin * 2.0) / (font_size_body as f32 * 0.5)) as usize;
    let separator = "-".repeat(sep_count.min(60));
    stream.push_str(&format!("/F1 {} Tf\n", font_size_body));
    stream.push_str(&format!("0 -{} Td\n", line_height * 1.2));
    stream.push_str(&format!("({}) Tj\n", separator));
    for line in lines {
        stream.push_str(&format!("0 -{} Td\n", line_height));
        stream.push_str(&format!("({}) Tj\n", escape_pdf_string(line)));
    }
    stream.push_str("ET\n");

    let stream_bytes = stream.as_bytes().len();
    objects.push(format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj",
        stream_bytes, stream
    ));
    objects.push(
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj"
            .to_string(),
    );
    objects.insert(
        2,
        format!(
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj",
            width, height
        ),
    );

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets: Vec<usize> = Vec::new();
    for obj in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(obj.as_bytes());
        pdf.push(b'\n');
    }

    let xref_offset = pdf.len();
    let xref_count = objects.len() + 1;
    let mut xref = format!("xref\n0 {}\n", xref_count);
    xref.push_str("0000000000 65535 f \n");
    for &off in &offsets {
        xref.push_str(&format!("{:010} 00000 n \n", off));
    }
    pdf.extend_from_slice(xref.as_bytes());
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            xref_count, xref_offset
        )
        .as_bytes(),
    );
    pdf
}

fn escape_pdf_string(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '(' => r"\(".to_string(),
            ')' => r"\)".to_string(),
            '\\' => r"\\".to_string(),
            c if c.is_ascii() => c.to_string(),
            _ => "?".to_string(),
        })
        .collect()
}
