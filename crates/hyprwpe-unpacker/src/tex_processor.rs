use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use byteorder::{LittleEndian, ReadBytesExt};
use image::{imageops, ImageBuffer, Rgba};
use rayon::prelude::*;
use texture2ddecoder::{decode_bc1, decode_bc2, decode_bc3, decode_bc7};

pub fn process_tex_files(dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    let mut tex_files = Vec::new();
    collect_tex_files(dir, &mut tex_files);

    tex_files.par_iter().for_each(|path| {
        process_single_tex(path);
    });
}

fn collect_tex_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_tex_files(&path, acc);
            } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("tex") {
                acc.push(path);
            }
        }
    }
}

fn is_static_noise(rgba: &[u8]) -> bool {
    if rgba.len() < 64 { return true; }
    let pixel_count = rgba.len() / 4;
    let mut buckets = vec![0u32; 4096];
    let step = (pixel_count / 10000).max(1);

    for i in (0..pixel_count).step_by(step) {
        let r = (rgba[i * 4] / 16) as usize;
        let g = (rgba[i * 4 + 1] / 16) as usize;
        let b = (rgba[i * 4 + 2] / 16) as usize;
        let idx = (r << 8) | (g << 4) | b;
        buckets[idx] += 1;
    }

    let mut unique_buckets = 0;
    let mut single_sample_buckets = 0;
    for &count in &buckets {
        if count > 0 {
            unique_buckets += 1;
            if count == 1 {
                single_sample_buckets += 1;
            }
        }
    }

    let ratio = if unique_buckets > 0 { single_sample_buckets as f32 / unique_buckets as f32 } else { 0.0 };
    unique_buckets > 400 || (unique_buckets > 250 && ratio > 0.25)
}

fn decompress_lz4_stream(src: &[u8], target_len: usize) -> Vec<u8> {
    let safe_len = target_len.min(50 * 1024 * 1024);
    let mut dst = Vec::with_capacity(safe_len);
    let mut pos = 0;

    while pos + 4 <= src.len() && dst.len() < target_len {
        let chunk_len = u32::from_le_bytes([src[pos], src[pos+1], src[pos+2], src[pos+3]]) as usize;
        pos += 4;
        let is_raw = (chunk_len & 0x80000000) != 0;
        let real_len = chunk_len & 0x7FFFFFFF;

        if real_len == 0 || pos + real_len > src.len() {
            break;
        }

        let chunk_data = &src[pos..pos + real_len];
        pos += real_len;

        if is_raw {
            dst.extend_from_slice(chunk_data);
        } else if let Ok(decomp) = lz4_flex::block::decompress(chunk_data, 128_000) {
            dst.extend_from_slice(&decomp);
        } else {
            dst.extend_from_slice(chunk_data);
        }
    }

    dst
}

fn read_null_term_string<R: Read>(r: &mut R) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 1];
    while r.read(&mut buf)? > 0 {
        if buf[0] == 0 {
            break;
        }
        bytes.push(buf[0]);
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn save_fast_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ColorType;
    use image::ImageEncoder;
    use std::io::BufWriter;

    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let encoder = PngEncoder::new_with_quality(writer, CompressionType::Fast, FilterType::NoFilter);
    encoder.write_image(rgba, width, height, ColorType::Rgba8)?;
    Ok(())
}

#[inline]
fn bgra_u32_to_rgba_bytes(rgba_u32: &[u32]) -> Vec<u8> {
    let mut rgba_u8 = Vec::with_capacity(rgba_u32.len() * 4);
    for &pixel in rgba_u32 {
        let [b, g, r, a] = pixel.to_le_bytes();
        rgba_u8.extend_from_slice(&[r, g, b, a]);
    }
    rgba_u8
}

fn decode_tex_structured(data: &[u8], png_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    if data.len() < 32 {
        return Err("File too small".into());
    }
    let mut cursor = Cursor::new(data);
    let magic_v = read_null_term_string(&mut cursor)?;
    if !magic_v.starts_with("TEXV") {
        return Err("Missing TEXV header".into());
    }
    let magic_i = read_null_term_string(&mut cursor)?;
    if !magic_i.starts_with("TEXI") {
        return Err("Missing TEXI header".into());
    }
    let format = cursor.read_u32::<LittleEndian>()?;
    let _flags = cursor.read_u32::<LittleEndian>()?;
    let _tex_w = cursor.read_u32::<LittleEndian>()?;
    let _tex_h = cursor.read_u32::<LittleEndian>()?;
    let img_w = cursor.read_u32::<LittleEndian>()?;
    let img_h = cursor.read_u32::<LittleEndian>()?;
    let _unk0 = cursor.read_u32::<LittleEndian>()?;

    let magic_b = read_null_term_string(&mut cursor)?;
    if !magic_b.starts_with("TEXB") {
        return Err("Missing TEXB container".into());
    }
    let img_count = cursor.read_u32::<LittleEndian>()?;
    if img_count == 0 {
        return Err("Zero image count".into());
    }

    let freeimage_fmt = if magic_b == "TEXB0004" {
        let fmt = cursor.read_i32::<LittleEndian>()?;
        let _flags = cursor.read_u32::<LittleEndian>()?;
        fmt
    } else if magic_b == "TEXB0003" {
        cursor.read_i32::<LittleEndian>()?
    } else if magic_b == "TEXB0001" || magic_b == "TEXB0002" {
        let _ = cursor.read_i32::<LittleEndian>()?;
        let _ = cursor.read_i32::<LittleEndian>()?;
        -1
    } else {
        -1
    };

    let mip_count = cursor.read_u32::<LittleEndian>()?;
    if mip_count == 0 {
        return Err("Zero mip count".into());
    }

    let (mip_w, mip_h, is_lz4, decomp_sz, byte_len) = if magic_b.ends_with('1') {
        let w = cursor.read_u32::<LittleEndian>()?;
        let h = cursor.read_u32::<LittleEndian>()?;
        let bl = cursor.read_u32::<LittleEndian>()?;
        (w, h, 0u32, bl, bl)
    } else {
        let w = cursor.read_u32::<LittleEndian>()?;
        let h = cursor.read_u32::<LittleEndian>()?;
        let lz4 = cursor.read_u32::<LittleEndian>()?;
        let dsz = cursor.read_u32::<LittleEndian>()?;
        let bl = cursor.read_u32::<LittleEndian>()?;
        (w, h, lz4, dsz, bl)
    };

    if byte_len == 0 || (cursor.position() as usize + byte_len as usize) > data.len() {
        return Err("Invalid byte_len or truncated data".into());
    }

    let comp_start = cursor.position() as usize;
    let comp_slice = &data[comp_start..comp_start + byte_len as usize];

    if comp_slice.starts_with(b"\x89PNG\r\n\x1a\n") {
        fs::write(png_path, comp_slice)?;
        return Ok(true);
    }
    if comp_slice.starts_with(b"\xff\xd8\xff") || freeimage_fmt == 2 {
        let j_slice = if comp_slice.starts_with(b"\xff\xd8\xff") {
            comp_slice
        } else if let Some(pos) = comp_slice.windows(3).position(|w| w == b"\xff\xd8\xff") {
            &comp_slice[pos..]
        } else {
            comp_slice
        };
        if let Ok(dyn_img) = image::load_from_memory(j_slice) {
            dyn_img.to_rgba8().save(png_path)?;
            return Ok(true);
        }
    }

    let decomp_buf;
    let decomp_slice = if is_lz4 != 0 {
        if let Ok(buf) = lz4_flex::decompress(comp_slice, decomp_sz as usize) {
            decomp_buf = buf;
            &decomp_buf[..]
        } else {
            let buf = decompress_lz4_stream(comp_slice, decomp_sz as usize);
            if !buf.is_empty() {
                decomp_buf = buf;
                &decomp_buf[..]
            } else {
                comp_slice
            }
        }
    } else {
        comp_slice
    };

    if decomp_slice.starts_with(b"\x89PNG\r\n\x1a\n") {
        fs::write(png_path, decomp_slice)?;
        return Ok(true);
    }
    if decomp_slice.starts_with(b"\xff\xd8\xff") || freeimage_fmt == 2 {
        let j_slice = if decomp_slice.starts_with(b"\xff\xd8\xff") {
            decomp_slice
        } else if let Some(pos) = decomp_slice.windows(3).position(|w| w == b"\xff\xd8\xff") {
            &decomp_slice[pos..]
        } else {
            decomp_slice
        };
        if let Ok(dyn_img) = image::load_from_memory(j_slice) {
            dyn_img.to_rgba8().save(png_path)?;
            return Ok(true);
        }
    }

    let w = mip_w as usize;
    let h = mip_h as usize;
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return Err("Invalid mip dimensions".into());
    }

    let mut final_rgba_u8: Option<Vec<u8>> = None;

    match format {
        0 => {
            // RGBA8888
            if decomp_slice.len() >= w * h * 4 {
                final_rgba_u8 = Some(decomp_slice[..w * h * 4].to_vec());
            }
        }
        4 => {
            // DXT5 / BC3
            let mut rgba_u32 = vec![0u32; w * h];
            if decode_bc3(decomp_slice, w, h, &mut rgba_u32).is_ok() {
                final_rgba_u8 = Some(bgra_u32_to_rgba_bytes(&rgba_u32));
            }
        }
        6 => {
            // DXT3 / BC2
            let mut rgba_u32 = vec![0u32; w * h];
            if decode_bc2(decomp_slice, w, h, &mut rgba_u32).is_ok() {
                final_rgba_u8 = Some(bgra_u32_to_rgba_bytes(&rgba_u32));
            }
        }
        7 => {
            // DXT1 / BC1
            let mut rgba_u32 = vec![0u32; w * h];
            if decode_bc1(decomp_slice, w, h, &mut rgba_u32).is_ok() {
                final_rgba_u8 = Some(bgra_u32_to_rgba_bytes(&rgba_u32));
            }
        }
        8 => {
            // RG88: Two-channel tangent space normal map or RG vector map.
            // R = chunk[0], G = chunk[1]. Compute Z for normal maps, A = 255.
            if decomp_slice.len() >= w * h * 2 {
                let mut rgba_u8 = Vec::with_capacity(w * h * 4);
                for chunk in decomp_slice[..w * h * 2].chunks_exact(2) {
                    let nx = (chunk[0] as f32 / 255.0) * 2.0 - 1.0;
                    let ny = (chunk[1] as f32 / 255.0) * 2.0 - 1.0;
                    let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
                    let b = ((nz * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
                    rgba_u8.extend_from_slice(&[chunk[0], chunk[1], b, 255]);
                }
                final_rgba_u8 = Some(rgba_u8);
            }
        }
        9 => {
            // R8: single-channel luminance / alpha mask
            if decomp_slice.len() >= w * h {
                let mut rgba_u8 = Vec::with_capacity(w * h * 4);
                for &b in &decomp_slice[..w * h] {
                    rgba_u8.extend_from_slice(&[b, b, b, b]);
                }
                final_rgba_u8 = Some(rgba_u8);
            }
        }
        _ => {}
    }

    if let Some(rgba_vec) = final_rgba_u8 {
        let target_w = if img_w > 0 && img_w <= mip_w { img_w } else { mip_w };
        let target_h = if img_h > 0 && img_h <= mip_h { img_h } else { mip_h };
        if target_w < mip_w || target_h < mip_h {
            if let Some(img_buf) = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(mip_w, mip_h, rgba_vec) {
                let cropped = imageops::crop_imm(&img_buf, 0, 0, target_w, target_h).to_image();
                let _ = fs::remove_file(png_path);
                save_fast_png(png_path, &cropped.into_raw(), target_w, target_h)?;
                return Ok(true);
            }
        } else {
            let _ = fs::remove_file(png_path);
            save_fast_png(png_path, &rgba_vec, mip_w, mip_h)?;
            return Ok(true);
        }
    }

    Err("Decoding format failed".into())
}

pub fn process_single_tex(tex_path: &Path) -> bool {
    let png_path = tex_path.with_extension("png");

    let is_valid = |p: &Path| -> bool {
        if p.is_file() {
            if let Ok(meta) = fs::metadata(p) {
                if meta.len() <= 100 || meta.len() == 20650 {
                    return false;
                }
                if let Ok((w, h)) = image::image_dimensions(p) {
                    return w > 1 && h > 1;
                }
            }
        }
        false
    };

    if is_valid(&png_path) {
        return true;
    }

    let data = match fs::read(tex_path) {
        Ok(d) => d,
        Err(_) => return generate_fallback_png(&png_path),
    };

    // Fast path: Structured parsing based on official TEX format
    if let Ok(true) = decode_tex_structured(&data, &png_path) {
        if is_valid(&png_path) {
            return true;
        }
    }

    let texi_idx = match data.windows(4).position(|w| w == b"TEXI") {
        Some(idx) => idx,
        None => return generate_fallback_png(&png_path),
    };

    let mut cursor = std::io::Cursor::new(&data[texi_idx + 12..]);
    let mut raw_w = cursor.read_u32::<LittleEndian>().unwrap_or(1920) as usize;
    let mut raw_h = cursor.read_u32::<LittleEndian>().unwrap_or(1080) as usize;

    if raw_w == 0 || raw_w > 8192 { raw_w = 1920; }
    if raw_h == 0 || raw_h > 8192 { raw_h = 1080; }

    let mut crop_w = raw_w;
    let mut crop_h = raw_h;
    if data.len() >= texi_idx + 28 {
        let mut crop_cursor = std::io::Cursor::new(&data[texi_idx + 20..]);
        let cw = crop_cursor.read_u32::<LittleEndian>().unwrap_or(raw_w as u32) as usize;
        let ch = crop_cursor.read_u32::<LittleEndian>().unwrap_or(raw_h as u32) as usize;
        if cw > 0 && cw <= raw_w { crop_w = cw; }
        if ch > 0 && ch <= raw_h { crop_h = ch; }
    }

    let texb_idx = match data.windows(4).position(|w| w == b"TEXB") {
        Some(idx) => idx,
        None => return generate_fallback_png(&png_path),
    };

    let mut w_candidates = Vec::new();
    let mut h_candidates = Vec::new();

    if crop_w > 0 && crop_h > 0 {
        w_candidates.push(crop_w);
        h_candidates.push(crop_h);
    }
    if raw_w > 0 && raw_w != crop_w {
        w_candidates.push(raw_w);
        h_candidates.push(raw_h);
    }
    if w_candidates.is_empty() {
        w_candidates = vec![1920, 1280];
        h_candidates = vec![1080, 720];
    }

    for off in [texb_idx + 44, texb_idx + 40, texb_idx + 36, texb_idx + 32] {
        if off >= data.len() { continue; }

        let payload = &data[off..];

        let mut decomp_payload = Vec::new();
        if let Ok(d) = lz4_flex::decompress_size_prepended(payload) {
            decomp_payload = d;
        } else if let Ok(d) = lz4_flex::decompress(payload, crop_w * crop_h / 2) {
            decomp_payload = d;
        } else {
            let d = decompress_lz4_stream(payload, crop_w * crop_h);
            if !d.is_empty() {
                decomp_payload = d;
            }
        }

        let src_slice = if !decomp_payload.is_empty() {
            &decomp_payload[..]
        } else {
            payload
        };

        for &w in &w_candidates {
            for &h in &h_candidates {
                if w == 0 || h == 0 || w > 8192 || h > 8192 { continue; }

                let bc1_len = w * h / 2;
                let bc3_len = w * h;

                let mut rgba_u32 = vec![0u32; w * h];
                let mut ok = false;

                // Priority 1: BC1 (DXT1)
                if src_slice.len() >= bc1_len && decode_bc1(&src_slice[..bc1_len], w, h, &mut rgba_u32).is_ok() {
                    let rgba_u8 = bgra_u32_to_rgba_bytes(&rgba_u32);
                    if !is_static_noise(&rgba_u8) {
                        ok = true;
                    }
                }
                // Priority 2: BC3 (DXT5)
                if !ok && src_slice.len() >= bc3_len && decode_bc3(&src_slice[..bc3_len], w, h, &mut rgba_u32).is_ok() {
                    let rgba_u8 = bgra_u32_to_rgba_bytes(&rgba_u32);
                    if !is_static_noise(&rgba_u8) {
                        ok = true;
                    }
                }
                // Priority 3: BC7
                if !ok && src_slice.len() >= bc3_len && decode_bc7(&src_slice[..bc3_len], w, h, &mut rgba_u32).is_ok() {
                    let rgba_u8 = bgra_u32_to_rgba_bytes(&rgba_u32);
                    if !is_static_noise(&rgba_u8) {
                        ok = true;
                    }
                }

                if ok {
                    let rgba_u8 = bgra_u32_to_rgba_bytes(&rgba_u32);
                    if let Some(rgba_img) = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(w as u32, h as u32, rgba_u8) {
                        let _ = fs::remove_file(&png_path);
                        if rgba_img.save(&png_path).is_ok() && is_valid(&png_path) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    // Fallback: If native decoder did not produce a valid texture, invoke RePKG to decode proprietary TEX formats
    if let Some(parent) = tex_path.parent() {
        let current_exe = std::env::current_exe().unwrap_or_default();
        let crate_dir = current_exe.parent().unwrap_or(Path::new("."));
        let repkg_candidates = [
            crate_dir.join("RePKG.exe"),
            PathBuf::from("config/RePKG.exe"),
            PathBuf::from("/home/scushi/HyprWpE-Rust/config/RePKG.exe"),
        ];

        for repkg_exe in &repkg_candidates {
            if repkg_exe.is_file() {
                let _ = std::process::Command::new("dotnet")
                    .arg(repkg_exe)
                    .arg("extract")
                    .arg(tex_path)
                    .arg("-o")
                    .arg(parent)
                    .arg("--overwrite")
                    .output();

                if is_valid(&png_path) {
                    return true;
                }
                break;
            }
        }
    }

    generate_fallback_png(&png_path)
}

fn generate_fallback_png(png_path: &Path) -> bool {
    let width = 1u32;
    let height = 1u32;
    let img = ImageBuffer::from_pixel(width, height, Rgba([0u8, 0u8, 0u8, 0u8]));
    let _ = img.save(png_path);
    true
}
