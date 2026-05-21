/// Pipeline ESC/POS para impresoras "app" (usb_app y network).
///
/// Rutas disponibles:
///   1. `send_escpos_to_port`  — escribe bytes raw al puerto USB o COM de Windows.
///   2. `send_escpos_tcp`      — envía bytes por TCP (red).
///   3. `pdf_to_escpos`        — convierte PDF a bitmap y lo codifica como ESC/POS GS v 0.

// ─── Escritura raw a puerto USB/COM en Windows ───────────────────────────────

/// Escribe `data` directamente al puerto USB o COM (ej. "USB001", "COM3").
/// Usa Win32 CreateFile/WriteFile para acceder a `\\.\PORTxx`.
#[cfg(target_os = "windows")]
pub fn send_escpos_to_port(port: &str, data: &[u8]) -> Result<String, String> {
    use windows::core::PCSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileA, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE,
        OPEN_EXISTING,
    };
    use windows::Win32::System::IO::OVERLAPPED;

    let path = format!("\\\\.\\{port}\0");
    let handle = unsafe {
        CreateFileA(
            PCSTR(path.as_ptr()),
            GENERIC_WRITE.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| format!("No se pudo abrir {port}: {e}"))?;

    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("Handle inválido para {port}"));
    }

    let mut written: u32 = 0;
    let ok = unsafe {
        WriteFile(
            handle,
            Some(data),
            Some(&mut written),
            None::<*mut OVERLAPPED>,
        )
    };
    unsafe { let _ = CloseHandle(handle); }

    ok.map_err(|e| format!("Error escribiendo a {port}: {e}"))?;

    if written as usize != data.len() {
        return Err(format!(
            "Se escribieron {written}/{} bytes a {port}",
            data.len()
        ));
    }
    Ok(format!("ESC/POS enviado a {port} ({written} bytes)"))
}

#[cfg(not(target_os = "windows"))]
pub fn send_escpos_to_port(port: &str, data: &[u8]) -> Result<String, String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(port)
        .map_err(|e| e.to_string())?;
    f.write_all(data).map_err(|e| e.to_string())?;
    Ok(format!("ESC/POS enviado a {port}"))
}

// ─── TCP (re-export) ─────────────────────────────────────────────────────────

pub fn send_escpos_tcp(ip: &str, port_num: u16, data: &[u8]) -> Result<String, String> {
    use std::io::Write;
    use std::net::TcpStream;
    use std::time::Duration;
    let addr = format!("{ip}:{port_num}");
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|_| "Dirección TCP inválida".to_string())?,
        Duration::from_secs(5),
    )
    .map_err(|e| format!("No se pudo conectar a {addr}: {e}"))?;
    stream
        .write_all(data)
        .map_err(|e| format!("Error enviando datos: {e}"))?;
    Ok(format!("ESC/POS enviado a {addr}"))
}

// ─── Constructor de prueba ESC/POS ───────────────────────────────────────────

pub fn build_test_escpos(size: &str) -> Vec<u8> {
    let col = match size {
        "50mm" | "58mm" => 32usize,
        _ => 48,
    };
    let sep = "=".repeat(col);
    let mut d: Vec<u8> = Vec::new();
    d.extend_from_slice(b"\x1b@"); // ESC @ — init
    d.extend_from_slice(b"\x1b!\x08"); // bold
    d.extend_from_slice(sep.as_bytes());
    d.push(b'\n');
    let title = center_text("PRINTER MONITOR", col);
    d.extend_from_slice(title.as_bytes());
    d.push(b'\n');
    let sub = center_text("Pagina de prueba", col);
    d.extend_from_slice(sub.as_bytes());
    d.push(b'\n');
    d.extend_from_slice(b"\x1b!\x00"); // normal
    d.extend_from_slice(sep.as_bytes());
    d.push(b'\n');
    d.extend_from_slice(b"\n\n\n");
    d.extend_from_slice(b"\x1dVB\x00"); // GS V B — cut
    d
}

fn center_text(text: &str, width: usize) -> String {
    if text.len() >= width {
        return text.to_string();
    }
    let pad = (width - text.len()) / 2;
    format!("{}{}", " ".repeat(pad), text)
}

// ─── PDF → ESC/POS con PDFium ─────────────────────────────────────────────────

/// Convierte el primer/todos los PDFs a bitmap y genera el stream ESC/POS
/// usando el comando GS v 0 (graphics raster) compatible con la mayoría de
/// impresoras térmicas Epson/Bixolon/Citizen/Star.
///
/// `width`: "50mm" → 576 dots a 203 dpi | "80mm" → 832 dots a 203 dpi
#[cfg(target_os = "windows")]
pub fn pdf_to_escpos(pdf_bytes: &[u8], size: &str) -> Result<Vec<u8>, String> {
    use pdfium_render::prelude::*;
    use image::{GrayImage, Luma};

    // Ancho en dots de la zona imprimible según tamaño de papel:
    //   50mm / 58mm → 576 dots  (48mm × 12 dots/mm ≈ 576, resolución típica Epson/Bixolon)
    //   80mm        → 832 dots  (72mm × 11.5 dots/mm ≈ 832)
    // Nota: si tu impresora usa 203dpi (8dots/mm) cambia 576→384 o 832→576.
    let target_px = match size {
        "50mm" | "58mm" => 576u32,
        _ => 832u32,
    };

    let pdfium = crate::api_server::load_pdfium()?;
    let doc = pdfium
        .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
        .map_err(|e| format!("No se pudo abrir PDF: {e}"))?;

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"\x1b@"); // init

    for (idx, page) in doc.pages().iter().enumerate() {
        // set_target_width escala CUALQUIER tamaño de PDF (A4, Carta, 80mm, etc.)
        // al ancho exacto de dots del papel térmico, manteniendo la relación de aspecto.
        // rotate_if_landscape(Degrees90, true) rota páginas apaisadas 90° y reaplica
        // el constraint de ancho al lado largo, garantizando que el contenido siempre
        // llene el papel térmico independientemente de la orientación del PDF origen.
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(target_px as i32)
                    .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true),
            )
            .map_err(|e| format!("Render PDF pág {}: {e}", idx + 1))?;

        let rgba = bitmap.as_image().to_rgba8();
        let (w, h) = rgba.dimensions();

        // Convertir a gris y dithering Floyd-Steinberg
        let mut gray = GrayImage::new(w, h);
        for (x, y, px) in rgba.enumerate_pixels() {
            let r = px[0] as u32;
            let g = px[1] as u32;
            let b = px[2] as u32;
            let luma = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
            gray.put_pixel(x, y, Luma([luma]));
        }
        let dithered = floyd_steinberg(&gray);

        // Codificar como ESC/POS GS v 0
        out.extend_from_slice(&raster_to_escpos_gsvzero(&dithered));
        out.extend_from_slice(b"\n");
    }

    out.extend_from_slice(b"\n\n\n");
    out.extend_from_slice(b"\x1dVB\x00"); // cut
    Ok(out)
}

#[cfg(not(target_os = "windows"))]
pub fn pdf_to_escpos(pdf_bytes: &[u8], size: &str) -> Result<Vec<u8>, String> {
    let _ = (pdf_bytes, size);
    Err("Conversión PDF→ESC/POS aún no implementada en esta plataforma.".to_string())
}

/// Dithering Floyd-Steinberg sobre imagen gris → imagen 1-bit (0=negro, 255=blanco).
fn floyd_steinberg(src: &image::GrayImage) -> image::GrayImage {
    let (w, h) = src.dimensions();
    let mut buf: Vec<i32> = src.pixels().map(|p| p[0] as i32).collect();

    for y in 0..h as usize {
        for x in 0..w as usize {
            let old = buf[y * w as usize + x];
            let new = if old < 128 { 0i32 } else { 255 };
            buf[y * w as usize + x] = new;
            let err = old - new;
            macro_rules! diffuse {
                ($dx:expr, $dy:expr, $frac:expr) => {
                    let nx = x as isize + $dx;
                    let ny = y as isize + $dy;
                    if nx >= 0 && ny >= 0 && nx < w as isize && ny < h as isize {
                        let idx = ny as usize * w as usize + nx as usize;
                        buf[idx] = (buf[idx] + err * $frac / 16).clamp(0, 255);
                    }
                };
            }
            diffuse!(1, 0, 7);
            diffuse!(-1, 1, 3);
            diffuse!(0, 1, 5);
            diffuse!(1, 1, 1);
        }
    }

    let pixels: Vec<u8> = buf.iter().map(|&v| v as u8).collect();
    image::GrayImage::from_raw(w, h, pixels).unwrap_or_else(|| src.clone())
}

/// Convierte imagen 1-bit (0=negro, 255=blanco) a stream ESC/POS GS v 0.
/// Formato: GS v 0 m xL xH yL yH d1…dk
fn raster_to_escpos_gsvzero(img: &image::GrayImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let bytes_per_row = (w + 7) / 8;
    let xl = (bytes_per_row & 0xFF) as u8;
    let xh = ((bytes_per_row >> 8) & 0xFF) as u8;
    let yl = (h & 0xFF) as u8;
    let yh = ((h >> 8) & 0xFF) as u8;

    let mut out = vec![0x1d, b'v', b'0', 0x00, xl, xh, yl, yh];

    for y in 0..h {
        for byte_idx in 0..bytes_per_row {
            let mut byte: u8 = 0;
            for bit in 0..8 {
                let x = byte_idx * 8 + bit;
                if x < w {
                    let pixel = img.get_pixel(x, y)[0];
                    if pixel < 128 {
                        byte |= 1 << (7 - bit); // bit alto = izquierda
                    }
                }
            }
            out.push(byte);
        }
    }
    out
}

// ─── Raw Windows Spooler (para impresoras SO con driver Generic/Text Only) ────

/// Envía bytes crudos a una cola de impresión de Windows usando la API del
/// spooler (OpenPrinterW / StartDocPrinterW / WritePrinter / …).
/// Esto envía los bytes tal cual sin pasar por el subsistema GDI, lo que es
/// correcto para impresoras térmicas que usan driver "Generic / Text Only".
#[cfg(target_os = "windows")]
pub fn send_raw_to_windows_queue(printer: &str, data: &[u8]) -> Result<String, String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Printing::{
        ClosePrinter, DOC_INFO_1W, EndDocPrinter, EndPagePrinter,
        OpenPrinterW, StartDocPrinterW, StartPagePrinter, WritePrinter,
    };

    // Strings en wide (UTF-16) terminados en nulo
    let printer_w: Vec<u16> = printer.encode_utf16().chain(std::iter::once(0)).collect();
    let docname_w: Vec<u16> = "PM-Job\0".encode_utf16().collect();
    let datatype_w: Vec<u16> = "RAW\0".encode_utf16().collect();

    let mut handle = HANDLE::default();
    unsafe {
        OpenPrinterW(
            windows::core::PCWSTR(printer_w.as_ptr()),
            &mut handle,
            None,
        )
        .map_err(|e| format!("OpenPrinter falló para '{printer}': {e}"))?;
    }

    let result: Result<(), String> = (|| unsafe {
        let doc_info = DOC_INFO_1W {
            pDocName: windows::core::PWSTR(docname_w.as_ptr() as *mut u16),
            pOutputFile: windows::core::PWSTR::null(),
            pDatatype: windows::core::PWSTR(datatype_w.as_ptr() as *mut u16),
        };

        // StartDocPrinterW devuelve el ID del documento (>0 = OK, 0 = error)
        let doc_id = StartDocPrinterW(handle, 1, &doc_info);
        if doc_id == 0 {
            return Err("StartDocPrinterW falló (docId=0)".to_string());
        }

        // StartPagePrinter, WritePrinter, EndPage/Doc retornan BOOL (i32: 0=error)
        if StartPagePrinter(handle).0 == 0 {
            return Err("StartPagePrinter falló".to_string());
        }

        let mut written: u32 = 0;
        if WritePrinter(
            handle,
            data.as_ptr() as *const core::ffi::c_void,
            data.len() as u32,
            &mut written,
        ).0 == 0 {
            return Err("WritePrinter falló".to_string());
        }

        if written as usize != data.len() {
            return Err(format!(
                "WritePrinter: se escribieron {written}/{} bytes",
                data.len()
            ));
        }

        if EndPagePrinter(handle).0 == 0 {
            return Err("EndPagePrinter falló".to_string());
        }
        if EndDocPrinter(handle).0 == 0 {
            return Err("EndDocPrinter falló".to_string());
        }

        Ok(())
    })();

    unsafe { let _ = ClosePrinter(handle); }

    result?;
    Ok(format!(
        "PDF enviado a cola Windows '{printer}' ({} bytes ESC/POS)",
        data.len()
    ))
}

#[cfg(not(target_os = "windows"))]
pub fn send_raw_to_windows_queue(_printer: &str, _data: &[u8]) -> Result<String, String> {
    Err("send_raw_to_windows_queue solo disponible en Windows.".to_string())
}
