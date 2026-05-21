use axum::{extract::Json, routing::post, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

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

pub async fn start() {
    let app = Router::new().route("/api/print", post(handle_print));
    let listener = TcpListener::bind("127.0.0.1:8001").await.unwrap();
    log::info!("API server escuchando en http://127.0.0.1:8001");
    axum::serve(listener, app).await.unwrap();
}

async fn handle_print(Json(req): Json<PrintRequest>) -> Json<PrintResponse> {
    match print_pdf_job(&req.pdf_b64, &req.printer, &req.width) {
        Ok(msg) => Json(PrintResponse { ok: true, message: msg }),
        Err(e) => Json(PrintResponse { ok: false, message: e }),
    }
}

pub fn print_pdf_job(pdf_b64: &str, printer_name: &str, width: &str) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let pdf_bytes = base64_decode(pdf_b64)?;

    // Enviar a impresora usando lp / Out-Printer según SO
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let mut child = Command::new("lp")
            .args(["-d", printer_name, "-"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(&pdf_bytes).map_err(|e| e.to_string())?;
        }
        child.wait().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (pdf_bytes, width);
        return Err("Impresión PDF en Windows no implementada aún.".to_string());
    }

    Ok(format!("PDF enviado a '{printer_name}' [{width}]"))
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    // Simple base64 decode usando iteradores
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let clean: Vec<u8> = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=')
        .map(|c| alphabet.find(c).unwrap_or(0) as u8)
        .collect();

    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < clean.len() {
        let b0 = (clean[i] << 2) | (clean[i + 1] >> 4);
        let b1 = (clean[i + 1] << 4) | (clean[i + 2] >> 2);
        let b2 = (clean[i + 2] << 6) | clean[i + 3];
        out.extend_from_slice(&[b0, b1, b2]);
        i += 4;
    }
    Ok(out)
}
