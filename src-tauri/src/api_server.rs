use axum::{extract::Json, http::{HeaderMap, StatusCode}, routing::{get, post}, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use once_cell::sync::Lazy;
use crate::settings::get_custom_printer;

/// Token generado una sola vez al arranque — solo accesible a procesos del mismo usuario.
static API_TOKEN: Lazy<String> = Lazy::new(|| {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", ts ^ 0xDEAD_CAFE_1337_u128)
});

#[cfg(target_os = "windows")]
use pdfium_render::prelude::*;

#[derive(Deserialize)]
struct PrintRequest {
    printer: String,
    pdf_b64: String,
    width: String,
}

#[derive(Serialize)]
struct PrintResponse {
    ok: bool,
    message: String,
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    version: &'static str,
}

pub async fn start() {
    let port = crate::settings::get_server_port();
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Puerto {} ya en uso, servidor HTTP no iniciado: {e}", port);
            return;
        }
    };
    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/api/print", post(handle_print));
    log::info!("API server escuchando en http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}

async fn handle_health() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse { ok: true, version: env!("CARGO_PKG_VERSION") }),
    )
}

async fn handle_print(headers: HeaderMap, Json(req): Json<PrintRequest>) -> (StatusCode, Json<PrintResponse>) {
    let token = headers
        .get("x-pm-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if token != API_TOKEN.as_str() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(PrintResponse { ok: false, message: "Token inválido.".to_string() }),
        );
    }

    match print_pdf_job(&req.pdf_b64, &req.printer, &req.width) {
        Ok(msg) => (StatusCode::OK, Json(PrintResponse { ok: true, message: msg })),
        Err(e) => (StatusCode::OK, Json(PrintResponse { ok: false, message: e })),
    }
}

pub fn print_pdf_job(pdf_b64: &str, printer_name: &str, width: &str) -> Result<String, String> {
    let pdf_bytes = base64_decode(pdf_b64)?;
    print_pdf_bytes_job(&pdf_bytes, printer_name, width)
}

pub fn print_internal_test_pdf(printer_name: &str, width: &str) -> Result<String, String> {
    let pdf = generate_test_pdf_bytes(width);
    print_pdf_bytes_job(&pdf, printer_name, width)
}

pub fn print_pdf_bytes_job(pdf_bytes: &[u8], printer_name: &str, width: &str) -> Result<String, String> {
    if let Some(cp) = get_custom_printer(printer_name).map_err(|e| e.to_string())? {
        // Camino app: PDF → bitmap → ESC/POS → TCP o puerto USB
        if cp.connection_type == "network" || cp.connection_type == "usb_app" {
            return print_pdf_to_escpos_app(pdf_bytes, &cp, width);
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("lp")
            .args(["-d", printer_name, "-"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(pdf_bytes).map_err(|e| e.to_string())?;
        }
        child.wait().map_err(|e| e.to_string())?;
        Ok(format!("PDF enviado a '{printer_name}' [{width}]"))
    }
    #[cfg(target_os = "windows")]
    {
        print_pdf_windows_so(pdf_bytes, printer_name, width)
    }
}

#[cfg(target_os = "windows")]
fn print_pdf_windows_so(pdf_bytes: &[u8], printer_name: &str, width: &str) -> Result<String, String> {
    // Impresoras térmicas (driver "Generic / Text Only") → ESC/POS raw al spooler.
    // Impresoras estándar (PDF virtual, laser, etc.) → GDI directo al printer DC:
    // el pipeline nativo activa el diálogo de guardado para "Microsoft Print to PDF".
    if is_thermal_escpos_printer(printer_name) {
        let escpos = crate::escpos_print::pdf_to_escpos(pdf_bytes, width)?;
        crate::escpos_print::send_raw_to_windows_queue(printer_name, &escpos)?;
        Ok(format!("PDF impreso (ESC/POS) en '{printer_name}' [{width}]"))
    } else {
        crate::escpos_print::pdf_to_gdi_printer(pdf_bytes, printer_name, width)
    }
}

/// Devuelve `true` si la impresora usa driver ESC/POS (Generic / Text Only).
/// Consulta el driver via Win32; si falla, aplica heurística por nombre.
#[cfg(target_os = "windows")]
fn is_thermal_escpos_printer(printer_name: &str) -> bool {
    match get_printer_driver_name(printer_name) {
        Some(driver) => {
            let d = driver.to_ascii_lowercase();
            d.contains("generic") || d.contains("text only")
        }
        // No se pudo leer el driver: heurística por nombre
        None => {
            let n = printer_name.to_ascii_lowercase();
            !n.contains("pdf") && !n.contains("xps")
                && !n.contains("onenote") && !n.contains("fax")
        }
    }
}

/// Lee el nombre del driver de una impresora del SO via Win32 GetPrinterW (level 2).
#[cfg(target_os = "windows")]
fn get_printer_driver_name(printer_name: &str) -> Option<String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Printing::{
        ClosePrinter, GetPrinterW, OpenPrinterW, PRINTER_INFO_2W,
    };
    use windows::core::PCWSTR;

    let name_w: Vec<u16> = printer_name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut handle = HANDLE::default();

    unsafe {
        if OpenPrinterW(PCWSTR(name_w.as_ptr()), &mut handle, None).is_err() {
            return None;
        }

        // Primera llamada: obtener tamaño del buffer requerido
        let mut needed: u32 = 0;
        let _ = GetPrinterW(handle, 2, None, &mut needed as *mut u32);
        if needed == 0 {
            let _ = ClosePrinter(handle);
            return None;
        }

        // Buffer alineado a 8 bytes para que PRINTER_INFO_2W sea accesible
        let aligned_words = (needed as usize + 7) / 8;
        let mut buf: Vec<u64> = vec![0u64; aligned_words];
        let buf_slice = std::slice::from_raw_parts_mut(
            buf.as_mut_ptr() as *mut u8,
            needed as usize,
        );
        let result = GetPrinterW(handle, 2, Some(buf_slice), &mut needed as *mut u32);
        let _ = ClosePrinter(handle);

        if result.is_err() {
            return None;
        }

        let info = &*(buf.as_ptr() as *const PRINTER_INFO_2W);
        let drv_ptr = info.pDriverName.0 as *const u16;
        if drv_ptr.is_null() {
            return None;
        }

        let mut len = 0usize;
        while *drv_ptr.add(len) != 0 {
            len += 1;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(drv_ptr, len)))
    }
}

/// Carga PDFium. Función pública para ser usada desde otros módulos (ej. escpos_print).
#[cfg(target_os = "windows")]
pub fn load_pdfium() -> Result<Pdfium, String> {
    let dll_path = find_pdfium_dll().ok_or_else(|| {
        "No se encontró pdfium.dll. Para incrustarlo en la app colócalo en 'src-tauri/resources/pdfium.dll' y recompila. "
            .to_string()
            + "En desarrollo también funciona en './resources/pdfium.dll', './tools/pdfium.dll' o './bin/pdfium.dll'."
    })?;

    // pdfium_platform_library_name_at_path espera el DIRECTORIO que contiene el DLL,
    // no la ruta completa al archivo. Pasarle la ruta completa genera algo como
    // "…\pdfium.dll\pdfium.dll" que produce LoadLibraryExW error 126.
    let dll_dir = dll_path
        .parent()
        .ok_or_else(|| "No se pudo obtener el directorio de pdfium.dll".to_string())?;

    let lib_path =
        Pdfium::pdfium_platform_library_name_at_path(dll_dir.to_string_lossy().as_ref());

    let bindings = Pdfium::bind_to_library(lib_path)
        .map_err(|e| format!("No se pudo cargar pdfium.dll desde '{}': {e}", dll_path.display()))?;

    Ok(Pdfium::new(bindings))
}

#[cfg(target_os = "windows")]
fn find_pdfium_dll() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("pdfium.dll"));
            candidates.push(dir.join("resources").join("pdfium.dll"));
            candidates.push(dir.join("Resources").join("pdfium.dll"));
            candidates.push(dir.join("resources").join("pdfium").join("pdfium.dll"));
            candidates.push(dir.join("tools").join("pdfium.dll"));
            candidates.push(dir.join("bin").join("pdfium.dll"));
        }
    }

    // Soporte durante desarrollo desde src-tauri
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("resources").join("pdfium.dll"));
        candidates.push(cwd.join("src-tauri").join("resources").join("pdfium.dll"));
        candidates.push(cwd.join("libs").join("win-x64").join("pdfium.dll"));
        candidates.push(cwd.join("target").join("debug").join("pdfium.dll"));
    }

    candidates.into_iter().find(|p| p.exists())
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input.trim())
        .map_err(|e| format!("PDF base64 inválido: {e}"))
}

pub fn generate_test_pdf_bytes(width: &str) -> Vec<u8> {
    // Parámetros tipográficos según ancho de papel.
    // Para 50mm: 141.73pt de ancho → fuente 7pt con margen 3pt cabe ~38 chars/línea.
    // Para 80mm: 226.77pt de ancho → fuente 9pt con margen 6pt cabe ~47 chars/línea.
    let (w_mm, font_pt, margin_x, line_h) = match width {
        "58mm" => (50.0_f32, 7.0_f32, 3.0_f32, 10.0_f32),
        _      => (80.0_f32, 9.0_f32, 6.0_f32, 13.0_f32),
    };
    let h_mm = 80.0_f32;
    let w_pt = mm_to_pt(w_mm);
    let h_pt = mm_to_pt(h_mm);

    // Líneas cortas que caben en el ancho disponible de cada formato.
    let lines_narrow: &[&str] = &[
        "PRINTER MONITOR",
        "Pagina de prueba",
        width,
        "PDF -> ESC/POS -> RAW",
        "--------------------------",
        "Ticket correcto.",
        "Ruta PDF del SO: OK",
    ];
    let lines_wide: &[&str] = &[
        "PRINTER MONITOR - TEST PDF",
        "Documento interno de prueba",
        width,
        "PDF -> ESC/POS -> RAW OK",
        "--------------------------------",
        "Si este ticket se ve nitido,",
        "la ruta PDF del SO esta OK.",
    ];
    let lines = if w_mm <= 58.0 { lines_narrow } else { lines_wide };

    let mut y = h_pt - 10.0;
    let mut content = String::new();
    content.push_str(&format!("BT\n/F1 {font_pt:.1} Tf\n"));
    for line in lines {
        let escaped = pdf_escape_text(line);
        content.push_str(&format!("1 0 0 1 {margin_x:.1} {y:.2} Tm\n({escaped}) Tj\n"));
        y -= line_h;
    }
    content.push_str("ET\n");

    build_pdf_single_page(w_pt, h_pt, &content)
}

fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

fn pdf_escape_text(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Imprime PDF en una impresora App (network o usb_app).
/// Rasteriza con PDFium, aplica dithering Floyd-Steinberg y envía GS v 0 ESC/POS.
#[cfg(target_os = "windows")]
fn print_pdf_to_escpos_app(
    pdf_bytes: &[u8],
    cp: &crate::settings::CustomPrinter,
    width: &str,
) -> Result<String, String> {
    let data = crate::escpos_print::pdf_to_escpos(pdf_bytes, width)?;
    match cp.connection_type.as_str() {
        "network" => {
            let ip = cp
                .address
                .split(':')
                .next()
                .ok_or("Dirección TCP inválida".to_string())?;
            crate::escpos_print::send_escpos_tcp(ip, 9100, &data)
        }
        "usb_app" => {
            let port = crate::serial::resolve_usb_port(&cp.address)
                .ok_or("No se encontró puerto USB disponible".to_string())?;
            if port != cp.address {
                let _ = crate::settings::update_custom_printer_address(&cp.alias, &port);
            }
            if port.to_ascii_uppercase().starts_with("USB") {
                crate::escpos_print::send_escpos_to_usb_spooler_port(&port, &data)
            } else {
                crate::escpos_print::send_escpos_to_port(&port, &data)
            }
        }
        other => Err(format!("Tipo de conexión no soportado para App: {other}")),
    }
}

/// Stub para plataformas no-Windows (impresoras App no disponibles).
#[cfg(not(target_os = "windows"))]
fn print_pdf_to_escpos_app(
    _pdf_bytes: &[u8],
    _cp: &crate::settings::CustomPrinter,
    _width: &str,
) -> Result<String, String> {
    Err("Impresión App ESC/POS no disponible en esta plataforma.".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// PDF A4 rico para verificar el pipeline de escalado
// ─────────────────────────────────────────────────────────────────────────────

/// Genera un PDF A4 con colores, tabla, barras, patron QR, geometria vectorial
/// y texto a multiples tamanios, para verificar que el pipeline
/// PDF -> PDFium -> set_target_width -> Floyd-Steinberg -> ESC/POS
/// produce un ticket correcto al imprimir en papel termico de 50mm o 80mm.
pub fn generate_a4_test_pdf_bytes() -> Vec<u8> {
    use std::fmt::Write as FmtWrite;
    let w_pt = 595.28_f32;
    let h_pt = 841.89_f32;
    let mut cs = String::new();

    macro_rules! w {
        ($($t:tt)*) => { let _ = write!(cs, $($t)*); };
    }

    // ── 1. Barra de cabecera azul ────────────────────────────────────────
    w!("q\n0.15 0.35 0.65 rg\n0 800 595.28 41.89 re f\nQ\n");
    w!("BT\n/F1 13 Tf\n1 1 1 rg\n1 0 0 1 20 815 Tm\n(TEST A4 - VERIFICAR ESCALADO A PAPEL TERMICO) Tj\nET\n");
    w!("BT\n/F1 7 Tf\n0.8 0.9 1.0 rg\n1 0 0 1 20 804 Tm\n(A4: 595x842 pt. PDFium -> set_target_width -> Floyd-Steinberg -> GS v 0 ESC/POS -> Spooler RAW) Tj\nET\n");

    // ── 2. Paleta RGB + degradado de grises ───────────────────────────────
    w!("BT\n/F1 9 Tf\n0.15 0.35 0.65 rg\n1 0 0 1 20 791 Tm\n(1. PALETA RGB - Conversion a escala de grises) Tj\nET\n");
    let swatches: &[(f32, f32, f32, &str)] = &[
        (0.90, 0.10, 0.10, "Rojo"),
        (0.10, 0.75, 0.10, "Verde"),
        (0.10, 0.10, 0.90, "Azul"),
        (0.90, 0.80, 0.00, "Amarillo"),
        (0.00, 0.75, 0.80, "Cian"),
        (0.75, 0.00, 0.80, "Magenta"),
    ];
    for (i, &(r, g, b, _)) in swatches.iter().enumerate() {
        let x = 20.0_f32 + i as f32 * 58.0;
        w!("q\n{r:.2} {g:.2} {b:.2} rg\n{x:.1} 748 50 32 re f\nQ\n");
    }
    w!("BT\n/F1 6 Tf\n0 0 0 rg\n");
    for (i, &(_, _, _, lbl)) in swatches.iter().enumerate() {
        let tx = 20.0_f32 + i as f32 * 58.0 + 4.0;
        w!("1 0 0 1 {tx:.1} 742 Tm ({lbl}) Tj\n");
    }
    w!("ET\n");
    for i in 0..=10_usize {
        let x = 378.0_f32 + i as f32 * 17.5;
        let g = i as f32 / 10.0;
        w!("q\n{g:.2} {g:.2} {g:.2} rg\n{x:.1} 748 15 32 re f\nQ\n");
    }
    w!("BT\n/F1 6 Tf\n0.4 0.4 0.4 rg\n1 0 0 1 378 742 Tm\n(Negro -> Blanco) Tj\nET\n");

    // ── 3. Tabla de datos ─────────────────────────────────────────────────
    w!("BT\n/F1 9 Tf\n0.15 0.35 0.65 rg\n1 0 0 1 20 733 Tm\n(2. TABLA DE DATOS) Tj\nET\n");
    let ttx = 20.0_f32;
    let ttw = 555.0_f32;
    let trh = 16.0_f32;
    let tn  = 5_usize;
    let tty = 651.0_f32;
    let tc1 = ttx + 200.0;
    let tc2 = ttx + 390.0;
    let hdr_y = tty + (tn - 1) as f32 * trh;
    w!("q\n0.20 0.40 0.70 rg\n{ttx} {hdr_y:.1} {ttw} {trh} re f\nQ\n");
    w!("q\n0.1 0.1 0.1 RG\n0.6 w\n{ttx} {tty:.1} {ttw} {:.1} re S\nQ\n", tn as f32 * trh);
    for r in 1..tn {
        let ry = tty + r as f32 * trh;
        w!("q\n0.7 0.7 0.7 RG\n0.3 w\n{ttx} {ry:.1} m {:.1} {ry:.1} l S\nQ\n", ttx + ttw);
    }
    for &cx in &[tc1, tc2] {
        w!("q\n0.7 0.7 0.7 RG\n0.3 w\n{cx} {tty:.1} m {cx} {:.1} l S\nQ\n", tty + tn as f32 * trh);
    }
    let rows: &[(&str, &str, &str)] = &[
        ("Campo", "Valor", "Estado"),
        ("Papel termico 50mm", "576 dots (12 dots/mm)", "OK"),
        ("Papel termico 80mm", "832 dots (12 dots/mm)", "OK"),
        ("PDF fuente", "A4 595x842 pt", "Esta pagina"),
        ("Dithering", "Floyd-Steinberg 1-bit", "Activado"),
    ];
    w!("BT\n");
    for (ri, &(f1, f2, f3)) in rows.iter().enumerate() {
        let fy = tty + (tn - 1 - ri) as f32 * trh + 4.5;
        let (color, sz) = if ri == 0 { ("1 1 1", "8.5") } else { ("0 0 0", "8") };
        w!("/F1 {sz} Tf\n{color} rg\n");
        w!("1 0 0 1 {:.1} {fy:.1} Tm ({f1}) Tj\n", ttx + 4.0);
        w!("1 0 0 1 {:.1} {fy:.1} Tm ({f2}) Tj\n", tc1 + 4.0);
        w!("1 0 0 1 {:.1} {fy:.1} Tm ({f3}) Tj\n", tc2 + 4.0);
    }
    w!("ET\n");

    // ── 4. Patron de barras ────────────────────────────────────────────────
    w!("BT\n/F1 9 Tf\n0.15 0.35 0.65 rg\n1 0 0 1 20 641 Tm\n(3. PATRON DE BARRAS - prueba de rasterizado) Tj\nET\n");
    let bars: &[u32] = &[3,1,1,2,1,3,1,1,2,1,4,1,2,1,1,3,1,2,1,1,4,1,1,2,3,1,2,1,1,3,1,2,2,1,4,1,1,2,1,3];
    let bh_v  = 28.0_f32;
    let by_v  = 609.0_f32;
    let bu_v  = 3.0_f32;
    let gu_v  = 2.0_f32;
    let mut bx_v = 20.0_f32;
    w!("q\n0 0 0 rg\n");
    for (idx, &bw) in bars.iter().enumerate() {
        let bw_pt = bw as f32 * bu_v;
        if idx % 2 == 0 {
            w!("{bx_v:.1} {by_v} {bw_pt:.1} {bh_v} re f\n");
        }
        bx_v += if idx % 2 == 0 { bw as f32 * bu_v } else { bw as f32 * gu_v };
    }
    w!("Q\n");
    w!("q\n0.4 0.4 0.4 RG\n0.4 w\n18 {:.1} {:.1} {:.1} re S\nQ\n",
        by_v - 2.0, bx_v - 18.0 + 4.0, bh_v + 4.0);

    // ── 5. Patron QR ──────────────────────────────────────────────────────
    w!("BT\n/F1 9 Tf\n0.15 0.35 0.65 rg\n1 0 0 1 330 641 Tm\n(4. PATRON QR - microdetalle) Tj\nET\n");
    let finder: &[[u8; 7]] = &[
        [1,1,1,1,1,1,1],
        [1,0,0,0,0,0,1],
        [1,0,1,1,1,0,1],
        [1,0,1,0,1,0,1],
        [1,0,1,1,1,0,1],
        [1,0,0,0,0,0,1],
        [1,1,1,1,1,1,1],
    ];
    let cell = 7.5_f32;
    let (qx0, qy0) = (330.0_f32, 568.0_f32);
    w!("q\n0 0 0 rg\n");
    for (row, cols) in finder.iter().enumerate() {
        for (col, &v) in cols.iter().enumerate() {
            if v == 1 {
                let cx = qx0 + col as f32 * cell;
                let cy = qy0 + (6 - row) as f32 * cell;
                w!("{cx:.1} {cy:.1} {cell} {cell} re f\n");
            }
        }
    }
    let qx1 = qx0 + 80.0;
    for (row, cols) in finder.iter().enumerate() {
        for (col, &v) in cols.iter().enumerate() {
            if v == 1 {
                let cx = qx1 + col as f32 * cell;
                let cy = qy0 + (6 - row) as f32 * cell;
                w!("{cx:.1} {cy:.1} {cell} {cell} re f\n");
            }
        }
    }
    let qdata: &[u8] = &[1,0,1,0,1,0,1,1,0,1,0,0,1,1,0,1,1,0,0,1,0,1,1,0,1,0,0,1];
    let qxd = qx0 + 57.0;
    for (i, &v) in qdata.iter().enumerate() {
        if v == 1 && (i / 2) < 7 {
            let cx = qxd + (i % 2) as f32 * cell;
            let cy = qy0 + (6 - i / 2) as f32 * cell;
            w!("{cx:.1} {cy:.1} {cell} {cell} re f\n");
        }
    }
    w!("Q\n");

    // ── 6. Geometria vectorial ─────────────────────────────────────────────
    w!("BT\n/F1 9 Tf\n0.15 0.35 0.65 rg\n1 0 0 1 20 598 Tm\n(5. GEOMETRIA VECTORIAL) Tj\nET\n");
    // Abanico de lineas
    w!("q\n0.10 0.20 0.55 RG\n0.6 w\n");
    let (fx, fy_v) = (20.0_f32, 558.0_f32);
    for i in 0..=14_u32 {
        let t  = i as f32 / 14.0;
        let ex = fx + 130.0;
        let ey = fy_v - 38.0 + 76.0 * t;
        w!("{fx} {fy_v:.1} m {ex:.1} {ey:.1} l S\n");
    }
    w!("Q\n");
    // Rectangulos concentricos (degradado azul)
    let blues: &[(f32, f32, f32)] = &[
        (0.12, 0.30, 0.62),
        (0.22, 0.48, 0.78),
        (0.38, 0.65, 0.90),
        (0.62, 0.82, 0.97),
        (0.85, 0.93, 1.00),
    ];
    for (i, &(r, g, b)) in blues.iter().enumerate() {
        let pad = i as f32 * 9.0;
        let rx  = 220.0 + pad;
        let ry  = 513.0 + pad;
        let rw  = 170.0 - pad * 2.0;
        let rh2 = 62.0  - pad * 2.0;
        if rw > 4.0 && rh2 > 4.0 {
            w!("q\n{r:.2} {g:.2} {b:.2} rg\n{rx:.1} {ry:.1} {rw:.1} {rh2:.1} re f\nQ\n");
        }
    }
    w!("BT\n/F1 7 Tf\n1 1 1 rg\n1 0 0 1 228 543 Tm\n(Rectangulos) Tj\nET\n");
    // Circulo (aproximacion bezier)
    let (cx3, cy3, cr3) = (450.0_f32, 542.0_f32, 27.0_f32);
    let k3 = 0.5523_f32;
    w!("q\n0.78 0.15 0.08 rg\n");
    w!("{:.2} {:.2} m \
        {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
        {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
        {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
        {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c f\nQ\n",
        cx3, cy3 + cr3,
        cx3 + cr3*k3, cy3 + cr3, cx3 + cr3, cy3 + cr3*k3, cx3 + cr3, cy3,
        cx3 + cr3, cy3 - cr3*k3, cx3 + cr3*k3, cy3 - cr3, cx3, cy3 - cr3,
        cx3 - cr3*k3, cy3 - cr3, cx3 - cr3, cy3 - cr3*k3, cx3 - cr3, cy3,
        cx3 - cr3, cy3 + cr3*k3, cx3 - cr3*k3, cy3 + cr3, cx3, cy3 + cr3,
    );
    w!("BT\n/F1 7 Tf\n0.78 0.15 0.08 rg\n1 0 0 1 436 537 Tm\n(Circulo) Tj\nET\n");

    // ── 7. Texto a multiples tamanios ────────────────────────────────────
    w!("BT\n/F1 9 Tf\n0.15 0.35 0.65 rg\n1 0 0 1 20 504 Tm\n(6. TEXTO A MULTIPLES TAMANIOS) Tj\nET\n");
    let sizes: &[(f32, &str)] = &[
        (7.0,  "7pt: texto minimo legible en termicas de alta resolucion"),
        (8.5,  "8.5pt: texto pequeno para recibos y tickets"),
        (10.0, "10pt: texto comun en documentos de oficina"),
        (12.0, "12pt: texto grande / titulos menores"),
        (15.0, "15pt: encabezado de seccion"),
        (18.0, "18pt: TITULO PRINCIPAL"),
    ];
    w!("BT\n0 0 0 rg\n");
    let mut ty_s = 492.0_f32;
    for &(sz, txt) in sizes {
        w!("/F1 {sz} Tf\n1 0 0 1 20 {ty_s:.1} Tm ({txt}) Tj\n");
        ty_s -= sz * 1.5 + 1.5;
    }
    w!("ET\n");

    // ── 8. Texto en dos columnas ──────────────────────────────────────────
    w!("BT\n/F1 9 Tf\n0.15 0.35 0.65 rg\n1 0 0 1 20 315 Tm\n(7. INFO TECNICA EN DOS COLUMNAS) Tj\nET\n");
    let col_a: &[&str] = &[
        "Sistema: Printer Monitor",
        "Motor: Tauri 2 + Rust",
        "Libreria: pdfium-render 0.8",
        "Angular: 20 (standalone)",
        "",
        "PIPELINE:",
        "1. PDF recibido (cualquier tam.)",
        "2. PDFium rasteriza en memoria",
        "3. Escala a target_px del papel",
        "4. Rota landscape 90 grados",
        "5. RGBA -> escala de grises",
        "6. Dithering Floyd-Steinberg",
        "7. GS v 0 ESC/POS raster",
        "8. Spooler Windows RAW",
        "9. Impresora termica imprime",
    ];
    let col_b: &[&str] = &[
        "50mm: 576 dots/linea",
        "80mm: 832 dots/linea",
        "DPI: aprox. 300 (12 dots/mm)",
        "Color: 1-bit monocromo",
        "",
        "VERIFICAR EN EL TICKET:",
        "* Barras visibles -> raster OK",
        "* QR visible -> microdetalle OK",
        "* Grises distintos -> dither OK",
        "* Texto 7pt legible -> escala OK",
        "* Colores distintos -> grey OK",
        "* Tabla con bordes -> lineas OK",
        "* Circulo suave -> bezier OK",
        "* Abanico de lineas visible",
        "* Sin cortes -> margen OK",
    ];
    w!("BT\n/F1 8 Tf\n0 0 0 rg\n");
    for (i, &line) in col_a.iter().enumerate() {
        let ly = 303.0 - i as f32 * 11.5;
        w!("1 0 0 1 20 {ly:.1} Tm ({line}) Tj\n");
    }
    for (i, &line) in col_b.iter().enumerate() {
        let ly = 303.0 - i as f32 * 11.5;
        w!("1 0 0 1 310 {ly:.1} Tm ({line}) Tj\n");
    }
    w!("ET\n");
    // Divisor de columnas
    w!("q\n0.7 0.7 0.7 RG\n0.4 w\n300 133 m 300 307 l S\nQ\n");

    // ── 9. Footer ─────────────────────────────────────────────────────────
    w!("q\n0.93 0.93 0.93 rg\n0 0 595.28 58 re f\nQ\n");
    w!("q\n0.65 0.65 0.65 RG\n0.5 w\n0 58 m 595.28 58 l S\nQ\n");
    w!("BT\n/F1 7.5 Tf\n0.30 0.30 0.30 rg\n");
    w!("1 0 0 1 20 42 Tm (Printer Monitor - PDF A4 de prueba para verificar escalado a papel termico) Tj\n");
    w!("1 0 0 1 20 29 Tm (Resultado esperado: todo el contenido visible escalado al ancho del papel.) Tj\n");
    w!("1 0 0 1 20 16 Tm (Barras y QR visibles -> raster OK. Colores distintos en gris -> dither OK.) Tj\n");
    w!("ET\n");

    build_pdf_single_page(w_pt, h_pt, &cs)
}

fn build_pdf_single_page(width_pt: f32, height_pt: f32, content_stream: &str) -> Vec<u8> {
    let mut out = Vec::<u8>::new();
    out.extend_from_slice(b"%PDF-1.4\n%");
    out.extend_from_slice(&[0xE2, 0xE3, 0xCF, 0xD3]);
    out.extend_from_slice(b"\n");

    let mut offsets: Vec<usize> = Vec::new();
    let push_obj = |obj_id: usize, body: String, out: &mut Vec<u8>, offsets: &mut Vec<usize>| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{obj_id} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    push_obj(1, "<< /Type /Catalog /Pages 2 0 R >>".to_string(), &mut out, &mut offsets);
    push_obj(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(), &mut out, &mut offsets);
    push_obj(
        3,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
            width_pt, height_pt
        ),
        &mut out,
        &mut offsets,
    );

    let stream = content_stream.as_bytes();
    let obj4 = format!(
        "<< /Length {} >>\nstream\n{}\nendstream",
        stream.len(),
        content_stream
    );
    push_obj(4, obj4, &mut out, &mut offsets);
    push_obj(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(), &mut out, &mut offsets);

    let xref_start = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            6, xref_start
        )
        .as_bytes(),
    );
    out
}
