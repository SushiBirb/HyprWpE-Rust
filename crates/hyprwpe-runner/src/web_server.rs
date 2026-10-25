use std::fs::File;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tiny_http::{Header, Response, Server};

pub fn start_local_http_server(directory: PathBuf) -> Result<u16, String> {
    let server = Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = server.server_addr().to_ip().map(|addr| match addr {
        SocketAddr::V4(v4) => v4.port(),
        SocketAddr::V6(v6) => v6.port(),
    }).unwrap_or(0);

    let dir = Arc::new(directory);

    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let url = request.url();
            let clean_url = url.split('?').next().unwrap_or(url);
            let decoded_url = urlencoding::decode(clean_url).unwrap_or_else(|_| clean_url.into());
            let rel_path = decoded_url.trim_start_matches('/');

            let mut file_path = if rel_path.is_empty() {
                dir.join("index.html")
            } else {
                dir.join(rel_path)
            };

            if !file_path.is_file() {
                let fallback_assets = [
                    PathBuf::from("/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets"),
                    PathBuf::from("/home/scushi/.steam/steam/steamapps/common/wallpaper_engine/assets"),
                ];
                for base in &fallback_assets {
                    let cand = base.join(rel_path);
                    if cand.is_file() {
                        file_path = cand;
                        break;
                    }
                }
            }

            let cors_header = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
            let cache_header = Header::from_bytes(&b"Cache-Control"[..], &b"no-store, no-cache, must-revalidate"[..]).unwrap();

            if file_path.is_file() {
                if let Ok(file) = File::open(&file_path) {
                    let content_type = match file_path.extension().and_then(|e| e.to_str()) {
                        Some("html") => "text/html",
                        Some("json") => "application/json",
                        Some("js") => "application/javascript",
                        Some("css") => "text/css",
                        Some("png") => "image/png",
                        Some("jpg") | Some("jpeg") => "image/jpeg",
                        Some("gif") => "image/gif",
                        Some("vert") | Some("frag") | Some("glsl") => "text/plain",
                        Some("ttf") => "font/ttf",
                        Some("otf") => "font/otf",
                        Some("woff") => "font/woff",
                        Some("woff2") => "font/woff2",
                        _ => "application/octet-stream",
                    };

                    let type_header = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();

                    let response = Response::from_file(file)
                        .with_header(cors_header)
                        .with_header(cache_header)
                        .with_header(type_header);

                    let _ = request.respond(response);
                    continue;
                }
            }

            let response = Response::from_string("404 Not Found")
                .with_status_code(404)
                .with_header(cors_header)
                .with_header(cache_header);

            let _ = request.respond(response);
        }
    });

    Ok(port)
}
