//! Pont de développement : sert l'interface dans un navigateur et relaie les appels vers le moteur.
//! Usage : cargo run --features bridge --bin ondes-bridge   puis http://127.0.0.1:1430
use ondes_lib::Engine;
use serde_json::{json, Value};
use std::path::PathBuf;

fn main() {
    let dir = std::env::var("ONDES_DATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("ondes-bridge"));
    let ui = PathBuf::from(std::env::var("ONDES_UI").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../ui").into()));
    let port = std::env::var("ONDES_PORT").unwrap_or_else(|_| "1430".into());
    let engine = Engine::new(dir.clone());
    engine.start();
    let server = tiny_http::Server::http(format!("127.0.0.1:{port}")).expect("port occupé");
    eprintln!("Ondes bridge : http://127.0.0.1:{port}  (données : {})", dir.display());
    for mut req in server.incoming_requests() {
        let url = req.url().split('?').next().unwrap_or("/").to_string();
        if req.method() == &tiny_http::Method::Post && url == "/invoke" {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("").to_string();
            let args = v.get("args").cloned().unwrap_or(Value::Null);
            let out = match tauri::async_runtime::block_on(engine.dispatch(&cmd, args)) {
                Ok(r) => json!({ "result": r }),
                Err(e) => json!({ "error": e.to_string() }),
            };
            let resp = tiny_http::Response::from_string(out.to_string()).with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap());
            let _ = req.respond(resp);
            continue;
        }
        let rel = if url == "/" { "index.html".to_string() } else { url.trim_start_matches('/').replace("..", "") };
        let path = ui.join(&rel);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let ct = match path.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("js") => "text/javascript; charset=utf-8",
                    Some("css") => "text/css; charset=utf-8",
                    Some("svg") => "image/svg+xml",
                    Some("png") => "image/png",
                    _ => "application/octet-stream",
                };
                let _ = req.respond(tiny_http::Response::from_data(bytes).with_header(format!("Content-Type: {ct}").parse::<tiny_http::Header>().unwrap()));
            }
            Err(_) => {
                let _ = req.respond(tiny_http::Response::from_string("404").with_status_code(404));
            }
        }
    }
}
