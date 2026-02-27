#![allow(unused_imports)]
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

/// Logo de la app embebido en el binario en tiempo de compilación.
const APP_LOGO: &[u8] = include_bytes!("../icons/icon.png");

// ─── Driver ESC/POS en memoria ───────────────────────────────────────────────

/// Driver que acumula los bytes ESC/POS en un buffer compartido (`Arc<Mutex<Vec<u8>>>`)
/// en lugar de enviarlos por red o puerto serie. Permite usar el mismo
/// `escpos::Printer` para construir el payload y luego enviarlo por cualquier
/// canal (lp -d, socket…).
struct VecDriver {
    buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl VecDriver {
    /// Devuelve `(driver, shared_buf)`. Después de hacer `drop(printer)` se
    /// puede leer `shared_buf.lock().unwrap()` para obtener los bytes.
    fn new() -> (Self, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        (Self { buf: std::sync::Arc::clone(&buf) }, buf)
    }
}

impl escpos::driver::Driver for VecDriver {
    fn name(&self) -> String { "vec".to_string() }
    fn write(&self, data: &[u8]) -> escpos::errors::Result<()> {
        self.buf.lock().unwrap().extend_from_slice(data);
        Ok(())
    }
    fn flush(&self) -> escpos::errors::Result<()> { Ok(()) }
    fn read(&self, _buf: &mut [u8]) -> escpos::errors::Result<usize> { Ok(0) }
}

/// Pre-procesa una imagen para impresión térmica 1bpp.
///
/// Pasos:
///   1. Decodifica los bytes (PNG o JPEG).
///   2. Redimensiona a `max_width` px preservando proporción (Lanczos3).
///   3. Convierte a escala de grises (luma8).
///   4. Aplica dithering Floyd-Steinberg: cada píxel queda en 0 (negro) ó 255 (blanco).
///   5. Codifica el resultado como PNG y lo devuelve.
///
/// El crate `escpos` usa umbral fijo `luma <= 128 → negro`. Al pasarle
/// solo 0/255 el umbral es irrelevante y se conserva todo el detalle.
fn dither_image_bytes(img_bytes: &[u8], max_width: u32) -> Result<Vec<u8>, String> {
    use image::{GrayImage, DynamicImage, RgbImage};

    let img = image::load_from_memory(img_bytes)
        .map_err(|e| format!("Error cargando imagen: {e}"))?;

    // Redimensionar preservando proporción
    let orig_w = img.width().max(1);
    let orig_h = img.height().max(1);
    let (target_w, target_h) = if orig_w > max_width {
        let h = ((orig_h as f64 * max_width as f64) / orig_w as f64) as u32;
        (max_width, h.max(1))
    } else {
        (orig_w, orig_h)
    };

    let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3);

    // Componer sobre fondo blanco ANTES de convertir a grises.
    // to_luma8() ignora el canal alpha → píxeles transparentes quedan en negro (luma=0).
    // Con la composición: pixel_final = pixel * alpha + blanco * (1 - alpha)
    // → transparencia total = blanco, semitransparente = tono claro.
    let rgba = resized.to_rgba8();
    let mut rgb_white = RgbImage::new(target_w, target_h);
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p.0[3] as u32;
        let ia = 255 - a;
        rgb_white.put_pixel(x, y, image::Rgb([
            ((p.0[0] as u32 * a + ia * 255) / 255) as u8,
            ((p.0[1] as u32 * a + ia * 255) / 255) as u8,
            ((p.0[2] as u32 * a + ia * 255) / 255) as u8,
        ]));
    }

    let gray = DynamicImage::from(rgb_white).to_luma8();

    let w = target_w as usize;
    let h = target_h as usize;
    let mut pixels: Vec<f32> = gray.pixels().map(|p| p.0[0] as f32).collect();

    // Floyd-Steinberg dithering
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let old = pixels[idx];
            let new_val = if old < 128.0 { 0.0_f32 } else { 255.0 };
            let err = old - new_val;
            pixels[idx] = new_val;

            if x + 1 < w {
                pixels[idx + 1] = (pixels[idx + 1] + err * 7.0 / 16.0).clamp(0.0, 255.0);
            }
            if y + 1 < h {
                if x > 0 {
                    pixels[(y + 1) * w + x - 1] =
                        (pixels[(y + 1) * w + x - 1] + err * 3.0 / 16.0).clamp(0.0, 255.0);
                }
                pixels[(y + 1) * w + x] =
                    (pixels[(y + 1) * w + x] + err * 5.0 / 16.0).clamp(0.0, 255.0);
                if x + 1 < w {
                    pixels[(y + 1) * w + x + 1] =
                        (pixels[(y + 1) * w + x + 1] + err * 1.0 / 16.0).clamp(0.0, 255.0);
                }
            }
        }
    }

    let raw: Vec<u8> = pixels.iter().map(|&v| v as u8).collect();
    let dithered = GrayImage::from_vec(target_w, target_h, raw)
        .ok_or_else(|| "Error creando buffer dithered".to_string())?;

    let mut png_bytes: Vec<u8> = Vec::new();
    DynamicImage::from(dithered)
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("Error codificando PNG dithered: {e}"))?;

    Ok(png_bytes)
}

/// Codifica una imagen como comandos **ESC \*** (column format, modo 0: 8-dot single density).
///
/// A diferencia de `GS v 0` (raster), este formato es compatible con la gran mayoría
/// de impresoras térmicas de bajo costo (incluidas las micro-printers chinas).
/// Aplica dithering Floyd-Steinberg para máxima calidad en 1bpp.
fn encode_image_escstar(img_bytes: &[u8], max_width_px: u32) -> Result<Vec<u8>, String> {
    use image::{DynamicImage, GrayImage, RgbImage};

    let img = image::load_from_memory(img_bytes)
        .map_err(|e| format!("Error al cargar imagen: {e}"))?;

    // Redimensionar preservando proporción
    let orig_w = img.width().max(1);
    let orig_h = img.height().max(1);
    let (target_w, target_h) = if orig_w > max_width_px {
        let h = ((orig_h as f64 * max_width_px as f64) / orig_w as f64) as u32;
        (max_width_px, h.max(1))
    } else {
        (orig_w, orig_h)
    };
    let resized = img.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3);

    // Componer sobre fondo blanco (maneja transparencia)
    let rgba = resized.to_rgba8();
    let mut rgb_white = RgbImage::new(target_w, target_h);
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p.0[3] as u32;
        let ia = 255 - a;
        rgb_white.put_pixel(x, y, image::Rgb([
            ((p.0[0] as u32 * a + ia * 255) / 255) as u8,
            ((p.0[1] as u32 * a + ia * 255) / 255) as u8,
            ((p.0[2] as u32 * a + ia * 255) / 255) as u8,
        ]));
    }

    let gray = DynamicImage::from(rgb_white).to_luma8();
    let w = target_w as usize;
    let h = target_h as usize;

    // Floyd-Steinberg dithering sobre buffer f32
    let mut pixels: Vec<f32> = gray.pixels().map(|p| p.0[0] as f32).collect();
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let old = pixels[idx];
            let new_val = if old < 128.0 { 0.0_f32 } else { 255.0 };
            let err = old - new_val;
            pixels[idx] = new_val;
            if x + 1 < w { pixels[idx + 1] = (pixels[idx + 1] + err * 7.0 / 16.0).clamp(0.0, 255.0); }
            if y + 1 < h {
                if x > 0 { pixels[(y+1)*w + x-1] = (pixels[(y+1)*w + x-1] + err * 3.0 / 16.0).clamp(0.0, 255.0); }
                pixels[(y+1)*w + x] = (pixels[(y+1)*w + x] + err * 5.0 / 16.0).clamp(0.0, 255.0);
                if x + 1 < w { pixels[(y+1)*w + x+1] = (pixels[(y+1)*w + x+1] + err * 1.0 / 16.0).clamp(0.0, 255.0); }
            }
        }
    }

    let raw: Vec<u8> = pixels.iter().map(|&v| v as u8).collect();
    let dithered = GrayImage::from_vec(target_w, target_h, raw)
        .ok_or_else(|| "Error creando buffer dithered".to_string())?;

    // ── Codificar como ESC * (modo 33: 24-dot double density, 203 DPI) ──────
    let mut out: Vec<u8> = Vec::new();

    // ESC 3 n — fijar avance de línea en unidades de 1/360 pulgada.
    // Queremos avanzar exactamente 24 dots @ 203 DPI:
    //   24 dots = 24/203 inch = 0.1182 inch
    //   0.1182 inch × 360 = 42.56 unidades ≈ 42 unidades
    out.extend_from_slice(&[0x1B, 0x33, 42]);

    let mut y = 0usize;
    while y < h {
        // ESC * 33 nL nH — modo 33: 24-dot double density
        out.push(0x1B);
        out.push(0x2A);
        out.push(33); // 33 = 0x21
        out.push((w & 0xFF) as u8);
        out.push((w >> 8) as u8);

        // 3 bytes por columna: MSB superior, medio, LSB inferior
        for col in 0..w {
            let mut b1: u8 = 0;
            let mut b2: u8 = 0;
            let mut b3: u8 = 0;

            for dot in 0..8usize {
                if y + dot < h && dithered.get_pixel(col as u32, (y + dot) as u32).0[0] == 0 { b1 |= 1 << (7 - dot); }
                if y + 8 + dot < h && dithered.get_pixel(col as u32, (y + 8 + dot) as u32).0[0] == 0 { b2 |= 1 << (7 - dot); }
                if y + 16 + dot < h && dithered.get_pixel(col as u32, (y + 16 + dot) as u32).0[0] == 0 { b3 |= 1 << (7 - dot); }
            }
            out.push(b1);
            out.push(b2);
            out.push(b3);
        }
        out.push(0x0A); // LF — avanza 24 dots
        y += 24;
    }

    // ESC 2 — restaurar espaciado de línea por defecto
    out.extend_from_slice(&[0x1B, 0x32]);

    Ok(out)
}

/// El logo se pre-procesa con dithering Floyd-Steinberg para preservar todos los
/// tonos intermedios (el crate escpos usa umbral fijo sin dithering).
#[tauri::command]
pub fn print_test(printer_name: String, size: String) -> Result<String, String> {
    match resolve_printer_backend(&printer_name)? {
        PrinterBackend::Network(ip, port) => {
            use escpos::{driver::NetworkDriver, printer::Printer, utils::*};
            use std::time::Duration;

            let print_width_px: u32 = match size.as_str() {
                "thermal_80mm" => 576,
                _ => 384,
            };
            let logo_dithered = dither_image_bytes(APP_LOGO, print_width_px / 2)?;

            (|| -> escpos::errors::Result<()> {
                let driver = NetworkDriver::open(&ip, port, Some(Duration::from_secs(5)))?;
                let mut p = Printer::new(driver, Protocol::default(), None);
                p.init()?
                    .justify(JustifyMode::CENTER)?
                    .bit_image_from_bytes_option(
                        &logo_dithered,
                        BitImageOption::new(Some(print_width_px / 2), None, BitImageSize::Normal)?,
                    )?
                    .writeln("")?
                    .bold(true)?
                    .writeln("Printer Monitor")?
                    .bold(false)?
                    .justify(JustifyMode::LEFT)?
                    .writeln(&format!("Impresora: {}", printer_name))?
                    .writeln(&format!("Fecha: {}", chrono::Local::now().format("%d/%m/%Y %H:%M")))?
                    .writeln("")?
                    .writeln("Si ves esto, funciona!")?
                    .feeds(4)?
                    .print_cut()?;
                Ok(())
            })()
            .map_err(|e| format!("Error de impresión: {e}"))?;
        }
        PrinterBackend::CupsRaw(ref queue) => {
            let print_width_px: u32 = match size.as_str() {
                "thermal_80mm" => 576,
                _ => 384,
            };
            let label = match size.as_str() {
                "thermal_80mm" => "Termica 80mm",
                _ => "Termica 58mm",
            };

            // Logo vía ESC * (compatible con micro-printers que no soportan GS v 0)
            let logo_escstar = encode_image_escstar(APP_LOGO, print_width_px / 2)?;

            // Texto vía VecDriver + escpos (funciona correctamente)
            let (driver, shared_buf) = VecDriver::new();
            (|| -> escpos::errors::Result<()> {
                use escpos::{printer::Printer, utils::*};
                let mut p = Printer::new(driver, Protocol::default(), None);
                p.init()?
                    .justify(JustifyMode::CENTER)?
                    .bold(true)?
                    .writeln("Printer Monitor")?
                    .bold(false)?
                    .justify(JustifyMode::LEFT)?
                    .writeln(label)?
                    .writeln(&format!("Cola: {}", queue))?
                    .writeln(&format!("Fecha: {}", chrono::Local::now().format("%d/%m/%Y %H:%M")))?
                    .writeln("")?
                    .writeln("Si ves esto, funciona!")?
                    .feeds(4)?
                    .print_cut()?;
                Ok(())
            })()
            .map_err(|e| format!("Error de impresión: {e}"))?;

            // Combinar: imagen ESC * primero, luego texto
            let mut payload = logo_escstar;
            payload.push(0x0A);
            payload.extend_from_slice(&shared_buf.lock().unwrap());
            print_raw_cups(queue, &payload)?;
        }
    }
    Ok(format!("Prueba enviada a \u{ab}{}\u{bb}", printer_name))
}

/// Cancela todos los trabajos pendientes de la cola de una impresora.
///
/// macOS/Linux: `cancel -a <queue_name>`
/// Windows: `Get-PrintJob | Remove-PrintJob` vía PowerShell
#[tauri::command]
pub fn clear_print_queue(printer_name: String) -> Result<String, String> {
    use std::process::Command;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let output = Command::new("cancel")
            .args(["-a", &printer_name])
            .output()
            .map_err(|e| format!("No se pudo ejecutar cancel: {e}"))?;

        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();

        // `cancel` devuelve exit 1 con "no jobs for X" cuando la cola ya está vacía;
        // eso no es un error real.
        if output.status.success() {
            Ok(format!("Cola de «{}» vaciada", printer_name))
        } else if stderr.contains("no jobs") || stderr.is_empty() {
            Ok(format!("La cola de «{}» ya estaba vacía", printer_name))
        } else {
            Err(format!(
                "Error al limpiar cola: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "Get-PrintJob -PrinterName '{}' -EA SilentlyContinue | Remove-PrintJob -EA SilentlyContinue",
            printer_name.replace('\'', "''")
        );
        let output = crate::hidden_cmd("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ])
            .output()
            .map_err(|e| format!("No se pudo ejecutar PowerShell: {e}"))?;

        if output.status.success() {
            Ok(format!("Cola de «{}» vaciada", printer_name))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.trim().is_empty() {
                Ok(format!("La cola de «{}» ya estaba vacía", printer_name))
            } else {
                Err(format!("Error al limpiar cola: {}", stderr.trim()))
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Sistema operativo no soportado".to_string())
}

// ─── Impresión de tickets ────────────────────────────────────────────────────

/// Imprime un ticket con imagen, título y texto usando `escpos::NetworkDriver`.
///
/// - `image_b64` : PNG o JPEG en base64 (vacío = sin imagen).
/// - `size`      : `"thermal_50mm"` (384 px) | `"thermal_80mm"` (576 px).
#[tauri::command]
pub fn print_image_ticket(
    printer_name: String,
    title: String,
    body_lines: Vec<String>,
    image_b64: String,
    size: String,
) -> Result<String, String> {
    let print_width_px: u32 = match size.as_str() {
        "thermal_80mm" => 576,
        _ => 384,
    };

    let image_bytes: Option<Vec<u8>> = if !image_b64.is_empty() {
        use base64::Engine as _;
        Some(
            base64::engine::general_purpose::STANDARD
                .decode(image_b64.trim())
                .map_err(|e| format!("Error decodificando imagen base64: {e}"))?,
        )
    } else {
        None
    };

    match resolve_printer_backend(&printer_name)? {
        PrinterBackend::Network(ip, port) => {
            use escpos::{driver::NetworkDriver, printer::Printer, utils::*};
            use std::time::Duration;

            (|| -> escpos::errors::Result<()> {
                let driver = NetworkDriver::open(&ip, port, Some(Duration::from_secs(5)))?;
                let mut p = Printer::new(driver, Protocol::default(), None);
                p.init()?;

                if let Some(ref img) = image_bytes {
                    let img_dithered = dither_image_bytes(img, print_width_px)
                        .map_err(|e| escpos::errors::PrinterError::Input(e))?;
                    p.justify(JustifyMode::CENTER)?
                        .bit_image_from_bytes_option(
                            &img_dithered,
                            BitImageOption::new(Some(print_width_px), None, BitImageSize::Normal)?,
                        )?
                        .writeln("")?
                        .justify(JustifyMode::LEFT)?;
                }

                if !title.is_empty() {
                    p.justify(JustifyMode::CENTER)?
                        .bold(true)?
                        .writeln(&title)?
                        .bold(false)?
                        .justify(JustifyMode::LEFT)?;
                }

                for line in &body_lines {
                    p.writeln(line)?;
                }

                p.feeds(4)?.print_cut()?;
                Ok(())
            })()
            .map_err(|e| format!("Error de impresión: {e}"))?;
        }
        PrinterBackend::CupsRaw(ref queue) => {
            // Imagen vía ESC * (compatible con micro-printers que no soportan GS v 0)
            let mut payload: Vec<u8> = Vec::new();
            payload.extend_from_slice(&[0x1B, 0x40]); // ESC @ — init

            if let Some(ref img) = image_bytes {
                let img_escstar = encode_image_escstar(img, print_width_px)?;
                payload.extend_from_slice(&img_escstar);
                payload.push(0x0A);
            }

            // Texto vía VecDriver + escpos (omitimos init para no pisar el ESC @ inicial)
            let (driver, shared_buf) = VecDriver::new();
            (|| -> escpos::errors::Result<()> {
                use escpos::{printer::Printer, utils::*};
                let mut p = Printer::new(driver, Protocol::default(), None);
                // No llamamos init() para no insertar otro ESC @ que resetearía el estado
                if !title.is_empty() {
                    p.justify(JustifyMode::CENTER)?
                        .bold(true)?
                        .writeln(&title)?
                        .bold(false)?
                        .justify(JustifyMode::LEFT)?;
                }
                for line in &body_lines {
                    p.writeln(line)?;
                }
                p.feeds(4)?.print_cut()?;
                Ok(())
            })()
            .map_err(|e| format!("Error de impresión: {e}"))?;

            payload.extend_from_slice(&shared_buf.lock().unwrap());
            print_raw_cups(queue, &payload)?;
        }
    }

    Ok(format!("Enviado a \u{ab}{}\u{bb}", printer_name))
}

/// Backend de conexión para una impresora registrada en el sistema.
enum PrinterBackend {
    /// Impresora de red accesible por TCP socket (URI `socket://ip:puerto`).
    Network(String, u16),
    /// Impresora USB registrada en CUPS (URI `usb://`, `ipp://`, `lpd://`, etc.).
    /// Los datos ESC/POS se envían como trabajo raw vía `lp -d queue -o raw`.
    CupsRaw(String),
}

/// Determina cómo conectar con una impresora CUPS por su nombre de cola.
///
/// - `socket://` → [`PrinterBackend::Network`] (ESC/POS directo por TCP)
/// - `usb://`, `ipp://`, `lpd://` → [`PrinterBackend::CupsRaw`] (job raw vía `lp`)
fn resolve_printer_backend(printer_name: &str) -> Result<PrinterBackend, String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::process::Command;
        let out = Command::new("lpstat")
            .args(["-v", printer_name])
            .output()
            .map_err(|e| format!("No se pudo ejecutar lpstat: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();

        // Impresora de red (socket://)
        if let Some(line) = text.lines().find(|l| l.to_lowercase().contains("socket://")) {
            let uri = line
                .split_whitespace()
                .find(|t| t.to_lowercase().starts_with("socket://"))
                .unwrap_or("");
            let raw = uri
                .trim_start_matches("socket://")
                .trim_end_matches('/')
                .trim_end_matches('\\');
            let (host, port) = if let Some((h, p)) = raw.split_once(':') {
                (h.to_string(), p.parse().unwrap_or(9100))
            } else {
                (raw.to_string(), 9100)
            };
            return Ok(PrinterBackend::Network(host, port));
        }

        // Impresora USB u otro tipo CUPS (usb://, ipp://, lpd://, ...)
        if text.lines().any(|l| {
            let lo = l.to_lowercase();
            lo.contains("usb://") || lo.contains("ipp://") || lo.contains("lpd://")
        }) {
            return Ok(PrinterBackend::CupsRaw(printer_name.to_string()));
        }

        Err(format!(
            "No se encontró dirección para \u{ab}{printer_name}\u{bb}.\n\
             Asegúrate de que la impresora está instalada en CUPS."
        ))
    }

    #[cfg(target_os = "windows")]
    {
        let out = crate::hidden_cmd("powershell")
            .args([
                "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden",
                "-Command",
                &format!(
                    "(Get-PrinterPort -Name (Get-Printer -Name '{}' -EA SilentlyContinue)\
                     .PortName -EA SilentlyContinue).PrinterHostAddress",
                    printer_name.replace('\'', "''")
                ),
            ])
            .output()
            .map_err(|e| format!("No se pudo ejecutar PowerShell: {e}"))?;
        let host = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if host.is_empty() || host.to_lowercase().contains("null") {
            Err(format!("No se encontró la IP de \u{ab}{printer_name}\u{bb}."))
        } else {
            Ok(PrinterBackend::Network(host, 9100))
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err("Sistema operativo no soportado".to_string())
}

/// Envía datos ESC/POS crudos a una cola CUPS.
/// Escribe en un archivo temporal y usa `lp -d queue -o raw`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn print_raw_cups(queue_name: &str, data: &[u8]) -> Result<(), String> {
    let tmp = format!(
        "/private/tmp/pm_escpos_{}.bin",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    std::fs::write(&tmp, data).map_err(|e| format!("Error al crear archivo temporal: {e}"))?;
    let out = std::process::Command::new("lp")
        .args(["-d", queue_name, "-o", "raw", &tmp])
        .output()
        .map_err(|e| format!("Error al ejecutar lp: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Err(format!(
            "Error al imprimir: {}",
            if !stderr.is_empty() { stderr } else { stdout }
        ))
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
        _ => (32usize, "Termica 58mm"),
    };

    let data = build_escpos_test(&port_name, chars_per_line, label);
    write_to_serial_port(&port_name, &data)
}

/// Envía una página de prueba ESC/POS directamente a una IP:9100
/// **sin** registrar la impresora en CUPS. Útil para impresoras encontradas
/// en el escaneo TCP/IP que aún no están instaladas en el sistema.
#[tauri::command]
pub fn print_test_tcp(ip: String, size: String) -> Result<String, String> {
    use escpos::{driver::NetworkDriver, printer::Printer, utils::*};
    use std::time::Duration;

    let print_width_px: u32 = match size.as_str() {
        "thermal_80mm" => 576,
        _ => 384, // thermal_58mm
    };

    let logo_dithered = dither_image_bytes(APP_LOGO, print_width_px / 2)?;

    (|| -> escpos::errors::Result<()> {
        let driver = NetworkDriver::open(&ip, 9100, Some(Duration::from_secs(5)))?;
        let mut p = Printer::new(driver, Protocol::default(), None);
        p.init()?
            .justify(JustifyMode::CENTER)?
            .bit_image_from_bytes_option(
                &logo_dithered,
                BitImageOption::new(Some(print_width_px / 2), None, BitImageSize::Normal)?,
            )?
            .writeln("")?
            .bold(true)?
            .writeln("Printer Monitor")?
            .bold(false)?
            .justify(JustifyMode::LEFT)?
            .writeln(&format!("IP: {}", ip))?
            .writeln(&format!("Fecha: {}", chrono::Local::now().format("%d/%m/%Y %H:%M")))?
            .writeln("")?
            .writeln("Si ves esto, funciona!")?
            .feeds(4)?
            .print_cut()?;
        Ok(())
    })()
    .map_err(|e| format!("Error de impresion: {e}"))?;

    Ok(format!("Prueba enviada a {}:9100", ip))
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

/// Registra una impresora USB en CUPS buscando su URI con `lpinfo -v`.
/// Si hay una sola impresora USB disponible, la registra directamente.
/// Si hay varias, intenta hacer coincidir por `device_name`.
#[tauri::command]
pub fn add_usb_printer(device_name: String, cups_name: String) -> Result<String, String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::process::Command;

        // Obtener lista de URIs de impresoras USB disponibles
        let out = Command::new("lpinfo")
            .arg("-v")
            .output()
            .map_err(|_| "No se pudo ejecutar lpinfo. ¿Está CUPS disponible?".to_string())?;

        let text = String::from_utf8_lossy(&out.stdout).to_string();

        let usb_uris: Vec<String> = text
            .lines()
            .filter(|l| l.contains("usb://"))
            .filter_map(|l| l.split_whitespace().nth(1))
            .map(|s| s.to_string())
            .collect();

        if usb_uris.is_empty() {
            return Err(
                "No se detectaron impresoras USB disponibles. Verifique que la impresora esté conectada y encendida."
                    .to_string(),
            );
        }

        // Intentar hacer coincidir la URI con el nombre del dispositivo
        let search = device_name.to_lowercase().replace(' ', "").replace('-', "");
        let uri = usb_uris
            .iter()
            .find(|uri| {
                let u = uri.to_lowercase().replace("%20", "").replace('+', "").replace('-', "");
                u.contains(&search)
            })
            .unwrap_or(&usb_uris[0])
            .clone();

        // Buscar driver genérico disponible (igual que add_network_printer).
        // NO usar "-m everywhere" para USB: requiere IPP sobre red y falla.
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

        let safe_name = cups_name.replace("'", "\\'");
        let lpadmin_cmd = format!(
            "/usr/sbin/lpadmin -p '{}' -v '{}' -E{}",
            safe_name, uri, driver_flag
        );

        let script_path = "/private/tmp/cups_add_usb.sh";
        std::fs::write(
            script_path,
            format!("#!/bin/sh\nexport HOME=/private/tmp\ncd /private/tmp\n{}\n", lpadmin_cmd),
        )
        .map_err(|e| format!("Error al crear script: {}", e))?;
        let _ = Command::new("chmod").args(["+x", script_path]).output();

        println!("🖨️ USB lpadmin: {}", lpadmin_cmd);

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

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let has_shell_init_warn = stderr.contains("shell-init") || stderr.contains("getcwd");
        let has_real_error = stderr.contains("lpadmin:")
            || stderr.contains("Unable to")
            || stderr.contains("no se ha podido");

        if output.status.success() || (has_shell_init_warn && !has_real_error) {
            Ok(format!("Impresora USB '{}' registrada correctamente en CUPS.", cups_name))
        } else if stderr.contains("User cancelled") || stderr.contains("cancelado") {
            Err("Operación cancelada.".to_string())
        } else {
            Err(format!("Error al registrar: {}", stderr.trim()))
        }
    }

    #[cfg(target_os = "windows")]
    {
        let _ = (device_name, cups_name);
        Err("Agregar impresoras USB en Windows no está disponible desde esta herramienta.".to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// IMPRESIÓN DESDE PDF
// ═══════════════════════════════════════════════════════════════════════════════

/// Recibe un PDF en base64, lo convierte a imagen y lo imprime.
/// `width` acepta `"58mm"` (384 px @ 203 DPI, modo ESC* double density)
/// o `"80mm"` (576 px @ 203 DPI, modo ESC* double density).
pub fn print_pdf_job(pdf_b64: &str, printer_name: &str, width: &str) -> Result<String, String> {
    use base64::Engine as _;

    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(pdf_b64.trim())
        .map_err(|e| format!("Error decodificando PDF: {e}"))?;

    let print_width_px: u32 = if width.contains("58") { 384 } else { 576 };

    let png_bytes = render_pdf_to_png(&pdf_bytes, print_width_px)?;

    // Para depuración, guardamos la imagen generada en Documentos
    if let Ok(home) = std::env::var("HOME") {
        let debug_path = format!("{}/Documents/debug_printer_monitor.png", home);
        let _ = std::fs::write(&debug_path, &png_bytes);
        println!("📸 Imagen debug guardada en: {}", debug_path);
    }

    let escstar = encode_image_escstar(&png_bytes, print_width_px)?;

    let mut payload = vec![0x1B, 0x40u8]; // ESC @
    payload.extend_from_slice(&escstar);
    payload.extend_from_slice(&[0x0A, 0x0A, 0x0A, 0x0A]); // avance
    payload.extend_from_slice(&[0x1D, 0x56, 0x41, 0x00]); // corte

    match resolve_printer_backend(printer_name)? {
        PrinterBackend::CupsRaw(ref queue) => {
            print_raw_cups(queue, &payload)?;
        }
        PrinterBackend::Network(ref ip, port) => {
            use std::io::Write;
            let addr = format!("{ip}:{port}");
            let mut stream = std::net::TcpStream::connect_timeout(
                &addr.parse().map_err(|_| format!("Dirección inválida: {addr}"))?,
                std::time::Duration::from_secs(5),
            )
            .map_err(|e| format!("No se pudo conectar a {addr}: {e}"))?;
            stream.write_all(&payload).map_err(|e| format!("Error enviando datos: {e}"))?;
        }
    }

    Ok(format!("PDF impreso en «{printer_name}»"))
}

/// Comando Tauri: imprime un PDF base64 en la impresora indicada.
/// Usado desde Angular (y equivalente a `POST /api/print` para clientes externos).
#[tauri::command]
pub fn print_pdf(pdf_b64: String, printer_name: String, width: String) -> Result<String, String> {
    print_pdf_job(&pdf_b64, &printer_name, &width)
}

// ─── Helpers internos de conversión PDF ──────────────────────────────────────

fn get_pdfium_lib_path() -> Result<std::path::PathBuf, String> {
    use std::env;
    let mut exe_path = env::current_exe().map_err(|e| e.to_string())?;
    let mut use_cargo_dir = false;
    
    if cfg!(target_os = "macos") && exe_path.to_string_lossy().contains("Contents/MacOS") {
        exe_path.pop(); // MacOS
        exe_path.pop(); // Contents
        exe_path.push("Resources");
    } else if cfg!(target_os = "windows") && !exe_path.to_string_lossy().contains("target") {
        exe_path.pop(); // En Windows (producción) la librería está junto al ejecutable
    } else {
        use_cargo_dir = true;
    }

    let mut final_path = if use_cargo_dir {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    } else {
        exe_path.clone()
    };

    final_path.push("libs");

    #[cfg(target_os = "macos")]
    #[cfg(target_arch = "aarch64")]
    final_path.push("mac-arm64/libpdfium.dylib");

    #[cfg(target_os = "macos")]
    #[cfg(target_arch = "x86_64")]
    final_path.push("mac-x64/libpdfium.dylib");

    #[cfg(target_os = "windows")]
    final_path.push("win-x64/pdfium.dll");

    Ok(final_path)
}

fn render_pdf_to_png(pdf_bytes: &[u8], target_width_px: u32) -> Result<Vec<u8>, String> {
    use pdfium_render::prelude::*;
    use image::{DynamicImage, RgbImage};
    
    let lib_path = get_pdfium_lib_path()?;
    let bind = Pdfium::bind_to_library(lib_path.to_str().unwrap_or_default())
        .or_else(|_| Pdfium::bind_to_system_library())
        .map_err(|e| format!("No se pudo cargar libpdfium ({}): {:?}", lib_path.display(), e))?;
        
    let pdfium = Pdfium::new(bind);
    let document = pdfium.load_pdf_from_byte_slice(pdf_bytes, None)
        .map_err(|e| format!("Error cargando PDF: {:?}", e))?;
        
    let super_width = target_width_px * 4;
    let mut rendered_pages: Vec<DynamicImage> = Vec::new();
    
    for (index, page) in document.pages().iter().enumerate() {
        let width_pt = page.width().value;
        let scale = super_width as f32 / width_pt;
        
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(scale)
            .render_annotations(true)
            .set_clear_color(PdfColor::new(255, 255, 255, 255));
        
        let bitmap = page.render_with_config(&config)
            .map_err(|e| format!("Error renderizando página {}: {:?}", index, e))?;
            
        rendered_pages.push(bitmap.as_image());
    }
    
    if rendered_pages.is_empty() {
        return Err("El PDF no tiene páginas.".to_string());
    }
    
    // Normalizamos el ancho (por si las páginas tienen tamaños diferentes)
    for img in &mut rendered_pages {
        if img.width() != super_width {
            *img = img.resize_exact(
                super_width,
                (img.height() as f64 * super_width as f64 / img.width().max(1) as f64) as u32,
                image::imageops::FilterType::Lanczos3,
            );
        }
    }
    
    let total_h: u32 = rendered_pages.iter().map(|i| i.height()).sum();
    let mut canvas = RgbImage::new(super_width, total_h);
    for p in canvas.pixels_mut() {
        *p = image::Rgb([255, 255, 255]);
    }
    
    let mut y_off = 0u32;
    for img in &rendered_pages {
        for (x, y, p) in img.to_rgb8().enumerate_pixels() {
            canvas.put_pixel(x, y + y_off, *p);
        }
        y_off += img.height();
    }
    
    let mut out = Vec::new();
    DynamicImage::from(canvas)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("Error codificando imagen final: {}", e))?;
        
    Ok(out)
}

/// Recorta las filas completamente blancas del final de la imagen (elimina
/// el espacio en blanco sobrante del PDF que pdfmake genera con altura fija).
fn trim_white_rows_bottom(png_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(png_bytes)
        .map_err(|e| format!("Error cargando imagen: {e}"))?;
    let gray = img.to_luma8();
    let (w, h) = (gray.width(), gray.height());

    // Última fila con al menos un píxel no-blanco
    let mut last_y = 0u32;
    for y in 0..h {
        if (0..w).any(|x| gray.get_pixel(x, y).0[0] < 248) {
            last_y = y;
        }
    }

    let keep_h = (last_y + 24).min(h);
    if keep_h >= h {
        return Ok(png_bytes.to_vec()); // nada que recortar
    }

    let mut out = Vec::new();
    img.crop_imm(0, 0, w, keep_h)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("Error codificando PNG recortado: {e}"))?;
    Ok(out)
}


#[cfg(target_os = "windows")]
fn print_raw_cups(_queue_name: &str, _data: &[u8]) -> Result<(), String> {
    Err("Las colas Raw USB usando enrutador del sistema aun no estan soportadas en esta actualizacion de Windows. Enlaza tu termica via Red (TCP/IP).".to_string())
}
