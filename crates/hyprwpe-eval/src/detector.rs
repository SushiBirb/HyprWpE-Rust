use std::collections::HashMap;
use image::DynamicImage;

#[derive(Debug, Clone, Default)]
pub struct ArtifactPenalties {
    pub solid_color_fallback: bool,
    pub oscillating_vertical_lines: bool,
    pub empty_or_black_screen: bool,
    pub static_noise_detected: bool,
    pub partial_canvas_render: bool,
    pub total_penalty: f32,
    pub details: Vec<String>,
}

pub fn detect_artifacts(img: &DynamicImage) -> ArtifactPenalties {
    let mut penalties = ArtifactPenalties::default();
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();

    if w == 0 || h == 0 {
        penalties.empty_or_black_screen = true;
        penalties.total_penalty += 70.0;
        penalties.details.push("Empty/Zero dimension canvas".to_string());
        return penalties;
    }

    // 1. Calculate RGB Mean and Standard Deviation
    let mut sum_r = 0.0f64;
    let mut sum_g = 0.0f64;
    let mut sum_b = 0.0f64;
    let count = (w * h) as f64;

    for p in rgb.pixels() {
        sum_r += p[0] as f64;
        sum_g += p[1] as f64;
        sum_b += p[2] as f64;
    }

    let mean_r = sum_r / count;
    let mean_g = sum_g / count;
    let mean_b = sum_b / count;

    let mut var_r = 0.0f64;
    let mut var_g = 0.0f64;
    let mut var_b = 0.0f64;

    for p in rgb.pixels() {
        var_r += (p[0] as f64 - mean_r).powi(2);
        var_g += (p[1] as f64 - mean_g).powi(2);
        var_b += (p[2] as f64 - mean_b).powi(2);
    }

    let std_r = (var_r / count).sqrt();
    let std_g = (var_g / count).sqrt();
    let std_b = (var_b / count).sqrt();
    let avg_std = (std_r + std_g + std_b) / 3.0;

    // Detect Black Screen or Severely Dark Missing Canvas
    let mut pitch_black_pixels = 0usize;
    for p in rgb.pixels() {
        if p[0] < 5 && p[1] < 5 && p[2] < 5 {
            pitch_black_pixels += 1;
        }
    }
    let pitch_black_ratio = pitch_black_pixels as f64 / count;

    if mean_r < 2.0 && mean_g < 2.0 && mean_b < 2.0 {
        penalties.empty_or_black_screen = true;
        penalties.total_penalty += 80.0;
        penalties.details.push("Black screen detected (mean RGB < 2)".to_string());
    } else if pitch_black_ratio > 0.85 {
        penalties.empty_or_black_screen = true;
        penalties.total_penalty += 60.0;
        penalties.details.push(format!("Missing background / black canvas ({:.1}% pixels pitch black)", pitch_black_ratio * 100.0));
    } else if pitch_black_ratio > 0.65 {
        penalties.partial_canvas_render = true;
        penalties.total_penalty += 35.0;
        penalties.details.push(format!("Partial render / dark void ({:.1}% pixels pitch black)", pitch_black_ratio * 100.0));
    }

    // Detect Solid Single Color Fallback
    let is_fallback_lightblue = (mean_r - 100.0).abs() < 15.0
        && (mean_g - 140.0).abs() < 15.0
        && (mean_b - 200.0).abs() < 15.0;

    if avg_std < 1.0 || is_fallback_lightblue {
        penalties.solid_color_fallback = true;
        penalties.total_penalty += 60.0;
        penalties.details.push(format!(
            "Solid color fallback detected (std_dev={avg_std:.2}, mean_RGB=[{mean_r:.1}, {mean_g:.1}, {mean_b:.1}])"
        ));
    }

    // 2. Detect Random Noise Bucket Entropy (Decoder Corruption vs Natural Artwork)
    if w >= 32 && h >= 32 {
        let mut color_histogram: HashMap<(u8, u8, u8), usize> = HashMap::new();
        let step = 8;

        for y in (0..h).step_by(step) {
            for x in (0..w).step_by(step) {
                let p = rgb.get_pixel(x, y);
                let key = (p[0] / 16, p[1] / 16, p[2] / 16);
                *color_histogram.entry(key).or_insert(0) += 1;
            }
        }

        let unique_buckets = color_histogram.len();
        let single_sample_buckets = color_histogram.values().filter(|&&cnt| cnt <= 2).count();
        let noise_entropy_ratio = single_sample_buckets as f64 / unique_buckets.max(1) as f64;

        // To distinguish true decoder noise/corruption from smooth artistic gradients,
        // measure average adjacent pixel variation (local gradient high-frequency energy).
        let mut diff_sum = 0.0f64;
        let mut diff_count = 0usize;
        for y in (0..h).step_by(step) {
            for x in (0..w.saturating_sub(1)).step_by(step) {
                let p1 = rgb.get_pixel(x, y);
                let p2 = rgb.get_pixel(x + 1, y);
                let d = ((p1[0] as f64 - p2[0] as f64).abs()
                    + (p1[1] as f64 - p2[1] as f64).abs()
                    + (p1[2] as f64 - p2[2] as f64).abs()) / 3.0;
                diff_sum += d;
                diff_count += 1;
            }
        }
        let avg_neighbor_diff = if diff_count > 0 { diff_sum / diff_count as f64 } else { 0.0 };

        if unique_buckets > 300 && noise_entropy_ratio > 0.55 && avg_neighbor_diff > 25.0 {
            penalties.static_noise_detected = true;
            penalties.total_penalty += 100.0;
            penalties.details.push(format!(
                "Random decoder static noise detected (entropy_ratio={:.2}, unique_buckets={}, avg_diff={:.1})",
                noise_entropy_ratio, unique_buckets, avg_neighbor_diff
            ));
        }
    }

    // 3. Detect Oscillating Vertical Lines
    if w >= 16 && h >= 16 {
        let mut col_means = vec![0.0f64; w as usize];
        let mut row_means = vec![0.0f64; h as usize];

        for y in 0..h {
            for x in 0..w {
                let p = rgb.get_pixel(x, y);
                let lum = (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0;
                col_means[x as usize] += lum;
                row_means[y as usize] += lum;
            }
        }

        for val in &mut col_means {
            *val /= h as f64;
        }
        for val in &mut row_means {
            *val /= w as f64;
        }

        let col_mean_avg: f64 = col_means.iter().sum::<f64>() / col_means.len() as f64;
        let row_mean_avg: f64 = row_means.iter().sum::<f64>() / row_means.len() as f64;

        let col_var: f64 = col_means.iter().map(|v| (v - col_mean_avg).powi(2)).sum::<f64>() / col_means.len() as f64;
        let row_var: f64 = row_means.iter().map(|v| (v - row_mean_avg).powi(2)).sum::<f64>() / row_means.len() as f64;

        let mut col_flips = 0;
        for i in 1..col_means.len() - 1 {
            let d1 = col_means[i] - col_means[i - 1];
            let d2 = col_means[i + 1] - col_means[i];
            if (d1 * d2) < -1.0 {
                col_flips += 1;
            }
        }

        let stripe_ratio = if row_var > 0.001 {
            col_var / row_var
        } else if col_var > 1.0 {
            100.0
        } else {
            0.0
        };

        if stripe_ratio > 3.0 && col_flips > (w as usize / 4) {
            penalties.oscillating_vertical_lines = true;
            penalties.total_penalty += 50.0;
            penalties.details.push(format!(
                "Oscillating vertical line artifact detected (stripe_ratio={stripe_ratio:.2}, col_flips={col_flips})"
            ));
        }
    }

    penalties
}
