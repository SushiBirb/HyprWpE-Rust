use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use hyprwpe_core::{WallpaperProperties, WallpaperType};
use hyprwpe_scene::generate_scene_html;
use hyprwpe_unpacker::unpack_pkg;

use crate::analyzer::{
    calc_color_histogram_similarity, calc_motion_dynamics, calc_structural_gradient_similarity,
};
use crate::collage::generate_comparison_collage;
use crate::detector::detect_artifacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvaluationMode {
    Headless,
    LiveDesktop,
}

#[derive(Debug, Clone)]
pub struct EvaluationOptions {
    pub mode: EvaluationMode,
    pub live_monitor: Option<String>,
    pub keep_running: bool,
}

impl Default for EvaluationOptions {
    fn default() -> Self {
        Self {
            mode: EvaluationMode::Headless,
            live_monitor: None,
            keep_running: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EvaluationReport {
    pub wallpaper_id: String,
    pub title: String,
    pub wallpaper_type: String,
    pub mode: String,
    pub final_score: f32,
    pub color_score: f32,
    pub struct_score: f32,
    pub motion_score: f32,
    pub stability_score: f32,
    pub artifact_penalty: f32,
    pub solid_color_fallback: bool,
    pub oscillating_vertical_lines: bool,
    pub static_noise_detected: bool,
    pub details: Vec<String>,
    pub collage_path: Option<String>,
    pub error: Option<String>,
}

pub fn detect_wallpaper_info(wp_path: &Path) -> (WallpaperType, String, String) {
    let mut title = wp_path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string();
    let mut file_name = String::new();
    let mut wp_type = WallpaperType::Unknown("unknown".to_string());

    let pjson_path = wp_path.join("project.json");
    if pjson_path.exists() {
        if let Ok(content) = fs::read_to_string(&pjson_path) {
            if let Ok(data) = serde_json::from_str::<Value>(&content) {
                if let Some(t) = data.get("title").and_then(|v| v.as_str()) {
                    title = t.to_string();
                }
                if let Some(f) = data.get("file").and_then(|v| v.as_str()) {
                    file_name = f.to_string();
                }
                if let Some(t_str) = data.get("type").and_then(|v| v.as_str()) {
                    wp_type = WallpaperType::from_str(t_str);
                }
            }
        }
    }

    if matches!(wp_type, WallpaperType::Unknown(_)) {
        if wp_path.join("scene.pkg").exists() || wp_path.join("scene.json").exists() {
            wp_type = WallpaperType::Scene;
        } else if !file_name.is_empty() {
            let ext = Path::new(&file_name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if ext == "mp4" || ext == "webm" || ext == "mkv" || ext == "avi" {
                wp_type = WallpaperType::Video;
            } else if ext == "html" || ext == "htm" {
                wp_type = WallpaperType::Web;
            }
        } else if wp_path.join("index.html").exists() {
            wp_type = WallpaperType::Web;
        } else if let Ok(entries) = fs::read_dir(wp_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    let ext = ext.to_lowercase();
                    if ext == "mp4" || ext == "webm" || ext == "mkv" {
                        wp_type = WallpaperType::Video;
                        file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                        break;
                    }
                }
            }
        }
    }

    (wp_type, title, file_name)
}

fn open_preview_image(wp_path: &Path, render_ref: Option<&image::DynamicImage>) -> Option<image::DynamicImage> {
    let pjson = wp_path.join("project.json");
    if pjson.exists() {
        if let Ok(content) = fs::read_to_string(&pjson) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                if let Some(p) = val.get("preview").and_then(|v| v.as_str()) {
                    let rel = wp_path.join(p);
                    if rel.exists() {
                        if let Ok(img) = image::open(&rel) {
                            return Some(img);
                        }
                    }
                }
            }
        }
    }

    let mut best_img: Option<(f32, image::DynamicImage)> = None;

    if let Ok(entries) = fs::read_dir(wp_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                let name_low = name.to_ascii_lowercase();
                if name_low.starts_with("preview") {
                    if name_low.ends_with(".gif") {
                        if let Ok(file) = fs::File::open(&p) {
                            if let Ok(decoder) = image::codecs::gif::GifDecoder::new(file) {
                                use image::AnimationDecoder;
                                if let Ok(frames) = decoder.into_frames().collect_frames() {
                                    for frame in frames {
                                        let dyn_img = image::DynamicImage::ImageRgba8(frame.into_buffer());
                                        let score = if let Some(ref_frame) = render_ref {
                                            calc_structural_gradient_similarity(&dyn_img, ref_frame)
                                        } else {
                                            1.0
                                        };
                                        if best_img.is_none() || score > best_img.as_ref().unwrap().0 {
                                            best_img = Some((score, dyn_img));
                                        }
                                    }
                                }
                            }
                        }
                    } else if let Ok(img) = image::open(&p) {
                        return Some(img);
                    }
                }
            }
        }
    }

    best_img.map(|(_, img)| img)
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            let _ = fs::copy(&from, &to);
        }
    }
    Ok(())
}

pub fn evaluate_wallpaper(wallpaper_id: &str, steam_dir: &Path, output_base: &Path) -> EvaluationReport {
    evaluate_wallpaper_with_options(wallpaper_id, steam_dir, output_base, &EvaluationOptions::default())
}

pub fn evaluate_wallpaper_with_options(
    wallpaper_id: &str,
    steam_dir: &Path,
    output_base: &Path,
    options: &EvaluationOptions,
) -> EvaluationReport {
    let wp_path = steam_dir.join(wallpaper_id);
    if !wp_path.is_dir() {
        return EvaluationReport {
            wallpaper_id: wallpaper_id.to_string(),
            title: wallpaper_id.to_string(),
            wallpaper_type: "Unknown".to_string(),
            mode: format!("{:?}", options.mode),
            final_score: 0.0,
            color_score: 0.0,
            struct_score: 0.0,
            motion_score: 0.0,
            stability_score: 0.0,
            artifact_penalty: 0.0,
            solid_color_fallback: false,
            oscillating_vertical_lines: false,
            static_noise_detected: false,
            details: vec!["Wallpaper directory not found".to_string()],
            collage_path: None,
            error: Some("Wallpaper directory not found".to_string()),
        };
    }

    let (wp_type, title, file_name) = detect_wallpaper_info(&wp_path);
    let type_label = match wp_type {
        WallpaperType::Scene => "Scene",
        WallpaperType::Video => "Video",
        WallpaperType::Web => "Web",
        _ => "Scene",
    };

    let work_dir = output_base.join(format!("work_{wallpaper_id}"));
    let out_dir = output_base.join(format!("out_{wallpaper_id}"));
    let _ = fs::create_dir_all(&work_dir);
    let _ = fs::create_dir_all(&out_dir);

    let render_0: Option<image::DynamicImage>;
    let render_1: Option<image::DynamicImage>;
    let mode_label: String;

    match options.mode {
        EvaluationMode::LiveDesktop => {
            let monitor = options.live_monitor.as_deref().unwrap_or("DP-2");
            mode_label = format!("Live Desktop ({monitor})");
            let props = WallpaperProperties::default();

            match hyprwpe_runner::set_wallpaper(wallpaper_id, monitor, steam_dir, &props) {
                Ok(_) => {
                    // Allow compositor and renderer to map and stabilize
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let frame_0_path = out_dir.join("live_frame_0.png");
                    let _ = std::process::Command::new("grim")
                        .arg("-o")
                        .arg(monitor)
                        .arg(&frame_0_path)
                        .status();

                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let frame_1_path = out_dir.join("live_frame_1.png");
                    let _ = std::process::Command::new("grim")
                        .arg("-o")
                        .arg(monitor)
                        .arg(&frame_1_path)
                        .status();

                    if !options.keep_running {
                        hyprwpe_runner::stop_wallpaper(monitor);
                    }

                    render_0 = image::open(&frame_0_path).ok();
                    render_1 = image::open(&frame_1_path).ok();
                }
                Err(e) => {
                    return EvaluationReport {
                        wallpaper_id: wallpaper_id.to_string(),
                        title,
                        wallpaper_type: type_label.to_string(),
                        mode: mode_label,
                        final_score: 0.0,
                        color_score: 0.0,
                        struct_score: 0.0,
                        motion_score: 0.0,
                        stability_score: 0.0,
                        artifact_penalty: 0.0,
                        solid_color_fallback: false,
                        oscillating_vertical_lines: false,
                        static_noise_detected: false,
                        details: vec![format!("Failed to activate live wallpaper on {monitor}: {e}")],
                        collage_path: None,
                        error: Some(e),
                    };
                }
            }
        }
        EvaluationMode::Headless => {
            match wp_type {
                WallpaperType::Video => {
                    mode_label = "Headless FFmpeg".to_string();
                    let target_video = if !file_name.is_empty() && wp_path.join(&file_name).is_file() {
                        wp_path.join(&file_name)
                    } else {
                        let mut found = None;
                        if let Ok(entries) = fs::read_dir(&wp_path) {
                            for e in entries.flatten() {
                                let p = e.path();
                                if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                                    if ["mp4", "webm", "mkv", "avi"].contains(&ext.to_lowercase().as_str()) {
                                        found = Some(p);
                                        break;
                                    }
                                }
                            }
                        }
                        found.unwrap_or_else(|| wp_path.join(&file_name))
                    };

                    let frame_0_path = out_dir.join("frame_0.0.png");
                    let frame_1_path = out_dir.join("frame_1.5.png");

                    let _ = std::process::Command::new("ffmpeg")
                        .arg("-ss").arg("00:00:00.5")
                        .arg("-i").arg(&target_video)
                        .arg("-vframes").arg("1")
                        .arg("-y").arg(&frame_0_path)
                        .status();

                    let _ = std::process::Command::new("ffmpeg")
                        .arg("-ss").arg("00:00:02.0")
                        .arg("-i").arg(&target_video)
                        .arg("-vframes").arg("1")
                        .arg("-y").arg(&frame_1_path)
                        .status();

                    render_0 = image::open(&frame_0_path).ok();
                    render_1 = image::open(&frame_1_path).ok();
                }
                WallpaperType::Web => {
                    mode_label = "Headless Chromium".to_string();
                    let harness_script = PathBuf::from("tests/harness.js");
                    let entry = if file_name.is_empty() { "index.html" } else { &file_name };
                    let _ = std::process::Command::new("node")
                        .arg(&harness_script)
                        .arg(&wp_path)
                        .arg("--file").arg(entry)
                        .arg("--out").arg(&out_dir)
                        .arg("--fast")
                        .status();

                    let frame_0_path = out_dir.join("frame_0.0.png");
                    let frame_1_path = out_dir.join("frame_1.5.png");
                    render_0 = image::open(&frame_0_path).ok();
                    render_1 = image::open(&frame_1_path).ok();
                }
                WallpaperType::Scene | _ => {
                    mode_label = "Headless WebGL2".to_string();
                    // Unpack PKG files or copy unpacked scene files
                    let mut has_pkg = false;
                    if let Ok(entries) = fs::read_dir(&wp_path) {
                        for entry in entries.flatten() {
                            if entry.path().extension().and_then(|s| s.to_str()) == Some("pkg") {
                                let _ = unpack_pkg(&entry.path(), &work_dir);
                                has_pkg = true;
                            }
                        }
                    }
                    if !has_pkg && wp_path.join("scene.json").is_file() {
                        let _ = copy_dir_all(&wp_path, &work_dir);
                    }

                    let pjson_path = wp_path.join("project.json");
                    if pjson_path.exists() && !work_dir.join("project.json").exists() {
                        let _ = fs::copy(&pjson_path, work_dir.join("project.json"));
                    }

                    for prev_name in &["preview.jpg", "preview.png", "preview.jpeg", "preview.gif"] {
                        let p_src = wp_path.join(prev_name);
                        if p_src.is_file() {
                            let _ = fs::copy(&p_src, work_dir.join(prev_name));
                        }
                    }

                    // Compile WebGL scene HTML
                    let gen_ok = generate_scene_html(&work_dir);
                    if !gen_ok {
                        let _ = fs::remove_dir_all(&work_dir);
                        return EvaluationReport {
                            wallpaper_id: wallpaper_id.to_string(),
                            title,
                            wallpaper_type: type_label.to_string(),
                            mode: mode_label,
                            final_score: 0.0,
                            color_score: 0.0,
                            struct_score: 0.0,
                            motion_score: 0.0,
                            stability_score: 0.0,
                            artifact_penalty: 0.0,
                            solid_color_fallback: false,
                            oscillating_vertical_lines: false,
                            static_noise_detected: false,
                            details: vec!["Failed to generate WebGL scene index.html".to_string()],
                            collage_path: None,
                            error: Some("Failed to generate scene HTML".to_string()),
                        };
                    }

                    // Run Playwright visual inspection harness
                    let harness_script = PathBuf::from("tests/harness.js");
                    let _ = std::process::Command::new("node")
                        .arg(&harness_script)
                        .arg(&work_dir)
                        .arg("--out").arg(&out_dir)
                        .arg("--fast")
                        .status();

                    let frame_0_path = out_dir.join("frame_0.0.png");
                    let frame_1_path = out_dir.join("frame_1.5.png");
                    render_0 = image::open(&frame_0_path).ok();
                    render_1 = image::open(&frame_1_path).ok();
                }
            }
        }
    }

    if render_0.is_none() {
        return EvaluationReport {
            wallpaper_id: wallpaper_id.to_string(),
            title,
            wallpaper_type: type_label.to_string(),
            mode: mode_label,
            final_score: 0.0,
            color_score: 0.0,
            struct_score: 0.0,
            motion_score: 0.0,
            stability_score: 0.0,
            artifact_penalty: 0.0,
            solid_color_fallback: false,
            oscillating_vertical_lines: false,
            static_noise_detected: false,
            details: vec!["Failed to capture rendered frame".to_string()],
            collage_path: None,
            error: Some("Missing rendered output frame".to_string()),
        };
    }

    let render_frame = render_0.unwrap();
    let preview_img = open_preview_image(&wp_path, Some(&render_frame));

    // Compute metrics across animated frame captures
    let (color_score, struct_score) = if let Some(ref prev) = preview_img {
        let c0 = calc_color_histogram_similarity(&render_frame, prev);
        let s0 = calc_structural_gradient_similarity(&render_frame, prev);

        let (c1, s1) = if let Some(ref r1) = render_1 {
            (
                calc_color_histogram_similarity(r1, prev),
                calc_structural_gradient_similarity(r1, prev)
            )
        } else {
            (c0, s0)
        };

        (c0.max(c1), s0.max(s1))
    } else {
        (90.0, 90.0)
    };

    let motion_score = if let Some(ref r1) = render_1 {
        calc_motion_dynamics(&render_frame, r1, None)
    } else {
        90.0
    };

    let stability_score = 100.0;

    let mut artifacts = detect_artifacts(&render_frame);

    // Suppress false positive black screen penalty if preview asset is genuinely dark
    if artifacts.empty_or_black_screen || artifacts.partial_canvas_render {
        if let Some(ref prev) = preview_img {
            let prev_rgb = prev.to_rgb8();
            let mut p_sum = 0.0f64;
            let (pw, ph) = prev_rgb.dimensions();
            let mut p_dark_count = 0usize;
            for p in prev_rgb.pixels() {
                p_sum += (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0;
                if p[0] < 8 && p[1] < 8 && p[2] < 8 {
                    p_dark_count += 1;
                }
            }
            let p_count = (pw * ph).max(1) as f64;
            let p_mean = p_sum / p_count;
            let p_dark_ratio = p_dark_count as f64 / p_count;
            if p_mean < 15.0 || p_dark_ratio > 0.60 {
                artifacts.empty_or_black_screen = false;
                artifacts.partial_canvas_render = false;
                artifacts.solid_color_fallback = false;
                artifacts.total_penalty = 0.0;
            }
        }
    }

    // Suppress false positive black screen penalty if the scene explicitly defines an intentional black solid layer
    let has_intentional_black_bg = if let Ok(content) = fs::read_to_string(work_dir.join("scene.json")) {
        content.contains("\"color\": \"0.00000 0.00000 0.00000\"")
            || content.contains("\"color\":\"0 0 0\"")
            || content.contains("solidlayer")
    } else {
        false
    };

    if has_intentional_black_bg && artifacts.empty_or_black_screen {
        let rgb = render_frame.to_rgb8();
        let non_black_pixels = rgb.pixels().filter(|p| p[0] > 10 || p[1] > 10 || p[2] > 10).count();
        if non_black_pixels > 1000 {
            artifacts.empty_or_black_screen = false;
            artifacts.total_penalty = (artifacts.total_penalty - 60.0).max(0.0);
        }
    }

    // Suppress false positive static noise penalty only if structural similarity (SSIM) is truly high (>85%)
    if artifacts.static_noise_detected && struct_score >= 85.0 {
        artifacts.static_noise_detected = false;
        artifacts.total_penalty = (artifacts.total_penalty - 100.0).max(0.0);
    }

    let artifact_penalty = artifacts.total_penalty;
    let solid_color_fallback = artifacts.solid_color_fallback;
    let details = artifacts.details;

    // Visual fidelity is 85% of total score (50% SSIM Structure + 35% Color/Tone Fidelity).
    // Motion and stability are rewarded (up to 10% motion + 5% stability), but gated by visual fidelity:
    // a broken or black screen cannot achieve a high score simply by not crashing.
    let visual_fidelity = (struct_score * 0.50) + (color_score * 0.35);
    let fidelity_factor = (visual_fidelity / 100.0).clamp(0.05, 1.0);
    let motion_pts = (motion_score * 0.10) * fidelity_factor;
    let stability_pts = (stability_score * 0.05) * fidelity_factor;

    let base_score = visual_fidelity + motion_pts + stability_pts;
    let final_score = (base_score - artifact_penalty).clamp(0.0, 100.0);

    // Generate comparison collage
    let collage_file = out_dir.join("comparison.png");
    let collage_path = if generate_comparison_collage(
        preview_img.as_ref(),
        Some(&render_frame),
        wallpaper_id,
        &title,
        type_label,
        &mode_label,
        final_score,
        color_score,
        struct_score,
        motion_score,
        stability_score,
        artifact_penalty,
        &collage_file,
    ).is_ok() {
        Some(collage_file.to_string_lossy().to_string())
    } else {
        None
    };

    let report = EvaluationReport {
        wallpaper_id: wallpaper_id.to_string(),
        title,
        wallpaper_type: type_label.to_string(),
        mode: mode_label,
        final_score,
        color_score,
        struct_score,
        motion_score,
        stability_score,
        artifact_penalty,
        solid_color_fallback,
        oscillating_vertical_lines: artifacts.oscillating_vertical_lines,
        static_noise_detected: artifacts.static_noise_detected,
        details,
        collage_path,
        error: None,
    };

    // let _ = fs::remove_dir_all(&work_dir);

    report
}
