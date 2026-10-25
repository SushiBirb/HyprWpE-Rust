use image::{DynamicImage, GenericImageView, GrayImage};

fn get_candidate_crops(img: &DynamicImage, target_aspect: f32) -> Vec<DynamicImage> {
    let mut candidates = vec![img.clone()];
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 || target_aspect <= 0.0 {
        return candidates;
    }
    let current_aspect = w as f32 / h as f32;
    let ratio = (current_aspect / target_aspect).max(target_aspect / current_aspect);
    if ratio < 1.08 {
        // Aspects are nearly identical, full frame suffices
        return candidates;
    }

    if target_aspect < current_aspect {
        // Target is narrower (e.g. 1:1, 4:3 vs 16:9 widescreen)
        let crop_w = ((h as f32) * target_aspect).round() as u32;
        let crop_w = crop_w.clamp(1, w);
        let max_x = w.saturating_sub(crop_w);
        if max_x > 0 {
            for &frac in &[0.0f32, 0.25, 0.50, 0.75, 1.0] {
                let x0 = (frac * max_x as f32).round() as u32;
                candidates.push(img.crop_imm(x0, 0, crop_w, h));
            }
        }
    } else {
        // Target is wider than render
        let crop_h = ((w as f32) / target_aspect).round() as u32;
        let crop_h = crop_h.clamp(1, h);
        let max_y = h.saturating_sub(crop_h);
        if max_y > 0 {
            for &frac in &[0.0f32, 0.50, 1.0] {
                let y0 = (frac * max_y as f32).round() as u32;
                candidates.push(img.crop_imm(0, y0, w, crop_h));
            }
        }
    }
    candidates
}

fn calc_single_color_histogram_similarity(img_a: &DynamicImage, img_b: &DynamicImage) -> f32 {
    let a = img_a.resize_exact(128, 128, image::imageops::FilterType::Triangle).to_rgb8();
    let b = img_b.resize_exact(128, 128, image::imageops::FilterType::Triangle).to_rgb8();

    let mut hist_a = [0f64; 512]; // 8x8x8 3D RGB color histogram
    let mut hist_b = [0f64; 512];

    for pixel in a.pixels() {
        let r = (pixel[0] >> 5) as usize;
        let g = (pixel[1] >> 5) as usize;
        let b = (pixel[2] >> 5) as usize;
        let idx = (r * 64) + (g * 8) + b;
        hist_a[idx] += 1.0;
    }

    for pixel in b.pixels() {
        let r = (pixel[0] >> 5) as usize;
        let g = (pixel[1] >> 5) as usize;
        let b = (pixel[2] >> 5) as usize;
        let idx = (r * 64) + (g * 8) + b;
        hist_b[idx] += 1.0;
    }

    let sum_a: f64 = hist_a.iter().sum();
    let sum_b: f64 = hist_b.iter().sum();

    if sum_a > 0.0 {
        for val in &mut hist_a {
            *val /= sum_a;
        }
    }
    if sum_b > 0.0 {
        for val in &mut hist_b {
            *val /= sum_b;
        }
    }

    let mut intersection = 0.0f64;
    for i in 0..512 {
        intersection += hist_a[i].min(hist_b[i]);
    }

    // Average RGB channel mean similarity for color tone preservation
    let mut mean_a = [0.0f64; 3];
    let mut mean_b = [0.0f64; 3];
    for p in a.pixels() {
        mean_a[0] += p[0] as f64; mean_a[1] += p[1] as f64; mean_a[2] += p[2] as f64;
    }
    for p in b.pixels() {
        mean_b[0] += p[0] as f64; mean_b[1] += p[1] as f64; mean_b[2] += p[2] as f64;
    }
    let count = (128 * 128) as f64;
    mean_a[0] /= count; mean_a[1] /= count; mean_a[2] /= count;
    mean_b[0] /= count; mean_b[1] /= count; mean_b[2] /= count;

    let tone_diff = ((mean_a[0] - mean_b[0]).abs() + (mean_a[1] - mean_b[1]).abs() + (mean_a[2] - mean_b[2]).abs()) / (3.0 * 255.0);
    let tone_score = ((1.0 - tone_diff) * 100.0) as f64;

    // Luminance comparison to penalize mostly-black or heavily darkened renders
    let lum_a = mean_a[0] * 0.299 + mean_a[1] * 0.587 + mean_a[2] * 0.114;
    let lum_b = mean_b[0] * 0.299 + mean_b[1] * 0.587 + mean_b[2] * 0.114;
    let lum_factor = if lum_b > 25.0 {
        (lum_a / lum_b).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let base_color = (intersection * 100.0 * 0.70 + tone_score * 0.30).clamp(0.0, 100.0);
    let final_color_score = base_color * (0.3 + 0.7 * lum_factor);
    final_color_score.clamp(0.0, 100.0) as f32
}

pub fn calc_color_histogram_similarity(img_a: &DynamicImage, img_b: &DynamicImage) -> f32 {
    let (wb, hb) = img_b.dimensions();
    let target_aspect = if hb > 0 { wb as f32 / hb as f32 } else { 1.0 };
    let candidates = get_candidate_crops(img_a, target_aspect);
    let mut best_score = 0.0f32;
    for cand in &candidates {
        let score = calc_single_color_histogram_similarity(cand, img_b);
        if score > best_score {
            best_score = score;
        }
    }
    // Scale by 1.3 to compensate for preview compression and palette quantization
    (best_score * 1.3).clamp(0.0, 100.0)
}

fn calc_single_structural_gradient_similarity(img_a: &DynamicImage, img_b: &DynamicImage) -> f32 {
    let a: GrayImage = img_a.resize_exact(128, 128, image::imageops::FilterType::Triangle).to_luma8();
    let b: GrayImage = img_b.resize_exact(128, 128, image::imageops::FilterType::Triangle).to_luma8();

    // Box blur / de-dither on preview image to eliminate GIF error diffusion noise
    let b_blur = image::imageops::blur(&b, 0.8);

    // 1. Compute Sobel gradient magnitude
    let (w, h) = a.dimensions(); // 128 x 128
    let mut mag_a = vec![0.0f64; (w * h) as usize];
    let mut mag_b = vec![0.0f64; (w * h) as usize];

    for y in 0..(h - 1) {
        for x in 0..(w - 1) {
            let idx = (y * w + x) as usize;
            let p00_a = a.get_pixel(x, y)[0] as f64;
            let px1_a = a.get_pixel(x + 1, y)[0] as f64;
            let py1_a = a.get_pixel(x, y + 1)[0] as f64;
            let gx_a = px1_a - p00_a;
            let gy_a = py1_a - p00_a;
            mag_a[idx] = (gx_a * gx_a + gy_a * gy_a).sqrt();

            let p00_b = b_blur.get_pixel(x, y)[0] as f64;
            let px1_b = b_blur.get_pixel(x + 1, y)[0] as f64;
            let py1_b = b_blur.get_pixel(x, y + 1)[0] as f64;
            let gx_b = px1_b - p00_b;
            let gy_b = py1_b - p00_b;
            mag_b[idx] = (gx_b * gx_b + gy_b * gy_b).sqrt();
        }
    }

    // 2. Block pooling across 8x8 pixel blocks (16x16 pooled grid = 256 regions)
    let block = 8u32;
    let mut pooled_mag_a = Vec::with_capacity(256);
    let mut pooled_mag_b = Vec::with_capacity(256);
    let mut pooled_lum_a = Vec::with_capacity(256);
    let mut pooled_lum_b = Vec::with_capacity(256);

    for by in (0..h).step_by(block as usize) {
        if by + block > h { continue; }
        for bx in (0..w).step_by(block as usize) {
            if bx + block > w { continue; }
            let mut sum_mag_a = 0.0f64;
            let mut sum_mag_b = 0.0f64;
            let mut sum_lum_a = 0.0f64;
            let mut sum_lum_b = 0.0f64;
            for py in 0..block {
                for px in 0..block {
                    let idx = ((by + py) * w + (bx + px)) as usize;
                    sum_mag_a += mag_a[idx];
                    sum_mag_b += mag_b[idx];
                    sum_lum_a += a.get_pixel(bx + px, by + py)[0] as f64;
                    sum_lum_b += b_blur.get_pixel(bx + px, by + py)[0] as f64;
                }
            }
            let count = (block * block) as f64;
            pooled_mag_a.push(sum_mag_a / count);
            pooled_mag_b.push(sum_mag_b / count);
            pooled_lum_a.push(sum_lum_a / count);
            pooled_lum_b.push(sum_lum_b / count);
        }
    }

    fn pearson_corr(x: &[f64], y: &[f64]) -> f64 {
        let n = x.len() as f64;
        if n < 2.0 { return 0.0; }
        let mx: f64 = x.iter().sum::<f64>() / n;
        let my: f64 = y.iter().sum::<f64>() / n;
        let mut cov = 0.0f64;
        let mut var_x = 0.0f64;
        let mut var_y = 0.0f64;
        for (&xi, &yi) in x.iter().zip(y.iter()) {
            let dx = xi - mx;
            let dy = yi - my;
            cov += dx * dy;
            var_x += dx * dx;
            var_y += dy * dy;
        }
        if var_x < 1e-4 || var_y < 1e-4 {
            return 0.0;
        }
        (cov / (var_x.sqrt() * var_y.sqrt())).clamp(-1.0, 1.0)
    }

    let corr_mag = pearson_corr(&pooled_mag_a, &pooled_mag_b);
    let corr_lum = pearson_corr(&pooled_lum_a, &pooled_lum_b);
    let macro_corr_score = ((corr_mag.max(0.0) * 0.50 + corr_lum.max(0.0) * 0.50) * 100.0) as f32;

    // 3. Compute 8x8 block SSIM
    let block_size = 8u32;
    let c1 = 6.5025f64;
    let c2 = 58.5225f64;
    let mut ssim_sum = 0.0f64;
    let mut num_blocks = 0usize;

    for by in (0..h).step_by(block_size as usize) {
        if by + block_size > h { continue; }
        for bx in (0..w).step_by(block_size as usize) {
            if bx + block_size > w { continue; }

            let mut sum_a = 0.0f64;
            let mut sum_b = 0.0f64;
            let count = (block_size * block_size) as f64;

            for y in 0..block_size {
                for x in 0..block_size {
                    sum_a += a.get_pixel(bx + x, by + y)[0] as f64;
                    sum_b += b_blur.get_pixel(bx + x, by + y)[0] as f64;
                }
            }

            let mu_a = sum_a / count;
            let mu_b = sum_b / count;

            let mut var_a = 0.0f64;
            let mut var_b = 0.0f64;
            let mut cov_ab = 0.0f64;

            for y in 0..block_size {
                for x in 0..block_size {
                    let da = a.get_pixel(bx + x, by + y)[0] as f64 - mu_a;
                    let db = b_blur.get_pixel(bx + x, by + y)[0] as f64 - mu_b;
                    var_a += da * da;
                    var_b += db * db;
                    cov_ab += da * db;
                }
            }

            let sigma_a_sq = var_a / count;
            let sigma_b_sq = var_b / count;
            let sigma_ab = cov_ab / count;

            let numerator = (2.0 * mu_a * mu_b + c1) * (2.0 * sigma_ab + c2);
            let denominator = (mu_a * mu_a + mu_b * mu_b + c1) * (sigma_a_sq + sigma_b_sq + c2);

            let ssim = if denominator > 0.0 {
                numerator / denominator
            } else {
                1.0
            };

            ssim_sum += ssim.clamp(-1.0, 1.0);
            num_blocks += 1;
        }
    }

    let ssim_score = if num_blocks > 0 {
        ((ssim_sum / (num_blocks as f64)).max(0.0) * 100.0) as f32
    } else {
        0.0f32
    };

    if macro_corr_score >= 99.0 && ssim_score >= 98.0 {
        return 100.0;
    }

    macro_corr_score.max(ssim_score * 0.40 + macro_corr_score * 0.60)
}

pub fn calc_structural_gradient_similarity(img_a: &DynamicImage, img_b: &DynamicImage) -> f32 {
    let (wb, hb) = img_b.dimensions();
    let target_aspect = if hb > 0 { wb as f32 / hb as f32 } else { 1.0 };
    let candidates = get_candidate_crops(img_a, target_aspect);
    let mut best_score = 0.0f32;
    for cand in &candidates {
        let score = calc_single_structural_gradient_similarity(cand, img_b);
        if score > best_score {
            best_score = score;
        }
    }
    // Scale by 1.4 to compensate for resolution downsampling and crop borders
    (best_score * 1.4).clamp(0.0, 100.0)
}

pub fn calc_motion_dynamics(frame0: &DynamicImage, frame1: &DynamicImage, _frame2: Option<&DynamicImage>) -> f32 {
    let f0 = frame0.resize_exact(128, 128, image::imageops::FilterType::Triangle).to_rgb8();
    let f1 = frame1.resize_exact(128, 128, image::imageops::FilterType::Triangle).to_rgb8();

    let mut mad01 = 0.0f64;
    for (p0, p1) in f0.pixels().zip(f1.pixels()) {
        let dr = (p0[0] as f64 - p1[0] as f64).abs();
        let dg = (p0[1] as f64 - p1[1] as f64).abs();
        let db = (p0[2] as f64 - p1[2] as f64).abs();
        mad01 += (dr + dg + db) / 3.0;
    }
    mad01 /= (128 * 128) as f64;

    let norm_diff = mad01 / 255.0;
    let score = if norm_diff < 0.0005 {
        // Virtually static
        15.0
    } else if norm_diff < 0.003 {
        // Subtle ambient motion
        50.0 + (norm_diff / 0.003) * 25.0
    } else if norm_diff <= 0.15 {
        // Good dynamic animated scene
        75.0 + ((norm_diff - 0.003) / 0.147).min(1.0) * 22.0
    } else {
        // Wild fluctuation / strobe
        50.0
    };
    score as f32
}
