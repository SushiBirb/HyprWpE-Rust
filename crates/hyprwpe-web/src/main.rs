use std::env;
use std::fs::{self, File};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use tiny_http::{Header, Response, Server};
use webkit6::prelude::*;
use webkit6::{UserContentManager, UserScript, UserScriptInjectionTime, WebView};

fn start_local_http_server(directory: PathBuf) -> Result<u16, String> {
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

            let file_path = if rel_path.is_empty() {
                dir.join("index.html")
            } else {
                dir.join(rel_path)
            };

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

fn main() {
    if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: hyprwpe-web <uri_or_path> [monitor_name]");
        std::process::exit(1);
    }

    let target_input = args[1].clone();
    let monitor_name = if args.len() >= 3 {
        Some(args[2].clone())
    } else {
        None
    };

    let app = Application::builder()
        .application_id("dev.hyprwpe.web")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let (uri, props_json_str) = if target_input.starts_with("http://") || target_input.starts_with("https://") {
        let p_str = args.get(3).cloned().unwrap_or_else(|| "{}".to_string());
        (target_input, p_str)
    } else {
        let (raw_path, query) = if let Some(idx) = target_input.find('?') {
            (&target_input[..idx], &target_input[idx..])
        } else {
            (target_input.as_str(), "")
        };

        let path = fs::canonicalize(raw_path).unwrap_or_else(|_| PathBuf::from(raw_path));
        let root_dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };

        let html_file = if path.is_file() {
            path
        } else {
            root_dir.join("index.html")
        };

        let props_str = if let Some(props_arg) = args.get(3) {
            props_arg.clone()
        } else {
            let pj_path = root_dir.join("project.json");
            if pj_path.is_file() {
                fs::read_to_string(&pj_path)
                    .ok()
                    .and_then(|s| {
                        serde_json::from_str::<serde_json::Value>(&s).ok().and_then(|v| {
                            v.get("general")
                                .and_then(|g| g.get("properties"))
                                .map(|p| p.to_string())
                        })
                    })
                    .unwrap_or_else(|| "{}".to_string())
            } else {
                "{}".to_string()
            }
        };

        let port = start_local_http_server(root_dir.clone()).unwrap_or(0);
        let final_uri = if port > 0 {
            let filename = html_file.file_name().and_then(|n| n.to_str()).unwrap_or("index.html");
            format!("http://127.0.0.1:{port}/{filename}{query}")
        } else {
            format!("file://{}{query}", html_file.display())
        };

        (final_uri, props_str)
    };

    app.connect_activate(move |application| {
        build_wallpaper_window(application, &uri, monitor_name.as_deref(), &props_json_str);
    });

    app.run_with_args(&["hyprwpe-web"]);
}

fn build_wallpaper_window(app: &Application, uri: &str, target_monitor: Option<&str>, props_json: &str) {
    let window = ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .build();

    // 1. Initialize gtk4-layer-shell
    window.init_layer_shell();
    window.set_namespace(Some("hyprwpe-web"));
    window.set_layer(Layer::Background);
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::None);

    // Anchor to all 4 edges for full desktop coverage
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    // Apply configured margins from ConfigManager
    if let Ok(cm) = hyprwpe_core::ConfigManager::new() {
        if let Ok(props) = cm.load_properties() {
            let margins = cm.get_panel_margins(&props);
            window.set_margin(Edge::Top, margins.top as i32);
            window.set_margin(Edge::Bottom, margins.bottom as i32);
            window.set_margin(Edge::Left, margins.left as i32);
            window.set_margin(Edge::Right, margins.right as i32);
        }
    }

    // Set target monitor if specified
    if let Some(mon_name) = target_monitor {
        if let Some(display) = gtk4::gdk::Display::default() {
            let monitors = display.monitors();
            for i in 0..monitors.n_items() {
                if let Some(mon) = monitors.item(i).and_downcast::<gtk4::gdk::Monitor>() {
                    if let Some(conn) = mon.connector() {
                        if conn == mon_name {
                            window.set_monitor(Some(&mon));
                            break;
                        }
                    }
                }
            }
        }
    }

    // 2. Configure WebKit WebView & Settings
    let content_manager = UserContentManager::new();

    // Inject Wallpaper Engine standard Web Wallpaper API
    let we_api_script = format!(
        r#"
        window.__weProps = {};
        window.wallpaperRegisterAudioListener = function(callback) {{
            window.__audioListenerCallback = callback;
            const dummyAudio = new Array(128).fill(0.05);
            try {{ callback(dummyAudio); }} catch(e) {{}}
            if (!window.__audioInterval) {{
                window.__audioInterval = setInterval(function() {{
                    if (typeof window.__audioListenerCallback === 'function') {{
                        const t = performance.now() * 0.002;
                        const data = new Array(128);
                        for (let k = 0; k < 128; k++) {{
                            const wave = Math.sin(t * 3.0 + k * 0.15) * 0.5 + Math.cos(t * 1.5 - k * 0.08) * 0.3;
                            data[k] = Math.max(0.02, Math.min(1.0, Math.abs(wave)));
                        }}
                        try {{ window.__audioListenerCallback(data); }} catch(e) {{}}
                    }}
                }}, 33);
            }}
        }};
        window.wallpaperRequestRandomFileForProperty = function(prop, cb) {{
            if (typeof cb === 'function') cb('');
        }};
        "#,
        props_json
    );

    let user_script = UserScript::new(
        &we_api_script,
        webkit6::UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    );
    content_manager.add_script(&user_script);

    let webview = WebView::builder()
        .user_content_manager(&content_manager)
        .build();

    if let Some(settings) = webkit6::prelude::WebViewExt::settings(&webview) {
        settings.set_media_playback_allows_inline(true);
        settings.set_allow_file_access_from_file_urls(true);
        settings.set_allow_universal_access_from_file_urls(true);
        settings.set_enable_webgl(true);
        settings.set_enable_developer_extras(true);
    }

    // Apply user properties on page load completion
    webview.connect_load_changed(move |wv, event| {
        if event == webkit6::LoadEvent::Finished {
            let js = r#"
                (function() {
                    if (window.wallpaperPropertyListener && window.__weProps) {
                        if (typeof window.wallpaperPropertyListener.applyUserProperties === 'function') {
                            try { window.wallpaperPropertyListener.applyUserProperties(window.__weProps); } catch(e) {}
                        }
                        if (typeof window.wallpaperPropertyListener.applyGeneralProperties === 'function') {
                            try { window.wallpaperPropertyListener.applyGeneralProperties(window.__weProps); } catch(e) {}
                        }
                    }
                })();
            "#;
            wv.evaluate_javascript(js, None, None, gio::Cancellable::NONE, |_| {});
        }
    });

    // Guard against infinite crash loops
    let wv_clone = webview.clone();
    let crash_count = std::rc::Rc::new(std::cell::Cell::new(0));
    let last_crash = std::rc::Rc::new(std::cell::Cell::new(std::time::Instant::now()));
    webview.connect_web_process_terminated(move |_, reason| {
        let count = crash_count.get();
        let now = std::time::Instant::now();
        if now.duration_since(last_crash.get()).as_secs() > 10 {
            crash_count.set(1);
        } else {
            crash_count.set(count + 1);
        }
        last_crash.set(now);

        if crash_count.get() >= 2 {
            eprintln!("[hyprwpe-web] Web process terminated repeatedly ({:?}). Exiting to protect desktop from flickering.", reason);
            std::process::exit(1);
        }

        eprintln!("[hyprwpe-web] Web process terminated: {:?}. Reloading in 1s (attempt {}/2)...", reason, crash_count.get());
        let wv = wv_clone.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1000), move || {
            wv.reload();
        });
    });

    window.set_child(Some(&webview));
    webview.load_uri(uri);
    window.present();
}
