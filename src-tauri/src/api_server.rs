/// Servidor HTTP de impresión de PDF.
///
/// Escucha siempre en `0.0.0.0:8001`.
///
/// # Rutas
/// - `GET  /`            → HTML de estado ("backend printer trabajando")
/// - `GET  /api/status`  → JSON de salud
/// - `POST /api/print`   → imprime un PDF en base64
///
/// ## POST /api/print
/// ```json
/// {
///   "pdf_b64":  "<PDF en base64>",
///   "printer":  "<nombre de cola CUPS>",
///   "width":    "58mm" | "80mm"
/// }
/// ```
use std::io::Read;
use tiny_http::{Header, Method, Response, Server};

/// Puerto fijo del servidor de impresión PDF.
pub const PORT: u16 = 8001;

pub fn start() {
    let bind = format!("0.0.0.0:{}", PORT);
    let server = match Server::http(&bind) {
        Ok(s) => {
            println!("🖨️  Printer backend HTTP en http://0.0.0.0:{}", PORT);
            s
        }
        Err(e) => {
            eprintln!("❌ No se pudo iniciar servidor HTTP en {}: {}", bind, e);
            return;
        }
    };

    for request in server.incoming_requests() {
        std::thread::spawn(move || handle(request));
    }
}

// ─── Utilidades ──────────────────────────────────────────────────────────────

fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap(),
        Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
    ]
}

fn json_resp(body: &str, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut r = Response::from_string(body.to_owned());
    r = r.with_status_code(tiny_http::StatusCode(status));
    for h in cors_headers() {
        r = r.with_header(h);
    }
    r
}

fn ok(msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let safe = msg.replace('"', "'");
    json_resp(&format!(r#"{{"ok":true,"message":"{safe}"}}"#), 200)
}

fn err(msg: &str, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    let safe = msg.replace('"', "'").replace('\n', " ");
    json_resp(&format!(r#"{{"ok":false,"message":"{safe}"}}"#), status)
}

// ─── Dispatcher ──────────────────────────────────────────────────────────────

fn handle(mut request: tiny_http::Request) {
    let method = request.method().clone();
    // Normaliza la URL: quita trailing slash excepto si ya es "/"
    let raw = request.url().split('?').next().unwrap_or("/");
    let url = if raw.len() > 1 { raw.trim_end_matches('/') } else { raw };

    // CORS preflight
    if method == Method::Options {
        let _ = request.respond(json_resp(r#"{}"#, 204));
        return;
    }

    match (method, url) {
        (Method::Get, "/" | "") => {
            let html = format!(
                "<!DOCTYPE html><html lang=\"es\"><head>\
                <meta charset=\"UTF-8\">\
                <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
                <title>Printer Backend</title>\
                <style>body{{font-family:system-ui,sans-serif;display:flex;align-items:center;\
                justify-content:center;height:100vh;margin:0;background:#f0fdf4;}}\
                .card{{background:#fff;border-radius:12px;padding:2rem 3rem;\
                box-shadow:0 4px 24px #0001;text-align:center;}}\
                h1{{color:#16a34a;margin:0 0 .5rem}}p{{color:#555;margin:.25rem 0}}\
                .badge{{display:inline-block;background:#dcfce7;color:#15803d;\
                border-radius:999px;padding:.25rem 1rem;font-weight:600;margin-top:1rem}}</style>\
                </head><body><div class=\"card\">\
                <h1>&#x1F5A8; backend printer trabajando</h1>\
                <p>Puerto: <strong>{}</strong></p>\
                <p>Ruta de impresión: <code>POST /api/print</code></p>\
                <span class=\"badge\">&#x2705; activo</span>\
                </div></body></html>",
                PORT
            );
            let mut r = Response::from_string(html);
            r = r.with_status_code(tiny_http::StatusCode(200));
            r = r.with_header(
                Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap()
            );
            let _ = request.respond(r);
        }
        (Method::Get, "/api/status") => {
            let _ = request.respond(ok("Printer Monitor activo"));
        }
        (Method::Post, "/api/print") => {
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                let _ = request.respond(err("Error leyendo cuerpo de la solicitud", 400));
                return;
            }
            handle_print(request, &body);
        }
        _ => {
            let _ = request.respond(err("Ruta no encontrada", 404));
        }
    }
}

fn handle_print(request: tiny_http::Request, body: &str) {
    let json: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            let _ = request.respond(err(&format!("JSON inválido: {e}"), 400));
            return;
        }
    };

    let Some(pdf_b64) = json["pdf_b64"].as_str() else {
        let _ = request.respond(err("Falta campo pdf_b64", 400));
        return;
    };
    let Some(printer) = json["printer"].as_str() else {
        let _ = request.respond(err("Falta campo printer", 400));
        return;
    };
    let width = json["width"].as_str().unwrap_or("80mm");

    match crate::printers::print_pdf_job(pdf_b64, printer, width) {
        Ok(msg) => {
            let _ = request.respond(ok(&msg));
        }
        Err(e) => {
            let _ = request.respond(err(&e, 500));
        }
    }
}
