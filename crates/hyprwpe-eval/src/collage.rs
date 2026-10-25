use std::path::Path;
use image::{DynamicImage, ImageBuffer, Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};

pub fn generate_comparison_collage(
    preview_img: Option<&DynamicImage>,
    rendered_img: Option<&DynamicImage>,
    wallpaper_id: &str,
    title: &str,
    wp_type: &str,
    mode_label: &str,
    final_score: f32,
    color_score: f32,
    struct_score: f32,
    motion_score: f32,
    stability_score: f32,
    penalty: f32,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let target_w = 640u32;
    let target_h = 360u32;
    let canvas_w = target_w * 2 + 60;
    let canvas_h = target_h + 120;

    let mut canvas: RgbImage = ImageBuffer::from_pixel(canvas_w, canvas_h, Rgb([24, 26, 32]));

    // Paste Preview
    if let Some(prev) = preview_img {
        let resized = prev.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut canvas, &resized, 20, 80);
    } else {
        let fallback: RgbImage = ImageBuffer::from_pixel(target_w, target_h, Rgb([40, 44, 52]));
        image::imageops::overlay(&mut canvas, &fallback, 20, 80);
    }

    // Paste Rendered Frame
    if let Some(rend) = rendered_img {
        let resized = rend.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut canvas, &resized, (target_w + 40) as i64, 80);
    } else {
        let fallback: RgbImage = ImageBuffer::from_pixel(target_w, target_h, Rgb([40, 44, 52]));
        image::imageops::overlay(&mut canvas, &fallback, (target_w + 40) as i64, 80);
    }

    // Render Text Headers & Score Overlay
    let font_bytes = include_bytes!("../assets/DejaVuSans.ttf");
    if let Some(font) = Font::try_from_bytes(font_bytes) {
        let scale_header = Scale::uniform(20.0);
        let scale_sub = Scale::uniform(16.0);

        let title_text = format!("[{wp_type}] {title} (ID: {wallpaper_id})");
        let score_text = format!("SCORE: {final_score:.1} / 100");

        let score_color = if final_score >= 70.0 {
            Rgb([80, 220, 120])
        } else if final_score >= 50.0 {
            Rgb([240, 200, 80])
        } else {
            Rgb([240, 80, 80])
        };

        draw_text_mut(&mut canvas, Rgb([255, 255, 255]), 20, 15, scale_header, &font, &title_text);
        draw_text_mut(&mut canvas, Rgb([160, 160, 160]), 20, 45, scale_sub, &font, "Steam Preview Asset");
        let right_label = format!("Rendered Output ({mode_label})");
        draw_text_mut(&mut canvas, Rgb([160, 160, 160]), (target_w + 40) as i32, 45, scale_sub, &font, &right_label);

        draw_text_mut(&mut canvas, score_color, (canvas_w - 280) as i32, 15, scale_header, &font, &score_text);

        let breakdown = format!(
            "Color Sim: {color_score:.1}% | Structure: {struct_score:.1}% | Motion: {motion_score:.1}% | Stability: {stability_score:.1}% | Penalty: -{penalty:.1}"
        );
        draw_text_mut(&mut canvas, Rgb([200, 220, 255]), 20, (canvas_h - 35) as i32, scale_sub, &font, &breakdown);
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    canvas.save(output_path)?;
    Ok(())
}
