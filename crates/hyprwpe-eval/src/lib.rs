pub mod analyzer;
pub mod collage;
pub mod detector;
pub mod evaluator;

pub use analyzer::{calc_color_histogram_similarity, calc_motion_dynamics, calc_structural_gradient_similarity};
pub use collage::generate_comparison_collage;
pub use detector::{detect_artifacts, ArtifactPenalties};
pub use evaluator::{
    detect_wallpaper_info, evaluate_wallpaper, evaluate_wallpaper_with_options, EvaluationMode,
    EvaluationOptions, EvaluationReport,
};

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgb};

    #[test]
    fn test_solid_color_fallback_detection() {
        // Create solid light blue fallback image (100, 140, 200)
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(128, 128, Rgb([100, 140, 200])));
        let penalties = detect_artifacts(&img);

        assert!(penalties.solid_color_fallback);
        assert!(penalties.total_penalty >= 50.0);
    }

    #[test]
    fn test_oscillating_vertical_stripes_detection() {
        // Create synthetic alternating vertical stripes (0, 255, 0, 255...)
        let mut img_buf = ImageBuffer::new(128, 128);
        for y in 0..128 {
            for x in 0..128 {
                let color = if x % 2 == 0 { 255 } else { 0 };
                img_buf.put_pixel(x, y, Rgb([color, color, color]));
            }
        }
        let img = DynamicImage::ImageRgb8(img_buf);
        let penalties = detect_artifacts(&img);

        assert!(penalties.oscillating_vertical_lines);
        assert!(penalties.total_penalty >= 50.0);
    }

    #[test]
    fn test_identical_image_similarity_scores() {
        let mut img_buf = ImageBuffer::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let r = (x * 4) as u8;
                let g = (y * 4) as u8;
                img_buf.put_pixel(x, y, Rgb([r, g, 128]));
            }
        }
        let img = DynamicImage::ImageRgb8(img_buf);

        let color_sim = calc_color_histogram_similarity(&img, &img);
        let struct_sim = calc_structural_gradient_similarity(&img, &img);

        assert!((color_sim - 100.0).abs() < 1.0);
        assert!((struct_sim - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_black_screen_versus_colorful_preview_scores_low() {
        let mut preview_buf = ImageBuffer::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                preview_buf.put_pixel(x, y, Rgb([50, 180, 120]));
            }
        }
        let preview = DynamicImage::ImageRgb8(preview_buf);
        let black_render = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(64, 64, Rgb([0, 0, 0])));

        let color_sim = calc_color_histogram_similarity(&black_render, &preview);
        let struct_sim = calc_structural_gradient_similarity(&black_render, &preview);

        // Color and structure similarity between black and colorful scene must be low
        assert!(color_sim < 25.0, "Expected color_sim < 25.0, got {color_sim}");
        assert!(struct_sim < 20.0, "Expected struct_sim < 20.0, got {struct_sim}");
    }

    #[test]
    fn test_smooth_gradient_does_not_trigger_static_noise() {
        // Create smooth multi-hue diagonal gradient
        let mut img_buf = ImageBuffer::new(256, 256);
        for y in 0..256 {
            for x in 0..256 {
                let r = (x % 256) as u8;
                let g = (y % 256) as u8;
                let b = ((x + y) / 2) as u8;
                img_buf.put_pixel(x, y, Rgb([r, g, b]));
            }
        }
        let img = DynamicImage::ImageRgb8(img_buf);
        let penalties = detect_artifacts(&img);

        assert!(!penalties.static_noise_detected);
    }

    #[test]
    fn test_high_frequency_random_noise_triggers_static_noise() {
        // Create high-frequency pseudorandom hash noise
        let mut img_buf = ImageBuffer::new(256, 256);
        let mut seed: u32 = 0x12345678;
        for y in 0..256 {
            for x in 0..256 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let r = (seed >> 16) as u8;
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let g = (seed >> 16) as u8;
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let b = (seed >> 16) as u8;
                img_buf.put_pixel(x, y, Rgb([r, g, b]));
            }
        }
        let img = DynamicImage::ImageRgb8(img_buf);
        let penalties = detect_artifacts(&img);

        assert!(penalties.static_noise_detected);
        assert!(penalties.total_penalty >= 90.0);
    }
}
