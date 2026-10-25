use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde_json::Value;

use crate::glsl_preprocess::preprocess_program;

pub const EMBEDDED_SCENE_TEMPLATE: &str = include_str!("../../../config/scene_template.html");

fn is_valid_clean_png(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    if let Ok(meta) = fs::metadata(p) {
        if meta.len() <= 100 || meta.len() == 20650 {
            return false;
        }
    }
    if let Ok((w, h)) = image::image_dimensions(p) {
        return w > 1 && h > 1;
    }
    false
}

fn parse_vec3(val: Option<&Value>, default: [f64; 3]) -> [f64; 3] {
    match val {
        Some(Value::String(s)) => {
            let nums: Vec<f64> = s.split_whitespace().filter_map(|p| p.parse::<f64>().ok()).collect();
            if nums.len() >= 3 {
                [nums[0], nums[1], nums[2]]
            } else if nums.len() == 2 {
                [nums[0], nums[1], default[2]]
            } else if nums.len() == 1 {
                [nums[0], nums[0], nums[0]]
            } else {
                default
            }
        }
        Some(Value::Array(arr)) => {
            let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
            if nums.len() >= 3 {
                [nums[0], nums[1], nums[2]]
            } else if nums.len() == 2 {
                [nums[0], nums[1], default[2]]
            } else if nums.len() == 1 {
                [nums[0], nums[0], nums[0]]
            } else {
                default
            }
        }
        _ => default,
    }
}

fn compute_world_transform(
    parent_id: Option<i64>,
    local_origin: [f64; 3],
    local_scale: [f64; 3],
    parent_map: &HashMap<i64, (Option<i64>, [f64; 3], [f64; 3])>,
) -> ([f64; 3], [f64; 3]) {
    let mut cur_origin = local_origin;
    let mut cur_scale = local_scale;
    let mut cur_parent = parent_id;

    let mut depth = 0;
    while let Some(pid) = cur_parent {
        if depth > 20 { break; }
        if let Some((next_parent, p_orig, p_scale)) = parent_map.get(&pid) {
            cur_origin[0] = p_orig[0] + cur_origin[0] * p_scale[0];
            cur_origin[1] = p_orig[1] + cur_origin[1] * p_scale[1];
            cur_origin[2] = p_orig[2] + cur_origin[2] * p_scale[2];
            cur_scale[0] *= p_scale[0];
            cur_scale[1] *= p_scale[1];
            cur_scale[2] *= p_scale[2];
            cur_parent = *next_parent;
        } else {
            break;
        }
        depth += 1;
    }

    (cur_origin, cur_scale)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SubmeshGeometry {
    pub material: String,
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub texcoords: Vec<f32>,
    pub indices: Vec<u16>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ModelGeometry {
    pub submeshes: Vec<SubmeshGeometry>,
}

pub fn parse_mdl_geometry(bytes: &[u8]) -> Option<ModelGeometry> {
    if bytes.len() < 24 || !bytes.starts_with(b"MDLV") {
        return None;
    }
    let version: u32 = std::str::from_utf8(&bytes[4..8])
        .ok()?
        .parse()
        .ok()?;

    let fmt = u32::from_le_bytes(bytes[9..13].try_into().ok()?);
    let _unk = u32::from_le_bytes(bytes[13..17].try_into().ok()?);
    let num_submeshes = u32::from_le_bytes(bytes[17..21].try_into().ok()?);

    let mut off = 21;
    let mut submeshes = Vec::new();

    for _ in 0..num_submeshes {
        if off >= bytes.len() {
            break;
        }
        let end = bytes[off..].iter().position(|&b| b == 0).map(|p| off + p)?;
        let mat_str = std::str::from_utf8(&bytes[off..end]).unwrap_or("").to_string();
        off = end + 1;

        let (stride, v_bytes, is_puppet, sm_fmt) = if version >= 16 {
            if off + 12 > bytes.len() { break; }
            let _sm_id = u32::from_le_bytes(bytes[off..off+4].try_into().ok()?);
            let s_fmt = u32::from_le_bytes(bytes[off+4..off+8].try_into().ok()?);
            let vb = u32::from_le_bytes(bytes[off+8..off+12].try_into().ok()?);
            off += 12;
            let is_p = s_fmt == 0x01800009 || s_fmt == 0x0180000f;
            let s = if s_fmt == 15 { 48 } else if s_fmt == 0x0180000f { 80 } else if is_p { 52 } else { 48 };
            (s, vb as usize, is_p, s_fmt)
        } else {
            if off + 8 > bytes.len() { break; }
            let _sm_id = u32::from_le_bytes(bytes[off..off+4].try_into().ok()?);
            let vb = u32::from_le_bytes(bytes[off+4..off+8].try_into().ok()?);
            off += 8;
            let is_p = fmt == 0x01800009;
            let s = if fmt == 15 { 48 } else if is_p { 52 } else { 48 };
            (s, vb as usize, is_p, fmt)
        };

        if off + v_bytes > bytes.len() || stride == 0 { break; }
        let num_verts = v_bytes / stride;
        let mut positions = Vec::with_capacity(num_verts * 3);
        let mut normals = Vec::with_capacity(num_verts * 3);
        let mut texcoords = Vec::with_capacity(num_verts * 2);

        for v in 0..num_verts {
            let v_start = off + v * stride;
            let px = f32::from_le_bytes(bytes[v_start..v_start+4].try_into().unwrap());
            let py = f32::from_le_bytes(bytes[v_start+4..v_start+8].try_into().unwrap());
            let pz = f32::from_le_bytes(bytes[v_start+8..v_start+12].try_into().unwrap());
            positions.push(px);
            positions.push(py);
            positions.push(pz);

            if is_puppet {
                if stride == 52 {
                    let u = f32::from_le_bytes(bytes[v_start+44..v_start+48].try_into().unwrap());
                    let v_coord = f32::from_le_bytes(bytes[v_start+48..v_start+52].try_into().unwrap());
                    texcoords.push(u);
                    texcoords.push(v_coord);
                } else if stride == 80 {
                    let nx = f32::from_le_bytes(bytes[v_start+12..v_start+16].try_into().unwrap());
                    let ny = f32::from_le_bytes(bytes[v_start+16..v_start+20].try_into().unwrap());
                    let nz = f32::from_le_bytes(bytes[v_start+20..v_start+24].try_into().unwrap());
                    normals.push(nx);
                    normals.push(ny);
                    normals.push(nz);
                    let u = f32::from_le_bytes(bytes[v_start+40..v_start+44].try_into().unwrap());
                    let v_coord = f32::from_le_bytes(bytes[v_start+44..v_start+48].try_into().unwrap());
                    texcoords.push(u);
                    texcoords.push(v_coord);
                }
            } else if sm_fmt == 15 {
                let nx = f32::from_le_bytes(bytes[v_start+12..v_start+16].try_into().unwrap());
                let ny = f32::from_le_bytes(bytes[v_start+16..v_start+20].try_into().unwrap());
                let nz = f32::from_le_bytes(bytes[v_start+20..v_start+24].try_into().unwrap());
                normals.push(nx);
                normals.push(ny);
                normals.push(nz);

                let u = f32::from_le_bytes(bytes[v_start+40..v_start+44].try_into().unwrap());
                let v_coord = f32::from_le_bytes(bytes[v_start+44..v_start+48].try_into().unwrap());
                texcoords.push(u);
                texcoords.push(v_coord);
            }
        }
        off += v_bytes;

        if off + 4 > bytes.len() { break; }
        let idx_bytes = u32::from_le_bytes(bytes[off..off+4].try_into().ok()?) as usize;
        off += 4;
        if off + idx_bytes > bytes.len() { break; }
        let num_indices = idx_bytes / 2;
        let mut indices = Vec::with_capacity(num_indices);
        for i in 0..num_indices {
            let idx = u16::from_le_bytes(bytes[off+i*2..off+i*2+2].try_into().unwrap());
            indices.push(idx);
        }
        off += idx_bytes;

        submeshes.push(SubmeshGeometry {
            material: mat_str,
            positions,
            normals,
            texcoords,
            indices,
        });
    }

    if submeshes.is_empty() {
        None
    } else {
        Some(ModelGeometry { submeshes })
    }
}

pub fn extract_material_from_mdl(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 24 || !bytes.starts_with(b"MDLV") {
        return None;
    }
    let mut i = 16;
    while i < bytes.len().min(512) {
        if bytes[i..].starts_with(b"materials/") {
            let start = i;
            let mut end = start;
            while end < bytes.len() && bytes[end] != 0 && bytes[end] >= 32 && bytes[end] <= 126 {
                end += 1;
            }
            if let Ok(s) = std::str::from_utf8(&bytes[start..end]) {
                if s.ends_with(".json") || s.ends_with(".mat") {
                    return Some(s.to_string());
                }
            }
        }
        i += 1;
    }
    None
}

fn find_mdl_path_for_object(obj: &Value, unpacked_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(s) = obj.get("model").and_then(|v| v.as_str()) {
        candidates.push(s.to_string());
    }
    if let Some(s) = obj.get("puppet").and_then(|v| v.as_str()) {
        candidates.push(s.to_string());
    }
    if let Some(s) = obj.get("image").and_then(|v| v.as_str()) {
        candidates.push(s.to_string());
    }

    for cand in candidates {
        let clean = cand.replace('\\', "/");
        if clean.ends_with(".mdl") {
            let p = unpacked_dir.join(&clean);
            if p.is_file() {
                return Some(p);
            }
        } else if clean.ends_with(".json") {
            let p = unpacked_dir.join(&clean);
            if p.is_file() {
                if let Ok(raw) = fs::read_to_string(&p) {
                    if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                        if let Some(puppet) = val.get("puppet").and_then(|v| v.as_str()) {
                            let pup_clean = puppet.replace('\\', "/");
                            let pup_p = unpacked_dir.join(&pup_clean);
                            if pup_p.is_file() {
                                return Some(pup_p);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_material_for_object(obj: &Value, unpacked_dir: &Path) -> Option<(PathBuf, Value)> {
    let mut candidates = Vec::new();

    if let Some(mat_str) = obj.get("material").and_then(|v| v.as_str()) {
        candidates.push(mat_str.to_string());
    }
    if let Some(materials) = obj.get("materials").and_then(|v| v.as_array()) {
        for m in materials {
            if let Some(s) = m.as_str() {
                candidates.push(s.to_string());
            }
        }
    }
    if let Some(img_str) = obj.get("image").and_then(|v| v.as_str()) {
        candidates.push(img_str.to_string());
    }
    if let Some(mdl_str) = obj.get("model").and_then(|v| v.as_str()) {
        candidates.push(mdl_str.to_string());
    }

    for cand in candidates {
        let clean = cand.replace('\\', "/");
        if clean.ends_with(".mdl") {
            let mdl_path = unpacked_dir.join(&clean);
            if mdl_path.is_file() {
                if let Ok(bytes) = fs::read(&mdl_path) {
                    if let Some(mat_ref) = extract_material_from_mdl(&bytes) {
                        let mat_p = unpacked_dir.join(&mat_ref);
                        if mat_p.is_file() {
                            if let Ok(raw) = fs::read_to_string(&mat_p) {
                                if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                                    return Some((mat_p, val));
                                }
                            }
                        }
                    }
                }
            }
            let stem = Path::new(&clean).file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !stem.is_empty() {
                let direct_mat = unpacked_dir.join(format!("materials/{stem}/{stem}.json"));
                if direct_mat.is_file() {
                    if let Ok(raw) = fs::read_to_string(&direct_mat) {
                        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                            return Some((direct_mat, val));
                        }
                    }
                }
            }
        } else if clean.ends_with(".json") {
            let json_p = unpacked_dir.join(&clean);
            if json_p.is_file() {
                if let Ok(raw) = fs::read_to_string(&json_p) {
                    if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                        if val.get("passes").is_some() {
                            return Some((json_p, val));
                        }
                        if let Some(mat_ref) = val.get("material").and_then(|v| v.as_str()) {
                            let mat_clean = mat_ref.replace('\\', "/");
                            let mat_p = unpacked_dir.join(&mat_clean);
                            if mat_p.is_file() {
                                if let Ok(mraw) = fs::read_to_string(&mat_p) {
                                    if let Ok(mval) = serde_json::from_str::<Value>(&mraw) {
                                        return Some((mat_p, mval));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

pub(crate) fn find_effect_shaders(
    file_ref: &str,
    unpacked_dir: &Path,
    shaders_dir: Option<&Path>,
) -> Option<(PathBuf, PathBuf, String)> {
    let clean = file_ref.replace('\\', "/");
    let base = clean
        .trim_end_matches("/effect.json")
        .trim_end_matches(".json");
    let short_name = base.split('/').last().unwrap_or(base);

    // 1. Check if clean file is an effect.json on disk
    let effect_json_path = unpacked_dir.join(&clean);
    if effect_json_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&effect_json_path) {
            if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                // Check dependencies for .vert and .frag
                if let Some(deps) = json.get("dependencies").and_then(|v| v.as_array()) {
                    let mut vert_cand = None;
                    let mut frag_cand = None;
                    for dep in deps {
                        if let Some(s) = dep.as_str() {
                            let s_clean = s.replace('\\', "/");
                            if s_clean.ends_with(".vert") {
                                vert_cand = Some(s_clean);
                            } else if s_clean.ends_with(".frag") {
                                frag_cand = Some(s_clean);
                            }
                        }
                    }
                    if let (Some(v), Some(f)) = (vert_cand, frag_cand) {
                        let vp = unpacked_dir.join(&v);
                        let fp = unpacked_dir.join(&f);
                        if vp.is_file() && fp.is_file() {
                            return Some((vp, fp, short_name.to_string()));
                        }
                    }
                }
                // Check passes -> material -> shader
                if let Some(passes) = json.get("passes").and_then(|v| v.as_array()) {
                    if let Some(mat_ref) = passes.first().and_then(|p| p.get("material")).and_then(|v| v.as_str()) {
                        let mat_clean = mat_ref.replace('\\', "/");
                        let mat_path = unpacked_dir.join(&mat_clean);
                        if mat_path.is_file() {
                            if let Ok(mraw) = fs::read_to_string(&mat_path) {
                                if let Ok(mjson) = serde_json::from_str::<Value>(&mraw) {
                                    if let Some(mpasses) = mjson.get("passes").and_then(|v| v.as_array()) {
                                        if let Some(shader_name) = mpasses.first().and_then(|p| p.get("shader")).and_then(|v| v.as_str()) {
                                            let s_clean = shader_name.replace('\\', "/");
                                            for cand in &[
                                                format!("shaders/{s_clean}"),
                                                s_clean.clone(),
                                                format!("shaders/effects/{s_clean}"),
                                            ] {
                                                let vp = unpacked_dir.join(format!("{cand}.vert"));
                                                let fp = unpacked_dir.join(format!("{cand}.frag"));
                                                if vp.is_file() && fp.is_file() {
                                                    return Some((vp, fp, short_name.to_string()));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Direct path candidates in unpacked_dir
    let candidates = [
        format!("shaders/{base}"),
        format!("{base}"),
        format!("shaders/effects/{short_name}"),
        format!("effects/{short_name}"),
        format!("shaders/{short_name}"),
    ];

    for cand in &candidates {
        let vp = unpacked_dir.join(format!("{cand}.vert"));
        let fp = unpacked_dir.join(format!("{cand}.frag"));
        if vp.is_file() && fp.is_file() {
            return Some((vp, fp, short_name.to_string()));
        }
    }

    // 3. Global shaders_dir candidates
    if let Some(s_dir) = shaders_dir {
        for cand in &candidates {
            let vp = s_dir.join(format!("{cand}.vert"));
            let fp = s_dir.join(format!("{cand}.frag"));
            if vp.is_file() && fp.is_file() {
                return Some((vp, fp, short_name.to_string()));
            }
        }
    }

    None
}

fn resolve_prop_value(val: &Value, project_props: &Value) -> Option<Value> {
    if let Some(prop_key) = val.as_str() {
        if let Some(prop_obj) = project_props.get(prop_key) {
            if let Some(v) = prop_obj.get("value") {
                return Some(v.clone());
            }
            return Some(prop_obj.clone());
        }
        if prop_key.split_whitespace().any(|s| s.parse::<f64>().is_ok()) {
            return Some(val.clone());
        }
        return None;
    }
    Some(val.clone())
}

pub fn generate_scene_html(unpacked_dir: &Path) -> bool {
    let scene_json_path = unpacked_dir.join("scene.json");
    if !scene_json_path.exists() {
        return false;
    }

    let scene_raw = match fs::read_to_string(&scene_json_path) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let scene_json: Value = match serde_json::from_str(&scene_raw) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let project_props = {
        let pjson_path = unpacked_dir.join("project.json");
        if let Ok(raw) = fs::read_to_string(&pjson_path) {
            if let Ok(pjson) = serde_json::from_str::<Value>(&raw) {
                pjson.get("general")
                    .and_then(|g| g.get("properties"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        } else {
            serde_json::Value::Null
        }
    };

    let util_dir = unpacked_dir.join("util");
    let _ = fs::create_dir_all(&util_dir);
    let white_png = util_dir.join("white.png");
    if !white_png.exists() {
        let img = image::ImageBuffer::from_pixel(4u32, 4u32, image::Rgba([255u8, 255u8, 255u8, 255u8]));
        let _ = img.save(&white_png);
    }

    let shaders_dir = PathBuf::from("config/shaders");

    let proj_w = scene_json.get("general")
        .and_then(|g| g.get("orthogonalprojection"))
        .and_then(|p| p.get("width").and_then(|v| v.as_f64()))
        .unwrap_or(1920.0);
    let proj_h = scene_json.get("general")
        .and_then(|g| g.get("orthogonalprojection"))
        .and_then(|p| p.get("height").and_then(|v| v.as_f64()))
        .unwrap_or(1080.0);
    let projection = serde_json::json!([proj_w, proj_h]);

    let clearcolor_val = scene_json.get("general")
        .and_then(|g| g.get("clearcolor"))
        .or_else(|| scene_json.get("clearcolor"));
    let clearcolor_parsed = clearcolor_val.map(|v| {
        let s = if let Some(val) = v.get("value").and_then(|val| val.as_str()) {
            val
        } else if let Some(s) = v.as_str() {
            s
        } else {
            ""
        };
        let c = parse_vec3(Some(&serde_json::Value::String(s.to_string())), [0.0, 0.0, 0.0]);
        serde_json::json!([c[0], c[1], c[2]])
    }).unwrap_or(serde_json::json!([0.0, 0.0, 0.0]));

    let user_properties = scene_json.get("general")
        .and_then(|g| g.get("properties"))
        .cloned()
        .filter(|p| p.is_object() && !p.as_object().unwrap().is_empty())
        .unwrap_or(project_props.clone());

    let mut scene_data = serde_json::json!({
        "camera": scene_json.get("camera").cloned().unwrap_or(serde_json::Value::Null),
        "general": scene_json.get("general").cloned().unwrap_or(serde_json::Value::Null),
        "projection": projection,
        "clearcolor": clearcolor_parsed,
        "ambient": scene_json.get("ambient").cloned().unwrap_or(serde_json::json!([0.3, 0.3, 0.3])),
        "cameraparallax": scene_json.get("general").and_then(|g| g.get("cameraparallax")).and_then(|v| v.as_bool()).unwrap_or(true),
        "cameraparallaxamount": scene_json.get("general").and_then(|g| g.get("cameraparallaxamount")).and_then(|v| {
            if let Some(num) = v.as_f64() { Some(num) }
            else { v.get("value").and_then(|val| val.as_f64()) }
        }).unwrap_or(0.05),
        "cameraparallaxmouseinfluence": scene_json.get("general").and_then(|g| g.get("cameraparallaxmouseinfluence")).and_then(|v| v.as_f64()).unwrap_or(-0.25),
        "properties": user_properties,
        "objects": []
    });

    if let Some(objects) = scene_json.get("objects").and_then(|v| v.as_array()) {
        let mut processed_objects = Vec::new();
        let mut found_first_image = false;

        let mut parent_map: HashMap<i64, (Option<i64>, [f64; 3], [f64; 3])> = HashMap::new();
        for obj in objects {
            if let Some(id) = obj.get("id").and_then(|v| v.as_i64()) {
                let parent_id = obj.get("parent").and_then(|v| v.as_i64());
                let local_orig = parse_vec3(obj.get("origin"), [0.0, 0.0, 0.0]);
                let local_sc = parse_vec3(obj.get("scale"), [1.0, 1.0, 1.0]);
                parent_map.insert(id, (parent_id, local_orig, local_sc));
            }
        }

        let mut hidden_ids = std::collections::HashSet::new();
        for obj in objects {
            let id = obj.get("id").and_then(|v| v.as_i64());
            let mut hidden = false;
            if let Some(b) = obj.get("visible").and_then(|v| v.as_bool()) {
                if !b { hidden = true; }
            } else if let Some(val) = obj.get("visible").and_then(|v| v.get("value")).and_then(|v| v.as_bool()) {
                if !val { hidden = true; }
            }

            if hidden {
                if let Some(i) = id {
                    hidden_ids.insert(i);
                }
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            for obj in objects {
                if let Some(id) = obj.get("id").and_then(|v| v.as_i64()) {
                    if !hidden_ids.contains(&id) {
                        if let Some(parent) = obj.get("parent").and_then(|v| v.as_i64()) {
                            if hidden_ids.contains(&parent) {
                                hidden_ids.insert(id);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }

        for obj in objects {
            let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("object");
            let particle_ref = obj.get("particle").or_else(|| obj.get("particlesrc")).and_then(|v| v.as_str());
            let is_text = obj.get("text").is_some();
            let kind = if particle_ref.is_some() || obj.get("kind").and_then(|v| v.as_str()) == Some("particle") {
                "particle"
            } else if is_text {
                "text"
            } else {
                obj.get("kind").and_then(|v| v.as_str()).unwrap_or("image")
            };
            let mut visible = if let Some(b) = obj.get("visible").and_then(|v| v.as_bool()) {
                b
            } else if let Some(val) = obj.get("visible").and_then(|v| v.get("value")).and_then(|v| v.as_bool()) {
                val
            } else {
                true
            };

            let obj_id = obj.get("id").and_then(|v| v.as_i64());
            if let Some(i) = obj_id {
                if hidden_ids.contains(&i) {
                    visible = false;
                }
            }

            let lower_name = name.to_ascii_lowercase();
            let is_audio = kind == "sound"
                || obj.get("sound").is_some()
                || lower_name.ends_with(".mp3")
                || lower_name.ends_with(".ogg")
                || lower_name.ends_with(".wav")
                || lower_name.ends_with(".flac");

            let has_renderable = particle_ref.is_some()
                || is_text
                || obj.get("image").is_some()
                || obj.get("model").is_some()
                || obj.get("puppet").is_some()
                || obj.get("solidlayer").is_some()
                || obj.get("effects").and_then(|v| v.as_array()).map(|a| !a.is_empty()).unwrap_or(false);

            if kind == "folder" || is_audio || !has_renderable {
                visible = false;
            }

            let has_animated_alpha = obj.get("alpha").and_then(|v| v.get("animation")).is_some();
            let alpha = if has_animated_alpha {
                1.0
            } else if let Some(a) = obj.get("alpha").and_then(|v| v.as_f64()) {
                a
            } else if let Some(val) = obj.get("alpha").and_then(|v| v.get("value")).and_then(|v| v.as_f64()) {
                val
            } else {
                1.0
            };
            if alpha <= 0.001 && !has_animated_alpha {
                visible = false;
            }

            if kind == "particle" && visible {
                let p_rel = particle_ref.unwrap_or("").replace('\\', "/");
                let mut pj_val: Option<Value> = None;
                let we_assets = PathBuf::from("/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets");
                let preset_name = p_rel.trim_start_matches("particles/").trim_start_matches("presets/").trim_end_matches(".json").split('/').next().unwrap_or("");
                let mut cand_paths = vec![
                    unpacked_dir.join(&p_rel),
                    unpacked_dir.join(format!("{p_rel}.json")),
                    we_assets.join(&p_rel),
                    we_assets.join(format!("{p_rel}.json")),
                ];
                if !preset_name.is_empty() {
                    cand_paths.push(we_assets.join("presets").join(preset_name).join(&p_rel));
                    cand_paths.push(we_assets.join("presets").join(preset_name).join(format!("particles/presets/{preset_name}.json")));
                    cand_paths.push(we_assets.join("presets").join(preset_name).join(format!("{preset_name}.json")));
                }
                for cand in &cand_paths {
                    if cand.is_file() {
                        if let Ok(raw) = fs::read_to_string(cand) {
                            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                                pj_val = Some(val);
                                break;
                            }
                        }
                    }
                }

                if let Some(pj) = pj_val {
                    let maxcount = pj.get("maxcount").and_then(|v| v.as_i64()).unwrap_or(100);
                    let starttime = pj.get("starttime").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let mat_ref = pj.get("material").and_then(|v| v.as_str()).unwrap_or("");
                    let mat_clean = mat_ref.replace('\\', "/");
                    let mat_preset = mat_clean.trim_start_matches("materials/").trim_start_matches("presets/").trim_end_matches(".json").split('/').next().unwrap_or(preset_name);

                    let mut mj_val: Option<Value> = None;
                    let mut mat_cands = vec![
                        unpacked_dir.join(&mat_clean),
                        unpacked_dir.join(format!("{mat_clean}.json")),
                        we_assets.join(&mat_clean),
                        we_assets.join(format!("{mat_clean}.json")),
                    ];
                    if !mat_preset.is_empty() {
                        mat_cands.push(we_assets.join("presets").join(mat_preset).join(&mat_clean));
                        mat_cands.push(we_assets.join("presets").join(mat_preset).join(format!("materials/presets/{mat_preset}.json")));
                        mat_cands.push(we_assets.join("presets").join(mat_preset).join(format!("{mat_preset}.json")));
                    }
                    for mcand in &mat_cands {
                        if mcand.is_file() {
                            if let Ok(raw) = fs::read_to_string(mcand) {
                                if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                                    mj_val = Some(val);
                                    break;
                                }
                            }
                        }
                    }

                    let mut blending = "additive".to_string();
                    let mut particle_tex = String::new();
                    let mut overbright = 1.0;
                    let mut is_refract = false;

                    if let Some(mj) = mj_val {
                        if let Some(pass) = mj.get("passes").and_then(|v| v.as_array()).and_then(|p| p.first()) {
                            if let Some(combos) = pass.get("combos") {
                                if combos.get("REFRACT").and_then(|v| v.as_i64()) == Some(1) {
                                    is_refract = true;
                                }
                            }
                            if let Some(b) = pass.get("blending").and_then(|v| v.as_str()) {
                                blending = b.to_string();
                            }
                            if let Some(ob) = pass.get("constantshadervalues").and_then(|v| v.get("ui_editor_properties_overbright")).and_then(|v| v.as_f64()) {
                                overbright = ob;
                            }
                            if let Some(texs) = pass.get("textures").and_then(|v| v.as_array()) {
                                if let Some(first_t) = texs.first().and_then(|v| v.as_str()) {
                                    let (rel_t, _) = resolve_texture_path(first_t, unpacked_dir, false);
                                    particle_tex = rel_t;
                                }
                            }
                        }
                    }

                    if is_refract || particle_tex.contains("blank") {
                        visible = false;
                    }

                    if particle_tex.is_empty() {
                        particle_tex = "util/white.png".to_string();
                    }

                    let parent_id = obj.get("parent").and_then(|v| v.as_i64());
                    let local_orig = parse_vec3(obj.get("origin"), [0.0, 0.0, 0.0]);
                    let local_sc = parse_vec3(obj.get("scale"), [1.0, 1.0, 1.0]);
                    let (wo, ws) = compute_world_transform(parent_id, local_orig, local_sc, &parent_map);

                    let inst_override = obj.get("instanceoverride").cloned().unwrap_or(serde_json::Value::Null);
                    let particle_obj = serde_json::json!({
                        "id": obj.get("id").cloned().unwrap_or(serde_json::Value::Null),
                        "name": name,
                        "kind": "particle",
                        "visible": visible,
                        "origin": serde_json::json!([wo[0], wo[1], wo[2]]),
                        "scale": serde_json::json!([ws[0], ws[1], ws[2]]),
                        "angles": obj.get("angles").cloned().unwrap_or(serde_json::json!([0, 0, 0])),
                        "color": obj.get("color").cloned().unwrap_or(serde_json::json!([1, 1, 1])),
                        "alpha": alpha,
                        "instanceoverride": inst_override,
                        "parallaxDepth": obj.get("parallaxDepth").cloned().unwrap_or(serde_json::json!([0.5, 0.5])),
                        "maxcount": maxcount,
                        "starttime": starttime,
                        "material": {
                            "blending": blending,
                            "texture": particle_tex,
                            "overbright": overbright
                        },
                        "emitter": pj.get("emitter").cloned().unwrap_or(serde_json::json!([])),
                        "initializer": pj.get("initializer").cloned().unwrap_or(serde_json::json!([])),
                        "operator": pj.get("operator").cloned().unwrap_or(serde_json::json!([])),
                        "renderer": pj.get("renderer").cloned().unwrap_or(serde_json::json!([]))
                    });

                    processed_objects.push(particle_obj);

                    if let Some(children) = pj.get("children").and_then(|v| v.as_array()) {
                        for child in children {
                            let child_name_ref = child.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            if child_name_ref.is_empty() { continue; }
                            let child_clean = child_name_ref.replace('\\', "/");
                            let child_preset = child_clean.trim_start_matches("particles/").trim_start_matches("presets/").trim_end_matches(".json").split('/').next().unwrap_or("");
                            let mut child_cand_paths = vec![
                                unpacked_dir.join(&child_clean),
                                unpacked_dir.join(format!("{child_clean}.json")),
                                we_assets.join(&child_clean),
                                we_assets.join(format!("{child_clean}.json")),
                            ];
                            if !child_preset.is_empty() {
                                child_cand_paths.push(we_assets.join("presets").join(child_preset).join(&child_clean));
                                child_cand_paths.push(we_assets.join("presets").join(child_preset).join(format!("particles/presets/{child_preset}.json")));
                                child_cand_paths.push(we_assets.join("presets").join(child_preset).join(format!("{child_preset}.json")));
                            }
                            let mut cpj_val: Option<Value> = None;
                            for cand in &child_cand_paths {
                                if cand.is_file() {
                                    if let Ok(raw) = fs::read_to_string(cand) {
                                        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                                            cpj_val = Some(val);
                                            break;
                                        }
                                    }
                                }
                            }
                            if let Some(cpj) = cpj_val {
                                let c_maxcount = cpj.get("maxcount").and_then(|v| v.as_i64()).unwrap_or(100);
                                let c_starttime = cpj.get("starttime").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                let c_mat_ref = cpj.get("material").and_then(|v| v.as_str()).unwrap_or("");
                                let c_mat_clean = c_mat_ref.replace('\\', "/");
                                let c_mat_preset = c_mat_clean.trim_start_matches("materials/").trim_start_matches("presets/").trim_end_matches(".json").split('/').next().unwrap_or(child_preset);
                                let mut cmj_val: Option<Value> = None;
                                let mut c_mat_cands = vec![
                                    unpacked_dir.join(&c_mat_clean),
                                    unpacked_dir.join(format!("{c_mat_clean}.json")),
                                    we_assets.join(&c_mat_clean),
                                    we_assets.join(format!("{c_mat_clean}.json")),
                                ];
                                if !c_mat_preset.is_empty() {
                                    c_mat_cands.push(we_assets.join("presets").join(c_mat_preset).join(&c_mat_clean));
                                    c_mat_cands.push(we_assets.join("presets").join(c_mat_preset).join(format!("materials/presets/{c_mat_preset}.json")));
                                    c_mat_cands.push(we_assets.join("presets").join(c_mat_preset).join(format!("{c_mat_preset}.json")));
                                }
                                for mcand in &c_mat_cands {
                                    if mcand.is_file() {
                                        if let Ok(raw) = fs::read_to_string(mcand) {
                                            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                                                cmj_val = Some(val);
                                                break;
                                            }
                                        }
                                    }
                                }
                                let mut c_blending = "additive".to_string();
                                let mut c_tex = String::new();
                                let mut c_overbright = 1.0;
                                if let Some(cmj) = cmj_val {
                                    if let Some(pass) = cmj.get("passes").and_then(|v| v.as_array()).and_then(|p| p.first()) {
                                        if let Some(b) = pass.get("blending").and_then(|v| v.as_str()) {
                                            c_blending = b.to_string();
                                        }
                                        if let Some(ob) = pass.get("constantshadervalues").and_then(|v| v.get("ui_editor_properties_overbright")).and_then(|v| v.as_f64()) {
                                            c_overbright = ob;
                                        }
                                        if let Some(texs) = pass.get("textures").and_then(|v| v.as_array()) {
                                            if let Some(first_t) = texs.first().and_then(|v| v.as_str()) {
                                                let (rel_t, _) = resolve_texture_path(first_t, unpacked_dir, false);
                                                c_tex = rel_t;
                                            }
                                        }
                                    }
                                }
                                if c_tex.is_empty() { c_tex = "util/white.png".to_string(); }

                                let child_obj = serde_json::json!({
                                    "id": child.get("id").cloned().unwrap_or(serde_json::Value::Null),
                                    "name": format!("{}_{}", name, child.get("id").and_then(|i| i.as_i64()).unwrap_or(0)),
                                    "kind": "particle",
                                    "visible": visible,
                                    "parent_particle_id": obj.get("id").cloned().unwrap_or(serde_json::Value::Null),
                                    "child_type": child.get("type").cloned().unwrap_or(serde_json::Value::Null),
                                    "origin": serde_json::json!([wo[0], wo[1], wo[2]]),
                                    "scale": serde_json::json!([ws[0], ws[1], ws[2]]),
                                    "angles": obj.get("angles").cloned().unwrap_or(serde_json::json!([0, 0, 0])),
                                    "color": obj.get("color").cloned().unwrap_or(serde_json::json!([1, 1, 1])),
                                    "alpha": alpha,
                                    "instanceoverride": obj.get("instanceoverride").cloned().unwrap_or(serde_json::Value::Null),
                                    "parallaxDepth": obj.get("parallaxDepth").cloned().unwrap_or(serde_json::json!([0.5, 0.5])),
                                    "maxcount": c_maxcount,
                                    "starttime": c_starttime,
                                    "material": {
                                        "blending": c_blending,
                                        "texture": c_tex,
                                        "overbright": c_overbright
                                    },
                                    "emitter": cpj.get("emitter").cloned().unwrap_or(serde_json::json!([])),
                                    "initializer": cpj.get("initializer").cloned().unwrap_or(serde_json::json!([])),
                                    "operator": cpj.get("operator").cloned().unwrap_or(serde_json::json!([])),
                                    "renderer": cpj.get("renderer").cloned().unwrap_or(serde_json::json!([]))
                                });
                                processed_objects.push(child_obj);
                            }
                        }
                    }
                    continue;
                }
            }

            if kind == "text" && visible {
                let parent_id = obj.get("parent").and_then(|v| v.as_i64());
                let local_orig = parse_vec3(obj.get("origin"), [proj_w * 0.5, proj_h * 0.5, 0.0]);
                let local_sc = parse_vec3(obj.get("scale"), [1.0, 1.0, 1.0]);
                let (wo, ws) = compute_world_transform(parent_id, local_orig, local_sc, &parent_map);

                let text_val = obj.get("text").cloned().unwrap_or(serde_json::Value::String("".to_string()));
                let pointsize = obj.get("pointsize").and_then(|v| v.as_f64()).unwrap_or(36.0);
                let halign = obj.get("horizontalalign").and_then(|v| v.as_str()).unwrap_or("left");
                let valign = obj.get("verticalalign").and_then(|v| v.as_str()).unwrap_or("center");
                let font = obj.get("font").and_then(|v| v.as_str()).unwrap_or("sans-serif");

                let text_obj = serde_json::json!({
                    "name": name,
                    "kind": "text",
                    "visible": visible,
                    "origin": serde_json::json!([wo[0], wo[1], wo[2]]),
                    "scale": serde_json::json!([ws[0], ws[1], ws[2]]),
                    "angles": obj.get("angles").cloned().unwrap_or(serde_json::json!([0, 0, 0])),
                    "size": obj.get("size").cloned().unwrap_or(serde_json::json!([500.0, 200.0])),
                    "color": obj.get("color").cloned().unwrap_or(serde_json::json!([1, 1, 1])),
                    "alpha": alpha,
                    "parallaxDepth": obj.get("parallaxDepth").cloned().unwrap_or(serde_json::json!([0.5, 0.5])),
                    "text": text_val,
                    "pointsize": pointsize,
                    "horizontalalign": halign,
                    "verticalalign": valign,
                    "font": font,
                });
                processed_objects.push(text_obj);
                continue;
            }

            let brightness = if let Some(b) = obj.get("brightness").and_then(|v| v.as_f64()) {
                b
            } else if let Some(val) = obj.get("brightness").and_then(|v| v.get("value")).and_then(|v| v.as_f64()) {
                val
            } else {
                1.0
            };

            let blend = if let Some(b) = obj.get("blend").and_then(|v| v.as_str()) {
                b.to_string()
            } else if let Some(b) = obj.get("blending").and_then(|v| v.as_str()) {
                b.to_string()
            } else if let Some(bm) = obj.get("colorBlendMode").or_else(|| obj.get("blendMode")).and_then(|v| v.as_i64()) {
                match bm {
                    9 | 31 => "additive".to_string(),
                    7 => "screen".to_string(),
                    2 => "multiply".to_string(),
                    _ => "translucent".to_string(),
                }
            } else {
                "translucent".to_string()
            };

            let image_str = obj.get("image").and_then(|v| v.as_str()).unwrap_or("");
            let is_solid_model = image_str.contains("solidlayer");
            let is_util_model = image_str.contains("models/util/")
                || image_str.contains("fullscreenlayer")
                || image_str.contains("composelayer")
                || image_str.contains("projectlayer")
                || is_solid_model;
            let has_image_ref = (obj.get("image").is_some() || obj.get("materials").is_some()) && !is_util_model;
            let is_first_bg = !found_first_image && has_image_ref && visible;
            if is_first_bg {
                found_first_image = true;
            }

            let (tex_rel, _tex_abs) = resolve_object_texture(obj, unpacked_dir, is_first_bg);

            let parent_id = obj.get("parent").and_then(|v| v.as_i64());
            let has_origin = obj.get("origin").is_some();
            let (origin_val, scale_val) = if parent_id.is_some() || has_origin {
                let local_orig = parse_vec3(obj.get("origin"), [0.0, 0.0, 0.0]);
                let local_sc = parse_vec3(obj.get("scale"), [1.0, 1.0, 1.0]);
                let (wo, ws) = compute_world_transform(parent_id, local_orig, local_sc, &parent_map);
                (serde_json::json!([wo[0], wo[1], wo[2]]), serde_json::json!([ws[0], ws[1], ws[2]]))
            } else {
                (serde_json::Value::Null, obj.get("scale").cloned().unwrap_or(serde_json::json!([1, 1, 1])))
            };

            let user_vis = obj.get("visible")
                .and_then(|v| v.get("user"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string());

            let copy_bg = obj.get("copybackground").and_then(|v| v.as_bool())
                .or_else(|| obj.get("config").and_then(|c| c.get("passthrough")).and_then(|v| v.as_bool()))
                .unwrap_or(false);

            let mut obj_data = serde_json::json!({
                "id": obj.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "name": name,
                "kind": kind,
                "visible": visible,
                "user_vis": user_vis,
                "isFirstBg": is_first_bg,
                "isUtilLayer": is_util_model,
                "solidlayer": is_solid_model,
                "copybackground": copy_bg,
                "origin": origin_val,
                "size": obj.get("size").cloned().unwrap_or(serde_json::Value::Null),
                "scale": scale_val,
                "angles": obj.get("angles").cloned().unwrap_or(serde_json::json!([0, 0, 0])),
                "color": obj.get("color").cloned().unwrap_or(serde_json::json!([1, 1, 1])),
                "alpha": alpha,
                "brightness": brightness,
                "blend": blend,
                "alignment": obj.get("alignment").and_then(|v| v.as_str()).unwrap_or("center"),
                "parallaxDepth": obj.get("parallaxDepth").cloned().unwrap_or(serde_json::json!([0.5, 0.5])),
                "texture": tex_rel,
                "effects": []
            });

            if let Some(mdl_path) = find_mdl_path_for_object(obj, unpacked_dir) {
                if let Ok(bytes) = fs::read(&mdl_path) {
                    if let Some(geom) = parse_mdl_geometry(&bytes) {
                        obj_data["mesh"] = serde_json::to_value(&geom).unwrap_or(serde_json::Value::Null);
                    }
                }
            }

            let mut processed_effects = Vec::new();

            // Check if object has a material pass shader
            if let Some((_mat_path, mat_json)) = find_material_for_object(obj, unpacked_dir) {
                if let Some(passes) = mat_json.get("passes").and_then(|v| v.as_array()) {
                    if let Some(first_pass) = passes.first() {
                        if let Some(shader_name) = first_pass.get("shader").and_then(|v| v.as_str()) {
                            if let Some((vert_path, frag_path, short_name)) = find_effect_shaders(shader_name, unpacked_dir, Some(&shaders_dir)) {
                                let mut bound_textures = HashMap::new();
                                let mut eff_textures = serde_json::Map::new();
                                let mut extra_combos = HashMap::new();
                                let mut pass_values = HashMap::new();

                                if let Some(textures) = first_pass.get("textures").and_then(|v| v.as_array()) {
                                    for (slot, tex_val) in textures.iter().enumerate() {
                                        if let Some(tex_ref) = tex_val.as_str() {
                                            if tex_ref.starts_with("_rt_") || tex_ref == "previous" {
                                                bound_textures.insert(slot, tex_ref.to_string());
                                                eff_textures.insert(slot.to_string(), serde_json::Value::String(tex_ref.to_string()));
                                            } else {
                                                let (t_rel, _) = resolve_texture_path(tex_ref, unpacked_dir, false);
                                                bound_textures.insert(slot, t_rel.clone());
                                                eff_textures.insert(slot.to_string(), serde_json::Value::String(t_rel));
                                            }
                                        }
                                    }
                                }

                                if let Some(combos) = first_pass.get("combos").and_then(|v| v.as_object()) {
                                    for (k, v) in combos {
                                        if let Some(val) = v.as_i64() {
                                            extra_combos.insert(k.clone(), val as i32);
                                        }
                                    }
                                }

                                for key in &["constantshadervalues", "usershadervalues", "uniforms"] {
                                    if let Some(obj_vals) = first_pass.get(*key).and_then(|v| v.as_object()) {
                                        for (k, v) in obj_vals {
                                            if let Some(resolved) = resolve_prop_value(v, &project_props) {
                                                pass_values.insert(k.clone(), resolved);
                                            }
                                        }
                                    }
                                }

                                if let Ok(program) = preprocess_program(
                                    &vert_path,
                                    &frag_path,
                                    &bound_textures,
                                    Some(&extra_combos),
                                    Some(&shaders_dir),
                                ) {
                                    let mut eff_uniforms = serde_json::Map::new();

                                    for u in &program.uniforms {
                                        let mut resolved_val = None;
                                        if let Some(mat) = &u.material {
                                            if let Some(v) = pass_values.get(mat) {
                                                resolved_val = Some(v.clone());
                                            }
                                        }
                                        if resolved_val.is_none() {
                                            if let Some(v) = pass_values.get(&u.name) {
                                                resolved_val = Some(v.clone());
                                            }
                                        }
                                        if resolved_val.is_none() {
                                            let stripped = if u.name.starts_with("g_") || u.name.starts_with("u_") {
                                                &u.name[2..]
                                            } else {
                                                &u.name[..]
                                            };
                                            if let Some(v) = pass_values.get(stripped) {
                                                resolved_val = Some(v.clone());
                                            } else if let Some(v) = pass_values.get(&stripped.to_ascii_lowercase()) {
                                                resolved_val = Some(v.clone());
                                            } else {
                                                for (k, v) in &pass_values {
                                                    if k.eq_ignore_ascii_case(stripped) || k.eq_ignore_ascii_case(&u.name) {
                                                        resolved_val = Some(v.clone());
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                        if resolved_val.is_none() {
                                            if let Some(def) = &u.default {
                                                resolved_val = Some(def.clone());
                                            }
                                        }

                                        if let Some(val) = resolved_val {
                                            let coerced = crate::glsl_preprocess::coerce_value(&val, &u.utype);
                                            eff_uniforms.insert(u.name.clone(), coerced);
                                        }
                                    }

                                    processed_effects.push(serde_json::json!({
                                        "name": short_name,
                                        "vert": program.vert_glsl,
                                        "frag": program.frag_glsl,
                                        "combos": program.combos,
                                        "textures": eff_textures,
                                        "uniforms": eff_uniforms
                                    }));

                                    if let Some(b) = first_pass.get("blending").and_then(|v| v.as_str()) {
                                        obj_data["blend"] = serde_json::json!(b);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(effects) = obj.get("effects").and_then(|v| v.as_array()) {
                for eff in effects {
                    let is_eff_vis = if let Some(b) = eff.get("visible").and_then(|v| v.as_bool()) {
                        b
                    } else if let Some(vis_obj) = eff.get("visible").and_then(|v| v.as_object()) {
                        if let Some(user_cond) = vis_obj.get("user") {
                            let u_name = user_cond.get("name").and_then(|v| v.as_str());
                            let u_cond = user_cond.get("condition").and_then(|v| v.as_str());
                            if let (Some(name), Some(cond)) = (u_name, u_cond) {
                                if let Some(prop_val) = user_properties.get(name).and_then(|p| p.get("value")) {
                                    let prop_str = match prop_val {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Number(n) => n.to_string(),
                                        serde_json::Value::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
                                        _ => String::new(),
                                    };
                                    prop_str == cond
                                } else {
                                    vis_obj.get("value").and_then(|v| v.as_bool()).unwrap_or(true)
                                }
                            } else {
                                vis_obj.get("value").and_then(|v| v.as_bool()).unwrap_or(true)
                            }
                        } else {
                            vis_obj.get("value").and_then(|v| v.as_bool()).unwrap_or(true)
                        }
                    } else {
                        true
                    };
                    if !is_eff_vis {
                        continue;
                    }

                    if let Some(file_ref) = eff.get("file").and_then(|v| v.as_str()) {
                        if let Some((vert_path, frag_path, short_name)) = find_effect_shaders(file_ref, unpacked_dir, Some(&shaders_dir)) {
                            let mut bound_textures = HashMap::new();
                            let mut eff_textures = serde_json::Map::new();
                            let mut extra_combos = HashMap::new();
                            let mut pass_values = HashMap::new();

                            if let Some(passes) = eff.get("passes").and_then(|v| v.as_array()) {
                                for pass in passes {
                                    if let Some(textures) = pass.get("textures") {
                                        if let Some(arr) = textures.as_array() {
                                            for (slot, tex_val) in arr.iter().enumerate() {
                                                if let Some(tex_ref) = tex_val.as_str() {
                                                    let (t_rel, _) = resolve_texture_path(tex_ref, unpacked_dir, false);
                                                    bound_textures.insert(slot, t_rel.clone());
                                                    eff_textures.insert(slot.to_string(), serde_json::Value::String(t_rel));
                                                }
                                            }
                                        } else if let Some(map) = textures.as_object() {
                                            for (k, v) in map {
                                                if let Ok(slot) = k.parse::<usize>() {
                                                    if let Some(tex_ref) = v.as_str() {
                                                        let (t_rel, _) = resolve_texture_path(tex_ref, unpacked_dir, false);
                                                        bound_textures.insert(slot, t_rel.clone());
                                                        eff_textures.insert(k.clone(), serde_json::Value::String(t_rel));
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    if let Some(bind) = pass.get("bind").and_then(|v| v.as_array()) {
                                        for b in bind {
                                            if let (Some(bname), Some(idx)) = (b.get("name").and_then(|v| v.as_str()), b.get("index").and_then(|v| v.as_u64())) {
                                                let slot = idx as usize;
                                                let (t_rel, _) = resolve_texture_path(bname, unpacked_dir, false);
                                                bound_textures.insert(slot, t_rel.clone());
                                                eff_textures.insert(slot.to_string(), serde_json::Value::String(t_rel));
                                            }
                                        }
                                    }

                                    if let Some(combos) = pass.get("combos").and_then(|v| v.as_object()) {
                                        for (k, v) in combos {
                                            if let Some(val) = v.as_i64() {
                                                extra_combos.insert(k.clone(), val as i32);
                                            }
                                        }
                                    }

                                    for key in &["constantshadervalues", "usershadervalues", "uniforms"] {
                                        if let Some(obj) = pass.get(*key).and_then(|v| v.as_object()) {
                                            for (k, v) in obj {
                                                if let Some(resolved) = resolve_prop_value(v, &project_props) {
                                                    pass_values.insert(k.clone(), resolved);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let clean_file = file_ref.replace('\\', "/");
                            let effect_json_obj = {
                                let cand1 = unpacked_dir.join(&clean_file);
                                let cand2 = PathBuf::from("/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets").join(&clean_file);
                                let p = if cand1.is_file() { Some(cand1) } else if cand2.is_file() { Some(cand2) } else { None };
                                p.and_then(|path| fs::read_to_string(path).ok()).and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                            };

                            if let Some(ref eff_doc) = effect_json_obj {
                                if let Some(eff_passes) = eff_doc.get("passes").and_then(|v| v.as_array()) {
                                    let target_pass = eff_passes.iter().find(|p| {
                                        p.get("material").and_then(|m| m.as_str()).map(|s| s.contains("combine")).unwrap_or(false)
                                    }).or_else(|| eff_passes.last());

                                    if let Some(tp) = target_pass {
                                        if let Some(bind) = tp.get("bind").and_then(|v| v.as_array()) {
                                            for b in bind {
                                                if let (Some(bname), Some(idx)) = (b.get("name").and_then(|v| v.as_str()), b.get("index").and_then(|v| v.as_u64())) {
                                                    let slot = idx as usize;
                                                    let (t_rel, _) = resolve_texture_path(bname, unpacked_dir, false);
                                                    bound_textures.insert(slot, t_rel.clone());
                                                    eff_textures.insert(slot.to_string(), serde_json::Value::String(t_rel));
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if let Ok(program) = preprocess_program(
                                &vert_path,
                                &frag_path,
                                &bound_textures,
                                Some(&extra_combos),
                                Some(&shaders_dir),
                            ) {
                                let mut eff_uniforms = serde_json::Map::new();

                                for u in &program.uniforms {
                                    let mut resolved_val = None;
                                    if let Some(mat) = &u.material {
                                        if let Some(v) = pass_values.get(mat) {
                                            resolved_val = Some(v.clone());
                                        }
                                    }
                                    if resolved_val.is_none() {
                                        if let Some(v) = pass_values.get(&u.name) {
                                            resolved_val = Some(v.clone());
                                        }
                                    }
                                    if resolved_val.is_none() {
                                        let stripped = if u.name.starts_with("g_") || u.name.starts_with("u_") {
                                            &u.name[2..]
                                        } else {
                                            &u.name[..]
                                        };
                                        if let Some(v) = pass_values.get(stripped) {
                                            resolved_val = Some(v.clone());
                                        } else if let Some(v) = pass_values.get(&stripped.to_ascii_lowercase()) {
                                            resolved_val = Some(v.clone());
                                        } else {
                                            for (k, v) in &pass_values {
                                                if k.eq_ignore_ascii_case(stripped) || k.eq_ignore_ascii_case(&u.name) {
                                                    resolved_val = Some(v.clone());
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    if resolved_val.is_none() {
                                        if let Some(def) = &u.default {
                                            resolved_val = Some(def.clone());
                                        }
                                    }

                                    if let Some(val) = resolved_val {
                                        let coerced = crate::glsl_preprocess::coerce_value(&val, &u.utype);
                                        eff_uniforms.insert(u.name.clone(), coerced);
                                    }
                                }

                                processed_effects.push(serde_json::json!({
                                    "name": short_name,
                                    "vert": program.vert_glsl,
                                    "frag": program.frag_glsl,
                                    "combos": program.combos,
                                    "textures": eff_textures,
                                    "uniforms": eff_uniforms
                                }));
                            }
                        }
                    }
                }
            }

            // Fallback texture from material effect if base texture is empty or a render target
            let cur_tex = obj_data["texture"].as_str().unwrap_or("");
            if cur_tex.is_empty() || cur_tex.starts_with("_rt_") {
                if let Some(first_eff) = processed_effects.first() {
                    if let Some(texs) = first_eff.get("textures").and_then(|t| t.as_object()) {
                        for slot in &["1", "0", "2"] {
                            if let Some(t_name) = texs.get(*slot).and_then(|v| v.as_str()) {
                                if !t_name.starts_with("_rt_") && t_name != "previous" && !t_name.is_empty() {
                                    obj_data["texture"] = serde_json::json!(t_name);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            obj_data["effects"] = serde_json::Value::Array(processed_effects);
            processed_objects.push(obj_data);
        }

        scene_data["objects"] = serde_json::Value::Array(processed_objects);
    }

    let html_content = EMBEDDED_SCENE_TEMPLATE.replace("__SCENE_DATA_JSON__", &scene_data.to_string());

    let out_html_path = unpacked_dir.join("index.html");
    fs::write(out_html_path, html_content).is_ok()
}

fn resolve_texture_path(ref_name: &str, unpacked_dir: &Path, is_first_bg: bool) -> (String, PathBuf) {
    let clean_ref_path = ref_name.replace('\\', "/");
    if clean_ref_path.starts_with("_rt_") || clean_ref_path == "previous" {
        return (clean_ref_path.clone(), PathBuf::from(&clean_ref_path));
    }
    let clean_ref_path = clean_ref_path
        .trim_end_matches(".png")
        .trim_end_matches(".tex")
        .trim_end_matches(".json");

    if clean_ref_path.starts_with("util/") {
        let util_file = clean_ref_path.strip_prefix("util/").unwrap_or(&clean_ref_path);
        let src_util = PathBuf::from("config/util").join(format!("{util_file}.png"));
        let dst_util = unpacked_dir.join("util").join(format!("{util_file}.png"));
        if src_util.is_file() && !dst_util.exists() {
            let _ = fs::create_dir_all(unpacked_dir.join("util"));
            let _ = fs::copy(&src_util, &dst_util);
        }
        if dst_util.is_file() {
            let rel = path_to_posix_rel(&dst_util, unpacked_dir);
            return (rel, dst_util);
        }
    }

    if clean_ref_path.starts_with("particle/") || clean_ref_path.starts_with("materials/particle/") {
        let part_file = clean_ref_path.trim_start_matches("materials/").strip_prefix("particle/").unwrap_or(&clean_ref_path);
        let dst_part = unpacked_dir.join("particle").join(format!("{part_file}.png"));
        if !dst_part.exists() {
            if let Some(parent) = dst_part.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let src_part = PathBuf::from("config/assets/particle").join(format!("{part_file}.png"));
            let src_part_abs = PathBuf::from("/home/scushi/HyprWpE-Rust/config/assets/particle").join(format!("{part_file}.png"));
            let src_part_hypr = PathBuf::from("/home/scushi/HyprWpE/config/assets/particle").join(format!("{part_file}.png"));
            if src_part.is_file() {
                let _ = fs::copy(&src_part, &dst_part);
            } else if src_part_abs.is_file() {
                let _ = fs::copy(&src_part_abs, &dst_part);
            } else if src_part_hypr.is_file() {
                let _ = fs::copy(&src_part_hypr, &dst_part);
            } else {
                let we_assets = PathBuf::from("/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets");
                for cand in &[
                    we_assets.join("materials/particle").join(format!("{part_file}.tex")),
                    we_assets.join("particle").join(format!("{part_file}.tex")),
                    we_assets.join("materials/particle").join(format!("{part_file}.png")),
                    we_assets.join("particle").join(format!("{part_file}.png")),
                ] {
                    if cand.is_file() {
                        if cand.extension().and_then(|s| s.to_str()) == Some("tex") {
                            let dst_tex = unpacked_dir.join("particle").join(format!("{part_file}.tex"));
                            let _ = fs::copy(cand, &dst_tex);
                            let _ = hyprwpe_unpacker::process_single_tex(&dst_tex);
                            if dst_part.is_file() {
                                break;
                            }
                        } else {
                            let _ = fs::copy(cand, &dst_part);
                            break;
                        }
                    }
                }
            }
        }
        if dst_part.is_file() {
            let rel = path_to_posix_rel(&dst_part, unpacked_dir);
            return (rel, dst_part);
        }
    }

    let stripped_path = clean_ref_path.trim_start_matches("materials/").trim_start_matches("models/");
    let file_basename = Path::new(stripped_path).file_name().and_then(|n| n.to_str()).unwrap_or(stripped_path);

    // If referencing an MDL model, inspect material path embedded inside MDL header
    if clean_ref_path.ends_with(".mdl") || ref_name.ends_with(".mdl") {
        let mdl_path = unpacked_dir.join(&clean_ref_path);
        let mdl_path = if mdl_path.is_file() {
            mdl_path
        } else {
            unpacked_dir.join(ref_name)
        };
        if mdl_path.is_file() {
            if let Ok(bytes) = fs::read(&mdl_path) {
                if let Some(mat_str) = extract_material_from_mdl(&bytes) {
                    let res = resolve_texture_path(&mat_str, unpacked_dir, is_first_bg);
                    if !res.0.is_empty() && !res.0.starts_with("_rt_") && res.0 != "previous" {
                        return res;
                    }
                }
            }
        }
        let stem = Path::new(ref_name).file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !stem.is_empty() {
            let direct_mat = format!("materials/{stem}/{stem}.json");
            let res = resolve_texture_path(&direct_mat, unpacked_dir, is_first_bg);
            if !res.0.is_empty() && !res.0.starts_with("_rt_") && res.0 != "previous" {
                return res;
            }
        }
    }

    // If referencing a model or material JSON, follow references to discover actual texture
    for json_candidate in &[
        unpacked_dir.join(format!("{clean_ref_path}.json")),
        unpacked_dir.join(format!("models/{stripped_path}.json")),
        unpacked_dir.join(format!("materials/{stripped_path}.json")),
    ] {
        if json_candidate.is_file() {
            if let Ok(raw) = fs::read_to_string(json_candidate) {
                if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                    if let Some(mat_ref) = val.get("material").and_then(|v| v.as_str()) {
                        let mat_json_path = unpacked_dir.join(mat_ref);
                        if mat_json_path.is_file() {
                            if let Ok(mat_raw) = fs::read_to_string(&mat_json_path) {
                                if let Ok(mat_val) = serde_json::from_str::<Value>(&mat_raw) {
                                    if let Some(texs) = mat_val.get("passes")
                                        .and_then(|p| p.get(0))
                                        .and_then(|p0| p0.get("textures"))
                                        .and_then(|t| t.as_array())
                                        .or_else(|| mat_val.get("textures").and_then(|t| t.as_array()))
                                    {
                                        for tex_cand in texs {
                                            if let Some(t_str) = tex_cand.as_str() {
                                                if !t_str.starts_with("_rt_") && t_str != "previous" && t_str != ref_name && !t_str.is_empty() {
                                                    let res = resolve_texture_path(t_str, unpacked_dir, is_first_bg);
                                                    if !res.0.is_empty() && !res.0.starts_with("_rt_") && res.0 != "previous" {
                                                        return res;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if let Some(texs) = val.get("passes")
                        .and_then(|p| p.get(0))
                        .and_then(|p0| p0.get("textures"))
                        .and_then(|t| t.as_array())
                        .or_else(|| val.get("textures").and_then(|t| t.as_array()))
                    {
                        for tex_cand in texs {
                            if let Some(t_str) = tex_cand.as_str() {
                                if !t_str.starts_with("_rt_") && t_str != "previous" && t_str != ref_name && !t_str.is_empty() {
                                    let res = resolve_texture_path(t_str, unpacked_dir, is_first_bg);
                                    if !res.0.is_empty() && !res.0.starts_with("_rt_") && res.0 != "previous" {
                                        return res;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    for root_rel in &[
        format!("materials/{stripped_path}"),
        format!("models/{stripped_path}"),
        stripped_path.to_string(),
        file_basename.to_string()
    ] {
        for ext in &["png", "jpg", "jpeg"] {
            let img_file = unpacked_dir.join(format!("{root_rel}.{ext}"));
            if is_valid_clean_png(&img_file) {
                let rel = path_to_posix_rel(&img_file, unpacked_dir);
                return (rel, img_file);
            }
        }

        let png = unpacked_dir.join(format!("{root_rel}.png"));
        let tex = unpacked_dir.join(format!("{root_rel}.tex"));

        if tex.is_file() && (!png.is_file() || !is_valid_clean_png(&png)) {
            let _ = fs::remove_file(&png);
            if hyprwpe_unpacker::process_single_tex(&tex) && is_valid_clean_png(&png) {
                let rel = path_to_posix_rel(&png, unpacked_dir);
                return (rel, png);
            }
        }

        for ext in &["png", "jpg", "jpeg"] {
            let img_file = unpacked_dir.join(format!("{root_rel}.{ext}"));
            if is_valid_clean_png(&img_file) {
                let rel = path_to_posix_rel(&img_file, unpacked_dir);
                return (rel, img_file);
            }
        }
    }

    let mat_dir = unpacked_dir.join("materials");
    if let Ok(entries) = fs::read_dir(&mat_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if (ext == "png" || ext == "jpg" || ext == "jpeg") && is_valid_clean_png(&p) {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if stem == file_basename || stem.replace(' ', "_") == file_basename.replace(' ', "_") {
                        let rel = path_to_posix_rel(&p, unpacked_dir);
                        return (rel, p);
                    }
                }
            }
        }
    }

    let steam_assets = PathBuf::from("/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets");
    if steam_assets.is_dir() {
        for prefix in &["materials/", "presets/", ""] {
            let cand_tex = steam_assets.join(format!("{prefix}{stripped_path}.tex"));
            let cand_png = steam_assets.join(format!("{prefix}{stripped_path}.png"));
            let dst_png = unpacked_dir.join("materials").join(format!("{file_basename}.png"));
            if cand_png.is_file() && !dst_png.exists() {
                if let Some(parent) = dst_png.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::copy(&cand_png, &dst_png);
                let rel = path_to_posix_rel(&dst_png, unpacked_dir);
                return (rel, dst_png);
            }
            if cand_tex.is_file() && !dst_png.exists() {
                if let Some(parent) = dst_png.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let tmp_tex = dst_png.with_extension("tex");
                let _ = fs::copy(&cand_tex, &tmp_tex);
                if hyprwpe_unpacker::process_single_tex(&tmp_tex) && dst_png.is_file() {
                    let _ = fs::remove_file(&tmp_tex);
                    let rel = path_to_posix_rel(&dst_png, unpacked_dir);
                    return (rel, dst_png);
                }
            }
        }
    }

    let safe_ref = file_basename.replace(':', "_").replace('?', "_");
    let dst = unpacked_dir.join("materials").join(format!("{safe_ref}.png"));
    let tex_file = unpacked_dir.join("materials").join(format!("{safe_ref}.tex"));

    if tex_file.exists() {
        let _ = fs::remove_file(&dst);
        if hyprwpe_unpacker::process_single_tex(&tex_file) && is_valid_clean_png(&dst) {
            let rel = path_to_posix_rel(&dst, unpacked_dir);
            return (rel, dst);
        }
    }


    if !dst.exists() {
        if let Some(parent) = dst.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let width = 1u32;
        let height = 1u32;
        let img = if clean_ref_path.contains("solidlayer") || clean_ref_path.contains("white") {
            image::ImageBuffer::from_pixel(4u32, 4u32, image::Rgba([255u8, 255u8, 255u8, 255u8]))
        } else {
            image::ImageBuffer::from_pixel(width, height, image::Rgba([0u8, 0u8, 0u8, 0u8]))
        };
        let _ = img.save(&dst);
    }

    let rel = path_to_posix_rel(&dst, unpacked_dir);
    (rel, dst)
}

fn resolve_object_texture(obj: &Value, unpacked_dir: &Path, is_first_bg: bool) -> (String, PathBuf) {
    if let Some(image_ref) = obj.get("image").and_then(|v| v.as_str()) {
        let res = resolve_texture_path(image_ref, unpacked_dir, is_first_bg);
        if !res.0.is_empty() && !res.0.starts_with("_rt_") && res.0 != "previous" {
            return res;
        }
    }
    if let Some(materials) = obj.get("materials").and_then(|v| v.as_array()) {
        for mat in materials {
            if let Some(mat_str) = mat.as_str() {
                let res = resolve_texture_path(mat_str, unpacked_dir, is_first_bg);
                if !res.0.is_empty() && !res.0.starts_with("_rt_") && res.0 != "previous" {
                    return res;
                }
            }
        }
    }
    if let Some(model_ref) = obj.get("model").and_then(|v| v.as_str()) {
        let res = resolve_texture_path(model_ref, unpacked_dir, is_first_bg);
        if !res.0.is_empty() && !res.0.starts_with("_rt_") && res.0 != "previous" {
            return res;
        }
    }
    ("".to_string(), PathBuf::new())
}

fn path_to_posix_rel(path: &Path, base: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(base) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}
