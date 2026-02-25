use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub local_ip: String,
    pub port: u16,
    pub printers: Vec<PrinterInfo>,
}

/// Obtiene las impresoras del sistema usando lpstat (macOS/Linux) o wmic (Windows)
#[tauri::command]
fn get_printers() -> Vec<PrinterInfo> {
    let mut printers: Vec<PrinterInfo> = Vec::new();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::process::Command;

        let default_printer = Command::new("lpstat")
            .args(["-d"])
            .output()
            .ok()
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout).to_string();
                out.split(':').last().map(|s| s.trim().to_string())
            })
            .unwrap_or_default();

        if let Ok(output) = Command::new("lpstat").args(["-p"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.starts_with("printer ") {
                    let parts: Vec<&str> = line.splitn(3, ' ').collect();
                    if parts.len() >= 2 {
                        let name = parts[1].to_string();
                        let status = if line.contains("idle") {
                            "Disponible".to_string()
                        } else if line.contains("disabled") {
                            "Deshabilitada".to_string()
                        } else {
                            "Imprimiendo".to_string()
                        };
                        let is_default = name == default_printer;
                        printers.push(PrinterInfo { name, is_default, status });
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        if let Ok(output) = Command::new("wmic")
            .args(["printer", "get", "Name,Default,PrinterStatus", "/format:csv"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(2) {
                let cols: Vec<&str> = line.split(',').collect();
                if cols.len() >= 4 {
                    let is_default = cols[1].trim().eq_ignore_ascii_case("TRUE");
                    let name = cols[2].trim().to_string();
                    let status_code = cols[3].trim();
                    let status = match status_code {
                        "3" => "Disponible".to_string(),
                        "4" => "Imprimiendo".to_string(),
                        "5" => "Calentando".to_string(),
                        _ => "Desconocido".to_string(),
                    };
                    if !name.is_empty() {
                        printers.push(PrinterInfo { name, is_default, status });
                    }
                }
            }
        }
    }

    printers
}

/// Obtiene la IP local de la máquina
#[tauri::command]
fn get_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "No disponible".to_string())
}

/// Puerto en el que corre la app (fijo 4200)
#[tauri::command]
fn get_app_port() -> u16 {
    4200
}

/// Devuelve toda la info del sistema en un solo comando
#[tauri::command]
fn get_system_info() -> SystemInfo {
    SystemInfo {
        local_ip: get_local_ip(),
        port: get_app_port(),
        printers: get_printers(),
    }
}

/// Genera un PDF de prueba en memoria y lo imprime con `lp` (macOS/Linux) o `print` (Windows)
#[tauri::command]
fn print_test(printer_name: String, size: String) -> Result<String, String> {
    use std::io::Write;
    use std::process::Command;

    // Dimensiones en puntos PDF (1 pt = 1/72 inch)
    // A4: 595 x 842 pt
    // Térmica 50mm: 142 x 200 pt (~50mm ancho, recibo corto)
    // Térmica 80mm: 227 x 200 pt (~80mm ancho, recibo corto)
    let (page_width, page_height, label) = match size.as_str() {
        "thermal_50mm" => (142u32, 200u32, "Térmica 50mm"),
        "thermal_80mm" => (227u32, 200u32, "Térmica 80mm"),
        _ => (595u32, 842u32, "A4"),
    };

    // Genera un PDF mínimo válido en memoria
    let title = format!("Página de prueba — {}", label);
    let body_lines = vec![
        format!("Printer Monitor — Prueba de impresión"),
        format!("Impresora: {}", printer_name),
        format!("Formato:   {}", label),
        format!("Fecha:     {}", chrono::Local::now().format("%d/%m/%Y %H:%M:%S")),
        String::from(""),
        String::from("Si ves este texto, la impresora"),
        String::from("funciona correctamente. ✓"),
    ];

    let pdf_bytes = build_test_pdf(page_width, page_height, &title, &body_lines);

    // Escribe el PDF en un archivo temporal
    let tmp_path = std::env::temp_dir().join(format!("pm_test_{}.pdf", printer_name.replace(' ', "_")));
    {
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("No se pudo crear archivo temporal: {e}"))?;
        f.write_all(&pdf_bytes)
            .map_err(|e| format!("No se pudo escribir PDF: {e}"))?;
    }

    // Envía a la impresora
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
            let err = String::from_utf8_lossy(&output.stderr);
            Err(format!("Error de impresión: {}", err.trim()))
        }
    }

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("cmd")
            .args(["/C", "print", &format!("/D:{}", printer_name), tmp_path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| format!("Error al ejecutar print: {e}"))?;

        let _ = std::fs::remove_file(&tmp_path);

        if output.status.success() {
            Ok(format!("Trabajo enviado a «{}» ({})", printer_name, label))
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            Err(format!("Error de impresión: {}", err.trim()))
        }
    }
}

/// Construye un PDF mínimo válido (PDF 1.4) sin dependencias externas
fn build_test_pdf(width: u32, height: u32, title: &str, lines: &[String]) -> Vec<u8> {
    // Construimos el PDF manualmente siguiendo la especificación mínima
    let mut objects: Vec<String> = Vec::new();

    // Objeto 1: Catálogo
    objects.push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj".to_string());

    // Objeto 2: Pages
    objects.push("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj".to_string());

    // Construir contenido de la página
    let font_size_title = if width < 200 { 9 } else { 14 };
    let font_size_body  = if width < 200 { 7 } else { 11 };
    let margin          = if width < 200 { 8.0 } else { 50.0 };
    let line_height     = (font_size_body as f32) * 1.6;
    let start_y         = (height as f32) - margin - (font_size_title as f32) - 10.0;

    let mut stream = String::new();
    stream.push_str("BT\n");
    // Título
    stream.push_str(&format!("/F1 {} Tf\n", font_size_title));
    stream.push_str(&format!("{} {} Td\n", margin, start_y));
    stream.push_str(&format!("({}) Tj\n", escape_pdf_string(title)));
    // Línea separadora (guiones)
    let sep_count = ((width as f32 - margin * 2.0) / (font_size_body as f32 * 0.5)) as usize;
    let separator = "-".repeat(sep_count.min(60));
    stream.push_str(&format!("/F1 {} Tf\n", font_size_body));
    stream.push_str(&format!("0 -{} Td\n", line_height * 1.2));
    stream.push_str(&format!("({}) Tj\n", separator));
    // Líneas de cuerpo
    for line in lines {
        stream.push_str(&format!("0 -{} Td\n", line_height));
        stream.push_str(&format!("({}) Tj\n", escape_pdf_string(line)));
    }
    stream.push_str("ET\n");

    let stream_bytes = stream.as_bytes().len();

    // Objeto 4: Contenido de la página
    let content_obj = format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj",
        stream_bytes, stream
    );
    objects.push(content_obj);

    // Objeto 5: Fuente
    objects.push(
        "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj"
            .to_string(),
    );

    // Objeto 3: Page (usa recursos y contenido)
    let page_obj = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj",
        width, height
    );
    // Insertamos Page en posición correcta (índice 2)
    objects.insert(2, page_obj);

    // Ensamblamos el PDF
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");

    let mut offsets: Vec<usize> = Vec::new();
    for obj in &objects {
        offsets.push(pdf.len());
        pdf.extend_from_slice(obj.as_bytes());
        pdf.push(b'\n');
    }

    // xref
    let xref_offset = pdf.len();
    let xref_count = objects.len() + 1;
    let mut xref = format!("xref\n0 {}\n", xref_count);
    xref.push_str("0000000000 65535 f \n");
    for &off in &offsets {
        xref.push_str(&format!("{:010} 00000 n \n", off));
    }
    pdf.extend_from_slice(xref.as_bytes());

    // trailer
    let trailer = format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        xref_count, xref_offset
    );
    pdf.extend_from_slice(trailer.as_bytes());

    pdf
}

fn escape_pdf_string(s: &str) -> String {
    // Convierte caracteres UTF-8 a ASCII con escape básico para PDF strings
    s.chars()
        .map(|c| match c {
            '(' => r"\(".to_string(),
            ')' => r"\)".to_string(),
            '\\' => r"\\".to_string(),
            c if c.is_ascii() => c.to_string(),
            // Para caracteres no-ASCII, usa '?' como fallback
            _ => "?".to_string(),
        })
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_printers,
            get_local_ip,
            get_app_port,
            get_system_info,
            print_test,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
