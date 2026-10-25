use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use serde_json::Value;

use hyprwpe_core::{WallpaperProperties, WallpaperType};
use hyprwpe_scene::generate_scene_html;
use hyprwpe_unpacker::unpack_pkg;

use crate::process_manager::{stop_wallpaper, write_pid};

pub fn set_wallpaper(
    wallpaper_id: &str,
    monitor: &str,
    wallpaper_dir: &Path,
    properties: &WallpaperProperties,
) -> Result<u32, String> {
    stop_wallpaper(monitor);

    let wallpaper_path = wallpaper_dir.join(wallpaper_id);
    let project_json_path = wallpaper_path.join("project.json");

    if !project_json_path.exists() {
        return Err(format!("project.json not found in {}", wallpaper_path.display()));
    }

    let project_content = fs::read_to_string(&project_json_path)
        .map_err(|e| format!("Failed to read project.json: {e}"))?;

    let project_json: Value = serde_json::from_str(&project_content)
        .map_err(|e| format!("Failed to parse project.json: {e}"))?;

    let type_str = project_json.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("video");

    let wp_type = WallpaperType::from_str(type_str);

    let file_name = project_json.get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut content_root = wallpaper_path.clone();
    let tmp_dir = PathBuf::from(format!("/tmp/HyprWpE/{wallpaper_id}"));

    // Check if unpacking is required
    let target_file = wallpaper_path.join(file_name);
    if !target_file.exists() || wp_type == WallpaperType::Scene {
        if tmp_dir.join("index.html").is_file() {
            if wp_type == WallpaperType::Scene {
                let _ = generate_scene_html(&tmp_dir);
            }
            content_root = tmp_dir;
        } else if let Ok(entries) = fs::read_dir(&wallpaper_path) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("pkg") {
                    content_root = tmp_dir.clone();
                    let _ = fs::create_dir_all(&content_root);
                    if !content_root.join("project.json").exists() {
                        let _ = fs::copy(&project_json_path, content_root.join("project.json"));
                    }
                    for prev_name in &["preview.jpg", "preview.png", "preview.jpeg", "preview.gif"] {
                        let prev_src = wallpaper_path.join(prev_name);
                        if prev_src.is_file() {
                            let _ = fs::copy(&prev_src, content_root.join(prev_name));
                        }
                    }
                    let _ = unpack_pkg(&entry.path(), &content_root);
                    let _ = generate_scene_html(&content_root);
                    break;
                }
            }
        }
    }

    match wp_type {
        WallpaperType::Video => {
            let video_path = content_root.join(file_name);
            let mut opts = vec!["--loop-file=inf".to_string(), format!("--speed={}", properties.speed)];
            if !properties.audio {
                opts.push("--no-audio".to_string());
            }

            match properties.scale.to_lowercase().as_str() {
                "cover" => opts.push("--panscan=1.0".to_string()),
                "fill" => opts.push("--video-aspect-method=stretch".to_string()),
                _ => {}
            }

            let opts_str = opts.join(" ");

            let log_file = File::create(format!("/tmp/hyprwpe-mpv-{monitor}.log"))
                .map(Stdio::from)
                .unwrap_or_else(|_| Stdio::null());

            let mut cmd = Command::new("mpvpaper");
            cmd.arg("-o")
                .arg(&opts_str)
                .arg(monitor)
                .arg(&video_path)
                .stdin(Stdio::null())
                .stdout(log_file)
                .stderr(Stdio::null());

            #[cfg(unix)]
            cmd.process_group(0);

            stop_wallpaper(monitor);
            let child = cmd.spawn()
                .map_err(|e| format!("Failed to spawn mpvpaper: {e}"))?;

            let pid = child.id();
            write_pid(monitor, pid);
            Ok(pid)
        }
        WallpaperType::Web | WallpaperType::Scene => {
            if wp_type == WallpaperType::Scene {
                if !generate_scene_html(&content_root) {
                    return Err(format!("Failed to compile scene WebGL for {}", content_root.display()));
                }
            }

            let index_html = if file_name.is_empty() || wp_type == WallpaperType::Scene {
                content_root.join("index.html")
            } else {
                content_root.join(file_name)
            };

            let input_arg = format!("{}?speed={}", index_html.display(), properties.speed);

            let hyprwpe_web_bin = if Path::new("./target/release/hyprwpe-web").is_file() {
                Some(PathBuf::from("./target/release/hyprwpe-web"))
            } else if Path::new("./target/debug/hyprwpe-web").is_file() {
                Some(PathBuf::from("./target/debug/hyprwpe-web"))
            } else if let Ok(exe) = std::env::current_exe() {
                let candidate = exe.parent().unwrap_or(Path::new("")).join("hyprwpe-web");
                if candidate.is_file() {
                    Some(candidate)
                } else {
                    None
                }
            } else if let Ok(out) = Command::new("which").arg("hyprwpe-web").output() {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path.is_empty() && Path::new(&path).is_file() {
                    Some(PathBuf::from(path))
                } else {
                    None
                }
            } else {
                None
            };

            let log_path = format!("/tmp/hyprwpe-web-{monitor}.log");
            let log_file_out = File::create(&log_path)
                .map(Stdio::from)
                .unwrap_or_else(|_| Stdio::null());
            let log_file_err = File::options().append(true).open(&log_path)
                .map(Stdio::from)
                .unwrap_or_else(|_| Stdio::null());

            stop_wallpaper(monitor);
            let child = if let Some(bin) = hyprwpe_web_bin {
                let mut cmd = Command::new(bin);
                cmd.arg(&input_arg).arg(monitor);
                let pj_file = content_root.join("project.json");
                let mut props_map = serde_json::Map::new();
                if pj_file.is_file() {
                    if let Ok(content) = fs::read_to_string(&pj_file) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(props) = v.get("general").and_then(|g| g.get("properties")).and_then(|p| p.as_object()) {
                                props_map = props.clone();
                            }
                        }
                    }
                }
                for (k, val) in &properties.scene_props {
                    if let Some(existing) = props_map.get_mut(k) {
                        if let Some(obj) = existing.as_object_mut() {
                            obj.insert("value".to_string(), val.clone());
                        } else {
                            props_map.insert(k.clone(), val.clone());
                        }
                    } else {
                        props_map.insert(k.clone(), serde_json::json!({ "value": val }));
                    }
                }
                if !props_map.is_empty() {
                    cmd.arg(serde_json::Value::Object(props_map).to_string());
                }

                cmd.stdin(Stdio::null());
                cmd.stdout(log_file_out);
                cmd.stderr(log_file_err);

                #[cfg(unix)]
                cmd.process_group(0);

                cmd.spawn().map_err(|e| format!("Failed to launch hyprwpe-web: {e}"))?
            } else {
                let mut cmd = Command::new("python3");
                cmd.env("LD_PRELOAD", "/usr/lib/libgtk4-layer-shell.so")
                    .arg("config/web_viewer.py")
                    .arg(index_html.to_string_lossy().as_ref())
                    .arg(monitor)
                    .stdin(Stdio::null())
                    .stdout(log_file_out)
                    .stderr(log_file_err);

                #[cfg(unix)]
                cmd.process_group(0);

                cmd.spawn().map_err(|e| format!("Failed to launch web viewer: {e}"))?
            };

            let pid = child.id();
            write_pid(monitor, pid);
            Ok(pid)
        }
        _ => Err(format!("Unsupported wallpaper type: {type_str}")),
    }
}
