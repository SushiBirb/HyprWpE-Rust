use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const HLSL_SHIMS: &str = r#"
#define mul(x, y) ((y) * (x))
#define lerp(a, b, t) mix(a, b, t)
#define frac(x) fract(x)
#define saturate(x) clamp(x, 0.0, 1.0)
#define clip(x) if ((x) < 0.0) discard
#define rsqrt(x) inversesqrt(x)
#define CAST2(x) (vec2(x))
#define CAST3(x) (vec3(x))
#define CAST4(x) (vec4(x))
#define CAST3X3(x) (mat3(x))
#define float1 float
#define float2 vec2
#define float3 vec3
#define float4 vec4
#define half float
#define half2 vec2
#define half3 vec3
#define half4 vec4
#define fixed float
#define fixed2 vec2
#define fixed3 vec3
#define fixed4 vec4
#define int2 ivec2
#define int3 ivec3
#define int4 ivec4
#define texSample2D(samp, uv) texture(samp, (uv).xy)
#define texture2D(samp, uv) texture(samp, (uv).xy)
#define textureCube(samp, uv) texture(samp, uv)
#define texSample2DLod(samp, uv, lod) textureLod(samp, (uv).xy, lod)
#define log10(x) (log2(x) * 0.301029995663981)
#define atan2(y, x) atan(y, x)
#define fmod(x, y) ((x) - (y) * trunc((x) / (y)))
#define ddx(x) dFdx(x)
#define ddy(x) dFdy(-(x))
#define texSample2DOffset(samp, uv, off) textureOffset(samp, (uv).xy, off)
#define GLSL 1

float _hlsl_pow(float x, float y) { return pow(x, y); }
vec2 _hlsl_pow(vec2 x, vec2 y) { return pow(x, y); }
vec3 _hlsl_pow(vec3 x, vec3 y) { return pow(x, y); }
vec4 _hlsl_pow(vec4 x, vec4 y) { return pow(x, y); }
vec2 _hlsl_pow(float x, vec2 y) { return pow(vec2(x), y); }
vec3 _hlsl_pow(float x, vec3 y) { return pow(vec3(x), y); }
vec4 _hlsl_pow(float x, vec4 y) { return pow(vec4(x), y); }
vec2 _hlsl_pow(vec2 x, float y) { return pow(x, vec2(y)); }
vec3 _hlsl_pow(vec3 x, float y) { return pow(x, vec3(y)); }
vec4 _hlsl_pow(vec4 x, float y) { return pow(x, vec4(y)); }
#define pow(x, y) _hlsl_pow(x, y)

float _hlsl_max(float a, float b) { return max(a, b); }
int _hlsl_max(int a, int b) { return max(a, b); }
vec2 _hlsl_max(vec2 a, vec2 b) { return max(a, b); }
vec3 _hlsl_max(vec3 a, vec3 b) { return max(a, b); }
vec4 _hlsl_max(vec4 a, vec4 b) { return max(a, b); }
vec2 _hlsl_max(vec2 a, float b) { return max(a, b); }
vec3 _hlsl_max(vec3 a, float b) { return max(a, b); }
vec4 _hlsl_max(vec4 a, float b) { return max(a, b); }
vec2 _hlsl_max(float a, vec2 b) { return max(b, a); }
vec3 _hlsl_max(float a, vec3 b) { return max(b, a); }
vec4 _hlsl_max(float a, vec4 b) { return max(b, a); }
#define max(a, b) _hlsl_max(a, b)

float _hlsl_min(float a, float b) { return min(a, b); }
int _hlsl_min(int a, int b) { return min(a, b); }
vec2 _hlsl_min(vec2 a, vec2 b) { return min(a, b); }
vec3 _hlsl_min(vec3 a, vec3 b) { return min(a, b); }
vec4 _hlsl_min(vec4 a, vec4 b) { return min(a, b); }
vec2 _hlsl_min(vec2 a, float b) { return min(a, b); }
vec3 _hlsl_min(vec3 a, float b) { return min(a, b); }
vec4 _hlsl_min(vec4 a, float b) { return min(a, b); }
vec2 _hlsl_min(float a, vec2 b) { return min(b, a); }
vec3 _hlsl_min(float a, vec3 b) { return min(b, a); }
vec4 _hlsl_min(float a, vec4 b) { return min(b, a); }
#define min(a, b) _hlsl_min(a, b)
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaderUniform {
    pub name: String,
    pub utype: String,
    pub material: Option<String>,
    pub default: Option<Value>,
    pub combo: Option<String>,
    pub slot: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ProcessedProgram {
    pub vert_glsl: String,
    pub frag_glsl: String,
    pub combos: HashMap<String, i32>,
    pub uniforms: Vec<ShaderUniform>,
    pub params: Vec<ShaderUniform>,
}

pub fn resolve_includes(src: &str, scene_dir: Option<&Path>, shaders_dir: Option<&Path>) -> String {
    let re = Regex::new(r#"^\s*#include\s+"([^"]+)"\s*$"#).unwrap();
    let mut out = Vec::new();

    for line in src.lines() {
        if let Some(caps) = re.captures(line) {
            let target = &caps[1];
            if let Some(content) = read_header(target, scene_dir, shaders_dir) {
                out.push(resolve_includes(&content, scene_dir, shaders_dir));
            } else {
                out.push(format!("// missing include: {target}"));
            }
        } else {
            out.push(line.to_string());
        }
    }

    out.join("\n")
}

fn read_header(name: &str, scene_dir: Option<&Path>, shaders_dir: Option<&Path>) -> Option<String> {
    let mut candidates = Vec::new();

    let default_config_shaders = PathBuf::from("config/shaders");

    if let Some(s_dir) = shaders_dir {
        candidates.push(s_dir.join(name));
        candidates.push(s_dir.join(format!("{name}.h")));
        candidates.push(s_dir.join(format!("{}.h", name.replace(".h", ""))));
    } else {
        candidates.push(default_config_shaders.join(name));
        candidates.push(default_config_shaders.join(format!("{name}.h")));
        candidates.push(default_config_shaders.join(format!("{}.h", name.replace(".h", ""))));
    }

    if let Some(sc_dir) = scene_dir {
        candidates.push(sc_dir.join(name));
        candidates.push(sc_dir.join("shaders").join(name));
        candidates.push(sc_dir.join(format!("{name}.h")));
        candidates.push(sc_dir.join("shaders").join(format!("{name}.h")));
    }

    for candidate in candidates {
        if candidate.is_file() {
            if let Ok(content) = fs::read_to_string(candidate) {
                return Some(content);
            }
        }
    }

    None
}

pub fn parse_combos(src: &str) -> HashMap<String, i32> {
    let re = Regex::new(r#"//\s*\[COMBO\]\s*(\{.*\})"#).unwrap();
    let mut combos = HashMap::new();

    for caps in re.captures_iter(src) {
        if let Ok(meta) = serde_json::from_str::<Value>(&caps[1]) {
            if let Some(name) = meta.get("combo").and_then(|v| v.as_str()) {
                if let Some(default_val) = meta.get("default") {
                    let val = default_val.as_i64().unwrap_or(0) as i32;
                    combos.insert(name.to_string(), val);
                }
            }
        }
    }

    combos
}

pub fn parse_parameters(src: &str) -> Vec<ShaderUniform> {
    let re = Regex::new(r#"^\s*uniform\s+(?:(?:lowp|mediump|highp)\s+)?([\w]+)\s+([a-zA-Z_]\w*)(?:\[\s*\d*\s*\])?\s*;\s*(?://(.*))?$"#).unwrap();
    let mut params = Vec::new();

    for line in src.lines() {
        if let Some(caps) = re.captures(line) {
            let utype = caps[1].to_string();
            let uname = caps[2].to_string();
            let comment = caps.get(3).map(|m| m.as_str());

            let mut material = None;
            let mut default = None;
            let mut combo = None;

            if let Some(c) = comment {
                if let Some(start) = c.find('{') {
                    if let Some(end) = c.rfind('}') {
                        if let Ok(meta) = serde_json::from_str::<Value>(&c[start..=end]) {
                            material = meta.get("material").and_then(|v| v.as_str()).map(|s| s.to_string());
                            default = meta.get("default").cloned();
                            combo = meta.get("combo").and_then(|v| v.as_str()).map(|s| s.to_string());
                        }
                    }
                }
            }

            let mut slot = None;
            if uname.starts_with("g_Texture") {
                if let Ok(s) = uname["g_Texture".len()..].parse::<usize>() {
                    slot = Some(s);
                }
            }

            params.push(ShaderUniform {
                name: uname,
                utype,
                material,
                default,
                combo,
                slot,
            });
        }
    }

    params
}

pub fn rename_es_reserved(src: &str) -> String {
    let reserved = ["sample", "image", "input", "output", "filter", "partition", "active", "subset", "superp"];
    let mut s = src.to_string();
    for kw in reserved {
        let pattern = format!(r"\b{kw}\b");
        if let Ok(re) = Regex::new(&pattern) {
            s = re.replace_all(&s, format!("{kw}_")).to_string();
        }
    }

    // Rename RGB parameter in rgb2hsv to avoid colliding with combo `#define RGB 0/1`
    let re_rgb2hsv = Regex::new(r"vec3\s+rgb2hsv\s*\(\s*vec3\s+RGB\s*\)").unwrap();
    if re_rgb2hsv.is_match(&s) {
        s = re_rgb2hsv.replace_all(&s, "vec3 rgb2hsv(vec3 rgb_in)").to_string();
        s = s.replace("(RGB.g < RGB.b)", "(rgb_in.g < rgb_in.b)");
        s = s.replace("vec4(RGB.bg, -1.0, 2.0/3.0)", "vec4(rgb_in.bg, -1.0, 2.0/3.0)");
        s = s.replace("vec4(RGB.gb, 0.0, -1.0/3.0)", "vec4(rgb_in.gb, 0.0, -1.0/3.0)");
        s = s.replace("(RGB.r < P.x)", "(rgb_in.r < P.x)");
        s = s.replace("vec4(P.xyw, RGB.r)", "vec4(P.xyw, rgb_in.r)");
        s = s.replace("vec4(RGB.r, P.yzx)", "vec4(rgb_in.r, P.yzx)");
    }

    s
}

fn coerce_integers_to_floats(src: &str) -> String {
    let mut out_lines = Vec::new();

    let re_assign = Regex::new(r"\b(float|vec2|vec3|vec4)\s+([a-zA-Z_]\w*)\s*=\s*(\d+)\s*;").unwrap();
    let re_digits = Regex::new(r"\b\d+\b").unwrap();
    let re_vec2_vec4 = Regex::new(r"\bvec2\s+([a-zA-Z_]\w*)\s*=\s*(v_TexCoord|v_TexCoordRipple|a_Position|v_Position)\s*;").unwrap();
    let re_int_kw = Regex::new(r"\bint\b|\buint\b|\bivec[234]\b|\buvec[234]\b|\bfor\s*\(").unwrap();

    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || re_int_kw.is_match(trimmed) {
            out_lines.push(line.to_string());
            continue;
        }

        let line_vec = re_vec2_vec4.replace_all(line, "vec2 $1 = $2.xy;").to_string();
        let line_assign = re_assign.replace_all(&line_vec, "$1 $2 = $3.0;").to_string();
        let mut new_line = String::with_capacity(line_assign.len());
        let mut last_idx = 0;

        for m in re_digits.find_iter(&line_assign) {
            let start = m.start();
            let end = m.end();

            let char_before = if start > 0 { line_assign.as_bytes()[start - 1] as char } else { ' ' };
            let char_after = if end < line_assign.len() { line_assign.as_bytes()[end] as char } else { ' ' };

            if char_before != '.' && char_after != '.' {
                let rest_after = &line_assign[end..];
                let rest_before = &line_assign[..start];

                let is_scientific = rest_before.ends_with("e-")
                    || rest_before.ends_with("e+")
                    || rest_before.ends_with("e")
                    || rest_before.ends_with("E-")
                    || rest_before.ends_with("E+")
                    || rest_before.ends_with("E");

                if is_scientific {
                    new_line.push_str(&line_assign[last_idx..end]);
                    last_idx = end;
                    continue;
                }

                let is_followed_by_op = rest_after.trim_start().starts_with('*')
                    || rest_after.trim_start().starts_with('/')
                    || rest_after.trim_start().starts_with('+')
                    || rest_after.trim_start().starts_with('-')
                    || rest_after.trim_start().starts_with(';')
                    || rest_after.trim_start().starts_with(')');

                let is_preceded_by_op = rest_before.trim_end().ends_with('*')
                    || rest_before.trim_end().ends_with('/')
                    || rest_before.trim_end().ends_with('+')
                    || rest_before.trim_end().ends_with('-')
                    || rest_before.trim_end().ends_with('=')
                    || rest_before.trim_end().ends_with('(');

                if is_followed_by_op || is_preceded_by_op {
                    new_line.push_str(&line_assign[last_idx..start]);
                    new_line.push_str(&line_assign[start..end]);
                    new_line.push_str(".0");
                    last_idx = end;
                    continue;
                }
            }

            new_line.push_str(&line_assign[last_idx..end]);
            last_idx = end;
        }

        new_line.push_str(&line_assign[last_idx..]);
        out_lines.push(new_line);
    }

    out_lines.join("\n")
}

pub fn convert_to_es300(src: &str, stage: &str, combos: &HashMap<String, i32>) -> String {
    let mut s = src.to_string();

    // Strip HLSL 'f' literal suffix
    let re_float_f = Regex::new(r"(\d+\.\d+)f\b").unwrap();
    s = re_float_f.replace_all(&s, "$1").to_string();

    let re_int_f = Regex::new(r"\b(\d+)f\b").unwrap();
    s = re_int_f.replace_all(&s, "$1.0").to_string();

    // Coerce integer arguments in math functions (max, min, clamp, mix, smoothstep, lerp)
    let re_fn1 = Regex::new(r"\b(max|min|clamp|mix|smoothstep|lerp)\s*\(\s*(\d+)\s*,").unwrap();
    s = re_fn1.replace_all(&s, "$1($2.0,").to_string();

    let re_fn2 = Regex::new(r"\b(max|min|clamp|mix|smoothstep|lerp)\s*\(([^,]+),\s*(\d+)\s*([\),])").unwrap();
    s = re_fn2.replace_all(&s, "$1($2, $3.0$4").to_string();

    // GLSL defines max(genType, float) but not max(float, genType); swap if first arg is literal float/int
    let re_max_min_swap = Regex::new(r"\b(max|min)\s*\(\s*(\d+(?:\.\d+)?)\s*,\s*([a-zA-Z_]\w*(?:\.[a-zA-Z_]\w*)?)\s*\)").unwrap();
    s = re_max_min_swap.replace_all(&s, "$1($3, $2)").to_string();

    // Convert integer binary arithmetic ops to floats outside preprocessor & int loop lines
    s = coerce_integers_to_floats(&s);

    // Coerce float(ident + int) or float(ident + float) into (float(ident) + int.0) to protect loop ints
    let re_float_int_op = Regex::new(r"\bfloat\s*\(\s*([a-zA-Z_]\w*)\s*([\+\-])\s*(\d+)(?:\.0)?\s*\)").unwrap();
    s = re_float_int_op.replace_all(&s, "(float($1) $2 $3.0)").to_string();

    let re_kernel_div = Regex::new(r"\bkernel\s*\+\s*kernel\s*\+\s*2\.0\b").unwrap();
    s = re_kernel_div.replace_all(&s, "float(kernel + kernel) + 2.0").to_string();

    // Coerce modulo on uint / float expressions for GLSL ES 3.00
    let re_uint_mod = Regex::new(r"\buint\s+([a-zA-Z_]\w*)\s*=\s*([^;%]+)%\s*([^;]+);").unwrap();
    s = re_uint_mod.replace_all(&s, |caps: &regex::Captures| {
        let var = &caps[1];
        let lhs = caps[2].trim();
        let rhs = caps[3].trim();
        format!("uint {var} = uint({lhs}) % uint({rhs});")
    }).to_string();

    // Stage attribute/varying rewrites
    if stage == "vertex" {
        let re_attr = Regex::new(r"\battribute\b").unwrap();
        s = re_attr.replace_all(&s, "in").to_string();

        let re_var = Regex::new(r"\bvarying\b").unwrap();
        s = re_var.replace_all(&s, "out").to_string();

        // Normalize foliage sway displacement from pixel units to NDC units to prevent giant floating blade
        if s.contains("g_Strength * 100.0 * weight") {
            s = s.replace("g_Strength * 100.0 * weight", "g_Strength * 0.1 * weight");
        }

        // Align perspective vertical UV coordinates with WebGL texture space
        if s.contains("step(0.5, a_TexCoord.y)") && s.contains("mix(g_Top, g_Bottom") {
            s = s.replace("mix(g_Top, g_Bottom, step(0.5, a_TexCoord.y))", "mix(g_Bottom, g_Top, step(0.5, a_TexCoord.y))");
            s = s.replace("vec2 p3 = vec2(g_Top, g_Left);", "vec2 p3 = vec2(g_Bottom, g_Left);");
            s = s.replace("vec2 p2 = vec2(1 - g_Top, g_Right);", "vec2 p2 = vec2(1 - g_Bottom, g_Right);");
            s = s.replace("vec2 p1 = vec2(1 - g_Bottom, 1 - g_Right);", "vec2 p1 = vec2(1 - g_Top, 1 - g_Right);");
            s = s.replace("vec2 p0 = vec2(g_Bottom, 1 - g_Left);", "vec2 p0 = vec2(g_Top, 1 - g_Left);");
        }
    } else {
        let re_var = Regex::new(r"\bvarying\b").unwrap();
        s = re_var.replace_all(&s, "in").to_string();
        s = s.replace("gl_FragColor", "out_FragColor");
    }

    // Promote v_TexCoord to vec4 in fragment shader if .z, .w, or .zw is accessed
    if stage == "fragment" {
        if s.contains("v_TexCoord.z") || s.contains("v_TexCoord.w") || s.contains("v_TexCoord.zw") {
            let re_tc = Regex::new(r"\b(varying|in)\s+vec2\s+v_TexCoord\s*;").unwrap();
            s = re_tc.replace_all(&s, "in vec4 v_TexCoord;").to_string();
        }

        // Coerce 3D perspective mask vector truncation in 2D norm expressions
        let re_vec2_mask = Regex::new(r"vec2\s+(\w+)\s*=\s*step\s*\(([^,]+),\s*abs\s*\(\s*v_MaskPersp\s*-\s*([^)]+)\)\s*\);").unwrap();
        s = re_vec2_mask.replace_all(&s, "vec2 $1 = step($2, abs(v_MaskPersp.xy - $3));").to_string();
    }

    // Coerce expressions with known combo macros: `COMBO * 0.4`, `0.4 * COMBO`, `x * COMBO`, `COMBO * x`
    for combo_name in combos.keys() {
        if combo_name == "GLSL" || combo_name == "HLSL_SM30" {
            continue;
        }
        let p1 = format!(r"\b{}\s*([\*\+\-\/])\s*(\d+\.\d+)", regex::escape(combo_name));
        if let Ok(re1) = Regex::new(&p1) {
            s = re1.replace_all(&s, format!("float({combo_name}) $1 $2")).to_string();
        }
        let p2 = format!(r"(\d+\.\d+)\s*([\*\+\-\/])\s*\b{}", regex::escape(combo_name));
        if let Ok(re2) = Regex::new(&p2) {
            s = re2.replace_all(&s, format!("$1 $2 float({combo_name})")).to_string();
        }

        // On float lines (not preprocessor and not uint lines), coerce `var * COMBO` into `var * float(COMBO)`
        let p_float_combo = format!(r"([*+/-])\s*\b{}\b(\s*%)?", regex::escape(combo_name));
        if let Ok(re_fc) = Regex::new(&p_float_combo) {
            let mut new_lines = Vec::new();
            for line in s.lines() {
                let trimmed = line.trim_start();
                if !trimmed.starts_with('#') && !trimmed.contains("uint ") && (trimmed.contains("float ") || trimmed.contains("vec2 ") || trimmed.contains("vec3 ") || trimmed.contains("vec4 ") || trimmed.contains("frequency") || trimmed.contains("barHeight")) {
                    let replaced = re_fc.replace_all(line, |caps: &regex::Captures| {
                        if caps.get(2).is_some() {
                            caps[0].to_string()
                        } else {
                            format!("{} float({combo_name})", &caps[1])
                        }
                    });
                    new_lines.push(replaced.to_string());
                } else {
                    new_lines.push(line.to_string());
                }
            }
            s = new_lines.join("\n");
        }
    }

    // Coerce integer literals with arithmetic operators in uint contexts (e.g. barFreq1 + 1 -> barFreq1 + 1u)
    let re_uint_lit = Regex::new(r"(\b[a-zA-Z_]\w*\s*[\+\-\*\/]\s*)(\d+)([a-zA-Z_\.]?)").unwrap();
    let mut uint_lines = Vec::new();
    for line in s.lines() {
        if line.contains("uint ") || line.contains("uint(") {
            let replaced = re_uint_lit.replace_all(line, |caps: &regex::Captures| {
                let suffix = &caps[3];
                if suffix.is_empty() {
                    format!("{}{}u", &caps[1], &caps[2])
                } else {
                    caps[0].to_string()
                }
            });
            uint_lines.push(replaced.to_string());
        } else {
            uint_lines.push(line.to_string());
        }
    }
    s = uint_lines.join("\n");

    // Coerce for loop bounds when using float uniforms (ensuring <= and >= take precedence over < and >)
    let re_for_loop = Regex::new(r"for\s*\(\s*int\s+([a-zA-Z_]\w*)\s*=\s*([^;]+);\s*([a-zA-Z_]\w*)\s*(<=|>=|<|>)\s*([^;]+);\s*([^)]+)\)").unwrap();
    s = re_for_loop.replace_all(&s, |caps: &regex::Captures| {
        let v1 = caps[1].trim();
        let init = caps[2].trim();
        let v2 = caps[3].trim();
        let op = &caps[4];
        let limit = caps[5].trim();
        let step = caps[6].trim();

        let new_init = if init.chars().all(|c| c.is_ascii_digit()) || init.starts_with("int(") {
            init.to_string()
        } else {
            format!("int({init})")
        };

        let new_limit = if limit.chars().all(|c| c.is_ascii_digit()) || limit.starts_with("int(") {
            limit.to_string()
        } else {
            format!("int({limit})")
        };

        format!("for (int {v1} = {new_init}; {v2} {op} {new_limit}; {step})")
    }).to_string();

    // Inside float(var + int)
    let re_float_add = Regex::new(r"\bfloat\s*\(\s*([a-zA-Z_]\w*)\s*\+\s*(\d+)\s*\)").unwrap();
    s = re_float_add.replace_all(&s, "(float($1) + $2.0)").to_string();

    rename_es_reserved(&s)
}

fn strip_metadata_comments(src: &str) -> String {
    let mut out = Vec::new();
    let re_meta = Regex::new(r"(uniform\s+(?:(?:lowp|mediump|highp)\s+)?[\w]+\s+[a-zA-Z_]\w*(?:\[\s*\d*\s*\])?\s*;)\s*//\s*\{.*\}").unwrap();

    for line in src.lines() {
        if line.trim_start().starts_with("// [COMBO]") {
            continue;
        }
        let cleaned = re_meta.replace(line, "$1");
        out.push(cleaned.to_string());
    }

    out.join("\n")
}

fn hoist_uniforms(src: &str) -> (Vec<String>, String) {
    let mut uniforms = Vec::new();
    let mut clean_lines = Vec::new();
    let mut seen = HashSet::new();
    let re_uniform = Regex::new(r"^\s*uniform\s+(?:(?:lowp|mediump|highp)\s+)?([a-zA-Z0-9_]+)\s+([a-zA-Z0-9_]+(?:\s*\[\s*\d*\s*\])?)\s*;(?:\s*//.*)?$").unwrap();

    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(caps) = re_uniform.captures(trimmed) {
            let decl = format!("uniform {} {};", &caps[1], &caps[2]);
            if seen.insert(decl.clone()) {
                uniforms.push(decl);
            }
            clean_lines.push(format!("// hoisted: {trimmed}"));
        } else {
            clean_lines.push(line.to_string());
        }
    }

    (uniforms, clean_lines.join("\n"))
}

fn build_source(body: &str, stage: &str, combos: &HashMap<String, i32>) -> String {
    let body_prepped = strip_metadata_comments(body);
    let (hoisted_uniforms, body_unif) = hoist_uniforms(&body_prepped);
    let body_es = convert_to_es300(&body_unif, stage, combos);

    let re_def = Regex::new(r"^\s*#\s*define\s+([A-Za-z0-9_]+)").unwrap();
    let mut undef_body_lines = Vec::new();
    for line in body_es.lines() {
        if let Some(caps) = re_def.captures(line) {
            let def_name = &caps[1];
            undef_body_lines.push(format!("#undef {def_name}"));
        }
        undef_body_lines.push(line.to_string());
    }
    let body_es_undef = undef_body_lines.join("\n");

    let mut defines: Vec<String> = combos.iter()
        .map(|(k, v)| format!("#define {k} {v}"))
        .collect();
    defines.sort();

    let mut header_lines = vec![
        "#version 300 es".to_string(),
        "precision highp float;".to_string(),
        "precision highp int;".to_string(),
    ];

    if stage == "fragment" {
        header_lines.push("out vec4 out_FragColor;".to_string());
    }

    format!(
        "{}\n{}\n{}\n{}\n{}",
        header_lines.join("\n"),
        HLSL_SHIMS,
        defines.join("\n"),
        hoisted_uniforms.join("\n"),
        body_es_undef
    )
}

pub fn preprocess_program(
    vert_path: &Path,
    frag_path: &Path,
    bound_textures: &HashMap<usize, String>,
    extra_combos: Option<&HashMap<String, i32>>,
    shaders_dir: Option<&Path>,
) -> Result<ProcessedProgram, std::io::Error> {
    let vert_raw = fs::read_to_string(vert_path)?;
    let frag_raw = fs::read_to_string(frag_path)?;

    let scene_dir = vert_path.parent().and_then(|p| p.parent());

    let vert_src = resolve_includes(&vert_raw, scene_dir, shaders_dir);
    let frag_src = resolve_includes(&frag_raw, scene_dir, shaders_dir);

    let mut combos = HashMap::new();
    combos.extend(parse_combos(&vert_raw));
    combos.extend(parse_combos(&frag_raw));

    let mut params = parse_parameters(&vert_src);
    params.extend(parse_parameters(&frag_src));

    let bound_slots: HashSet<usize> = bound_textures.keys().copied().collect();
    for p in &params {
        if p.utype == "sampler2D" {
            if let (Some(combo), Some(slot)) = (&p.combo, p.slot) {
                if bound_slots.contains(&slot) {
                    combos.insert(combo.clone(), 1);
                }
            }
        }
    }

    if let Some(extras) = extra_combos {
        for (k, v) in extras {
            combos.insert(k.clone(), *v);
        }
    }
    combos.entry("HLSL_SM30".to_string()).or_insert(0);

    let re_cond_branch = Regex::new(r"#if\s+([^\r\n]*\.[^\r\n]*)\r?\n(?:#undef\s+(\w+)\r?\n)?#define\s+(\w+)\s+([^\r\n]+)\r?\n#else\r?\n(?:#undef\s+\w+\r?\n)?#define\s+\w+\s+([^\r\n]+)\r?\n#endif").unwrap();
    let vert_src = re_cond_branch.replace_all(&vert_src, "#define $3 (($1) ? ($4) : ($5))").to_string();
    let frag_src = re_cond_branch.replace_all(&frag_src, "#define $3 (($1) ? ($4) : ($5))").to_string();

    let re_macro = Regex::new(r"\b[A-Z_][A-Z0-9_]*\b").unwrap();
    let re_cond = Regex::new(r"#\s*(?:if|elif)\s+(.*)").unwrap();
    let re_internal_define = Regex::new(r"^\s*#\s*define\s+([A-Za-z0-9_]+)").unwrap();

    let mut internal_defines = HashSet::new();
    for line in vert_src.lines().chain(frag_src.lines()) {
        if let Some(caps) = re_internal_define.captures(line) {
            internal_defines.insert(caps[1].to_string());
        }
    }

    for line in vert_src.lines().chain(frag_src.lines()) {
        if let Some(caps) = re_cond.captures(line) {
            let cond_str = &caps[1];
            for m in re_macro.find_iter(cond_str) {
                let name = m.as_str();
                let end = m.end();
                if end < cond_str.len() && cond_str.as_bytes()[end] == b'.' {
                    continue;
                }
                if !matches!(name, "GLSL" | "HLSL_SM30" | "0" | "1" | "2" | "3" | "defined")
                    && !internal_defines.contains(name)
                    && !params.iter().any(|p| p.name == name)
                {
                    combos.entry(name.to_string()).or_insert(0);
                }
            }
        }
    }

    let mut vert_glsl = build_source(&vert_src, "vertex", &combos);
    let mut frag_glsl = build_source(&frag_src, "fragment", &combos);

    // Fix HLSL-style float = texSample2D(...) missing .r swizzle
    let re_float_tex = Regex::new(r"float\s+(\w+)\s*=\s*((?:texSample2D|texture2D)\([^\n;]+?\))\s*;").unwrap();
    let fix_float_tex = |src: &str| -> String {
        re_float_tex.replace_all(src, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let expr = &caps[2];
            if expr.ends_with(".r") || expr.ends_with(".g") || expr.ends_with(".b") || expr.ends_with(".a")
                || expr.ends_with(".x") || expr.ends_with(".y") || expr.ends_with(".z") || expr.ends_with(".w") {
                format!("float {var_name} = {expr};")
            } else {
                format!("float {var_name} = {expr}.r;")
            }
        }).to_string()
    };
    vert_glsl = fix_float_tex(&vert_glsl);
    frag_glsl = fix_float_tex(&frag_glsl);

    // Fix HLSL-style vec3 = texSample2D(...) missing .rgb swizzle
    let re_vec3_tex = Regex::new(r"vec3\s+(\w+)\s*=\s*((?:texSample2D|texture2D|texture)\([^\n;]+?\))\s*;").unwrap();
    let fix_vec3_tex = |src: &str| -> String {
        re_vec3_tex.replace_all(src, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let expr = &caps[2];
            if expr.ends_with(".rgb") || expr.ends_with(".xyz") {
                format!("vec3 {var_name} = {expr};")
            } else {
                format!("vec3 {var_name} = {expr}.rgb;")
            }
        }).to_string()
    };
    vert_glsl = fix_vec3_tex(&vert_glsl);
    frag_glsl = fix_vec3_tex(&frag_glsl);

    // Fix HLSL-style vec2 = texSample2D(...) missing .rg swizzle
    let re_vec2_tex = Regex::new(r"vec2\s+(\w+)\s*=\s*((?:texSample2D|texture2D|texture)\([^\n;]+?\))\s*;").unwrap();
    let fix_vec2_tex = |src: &str| -> String {
        re_vec2_tex.replace_all(src, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let expr = &caps[2];
            if expr.ends_with(".rg") || expr.ends_with(".xy") {
                format!("vec2 {var_name} = {expr};")
            } else {
                format!("vec2 {var_name} = {expr}.rg;")
            }
        }).to_string()
    };
    vert_glsl = fix_vec2_tex(&vert_glsl);
    frag_glsl = fix_vec2_tex(&frag_glsl);

    // Fix HLSL-style target.rgb = mix(target, ...) where target is vec4
    let re_mix_lhs_arg1 = Regex::new(r"\b(\w+)\.rgb\s*=\s*mix\(\s*(\w+)\s*,").unwrap();
    let fix_mix_arg1 = |src: &str| -> String {
        re_mix_lhs_arg1.replace_all(src, |caps: &regex::Captures| {
            let lhs = &caps[1];
            let arg1 = &caps[2];
            if lhs == arg1 {
                format!("{lhs}.rgb = mix({lhs}.rgb,")
            } else {
                caps[0].to_string()
            }
        }).to_string()
    };
    frag_glsl = fix_mix_arg1(&frag_glsl);
    vert_glsl = fix_mix_arg1(&vert_glsl);

    let re_mix_lhs_arg2 = Regex::new(r"\b(\w+)\.rgb\s*=\s*mix\(([^,]+),\s*(\w+)\s*,").unwrap();
    let fix_mix_arg2 = |src: &str| -> String {
        re_mix_lhs_arg2.replace_all(src, |caps: &regex::Captures| {
            let lhs = &caps[1];
            let arg1 = &caps[2];
            let arg2 = &caps[3];
            if lhs == arg2 {
                format!("{lhs}.rgb = mix({arg1}, {lhs}.rgb,")
            } else {
                caps[0].to_string()
            }
        }).to_string()
    };
    frag_glsl = fix_mix_arg2(&frag_glsl);
    vert_glsl = fix_mix_arg2(&vert_glsl);

    // Fix HLSL array indexing with float expressions (e.g. g_AudioSpectrum32Left[g_Chromatic])
    let re_audio_index = Regex::new(r"\b(g_AudioSpectrum\w*|u_AudioSpectrum\w*)\[([^\]]+)\]").unwrap();
    let fix_audio_index = |src: &str| -> String {
        re_audio_index.replace_all(src, |caps: &regex::Captures| {
            let arr_name = &caps[1];
            let idx_expr = caps[2].trim();
            if idx_expr.chars().all(|c| c.is_ascii_digit()) || idx_expr.starts_with("int(") {
                format!("{arr_name}[{idx_expr}]")
            } else {
                format!("{arr_name}[int({idx_expr})]")
            }
        }).to_string()
    };
    vert_glsl = fix_audio_index(&vert_glsl);
    frag_glsl = fix_audio_index(&frag_glsl);

    // Fix HLSL float var = g_PointerPosition * ... (needs vec2)
    let re_float_pointer = Regex::new(r"\bfloat\s+([a-zA-Z_]\w*)\s*=\s*(g_PointerPosition(?:\.xy)?\b[^;]*);").unwrap();
    frag_glsl = re_float_pointer.replace_all(&frag_glsl, "vec2 $1 = $2;").to_string();
    vert_glsl = re_float_pointer.replace_all(&vert_glsl, "vec2 $1 = $2;").to_string();

    // Fix HLSL vec3 var = vec4(...) dimension mismatch
    let re_vec3_vec4 = Regex::new(r"\bvec3\s+([a-zA-Z_]\w*)\s*=\s*(vec4\([^\n;]+\))\s*;").unwrap();
    frag_glsl = re_vec3_vec4.replace_all(&frag_glsl, "vec3 $1 = ($2).rgb;").to_string();
    vert_glsl = re_vec3_vec4.replace_all(&vert_glsl, "vec3 $1 = ($2).rgb;").to_string();

    // Fix integer combo ternary condition (e.g. INVERT ? a : b -> (INVERT != 0) ? a : b)
    for k in combos.keys() {
        if let Ok(re_ternary) = Regex::new(&format!(r"\b{}\s*\?", regex::escape(k))) {
            let repl = format!("({k} != 0) ?");
            vert_glsl = re_ternary.replace_all(&vert_glsl, repl.as_str()).to_string();
            frag_glsl = re_ternary.replace_all(&frag_glsl, repl.as_str()).to_string();
        }
    }

    // Fix HLSL select(vec4, vec4) calling select(vec3, vec3)
    if frag_glsl.contains("select(baseAlbedo, albedo)") {
        frag_glsl = frag_glsl.replace("select(baseAlbedo, albedo)", "select(baseAlbedo.rgb, albedo.rgb)");
    }

    // Fix HLSL vector operation with mismatched dimensions (e.g. v_TexCoord - CAST2(u_offset) where v_TexCoord is vec3)
    let re_vtexcoord_cast2 = Regex::new(r"\bv_TexCoord\s*([+\-*/])\s*CAST2\(").unwrap();
    frag_glsl = re_vtexcoord_cast2.replace_all(&frag_glsl, "v_TexCoord.xy $1 CAST2(").to_string();
    vert_glsl = re_vtexcoord_cast2.replace_all(&vert_glsl, "v_TexCoord.xy $1 CAST2(").to_string();

    let re_cast2_vtexcoord = Regex::new(r"CAST2\(([^)]+)\)\s*([+\-*/])\s*v_TexCoord\b").unwrap();
    frag_glsl = re_cast2_vtexcoord.replace_all(&frag_glsl, "CAST2($1) $2 v_TexCoord.xy").to_string();
    vert_glsl = re_cast2_vtexcoord.replace_all(&vert_glsl, "CAST2($1) $2 v_TexCoord.xy").to_string();

    // Synchronize v_TexCoord varying type across vertex and fragment shaders
    if frag_glsl.contains("in vec4 v_TexCoord;") && vert_glsl.contains("out vec2 v_TexCoord;") {
        vert_glsl = vert_glsl.replace("out vec2 v_TexCoord;", "out vec4 v_TexCoord;");
        let re_assign = Regex::new(r"\bv_TexCoord\s*=\s*a_TexCoord\s*;").unwrap();
        vert_glsl = re_assign.replace_all(&vert_glsl, "v_TexCoord = vec4(a_TexCoord, 0.0, 0.0);").to_string();
    } else if vert_glsl.contains("out vec4 v_TexCoord;") && frag_glsl.contains("in vec2 v_TexCoord;") {
        frag_glsl = frag_glsl.replace("in vec2 v_TexCoord;", "in vec4 v_TexCoord;");
    }

    // Synchronize uniform types across vertex and fragment shaders (e.g. g_CloudSpeeds vec2 vs vec4)
    let re_unif_decl = Regex::new(r"(?m)^\s*uniform\s+(vec[234]|float|int|ivec[234])\s+([A-Za-z0-9_]+)\s*;").unwrap();
    let mut vert_unifs = HashMap::new();
    for cap in re_unif_decl.captures_iter(&vert_glsl) {
        vert_unifs.insert(cap[2].to_string(), cap[1].to_string());
    }
    let mut replacements = Vec::new();
    for cap in re_unif_decl.captures_iter(&frag_glsl) {
        let name = cap[2].to_string();
        let frag_type = cap[1].to_string();
        if let Some(vert_type) = vert_unifs.get(&name) {
            if vert_type != &frag_type {
                let type_rank = |t: &str| match t {
                    "float" | "int" => 1,
                    "vec2" | "ivec2" => 2,
                    "vec3" | "ivec3" => 3,
                    "vec4" | "ivec4" => 4,
                    _ => 0,
                };
                let (target_type, lower_type) = if type_rank(&frag_type) >= type_rank(vert_type) {
                    (&frag_type, vert_type)
                } else {
                    (vert_type, &frag_type)
                };
                let old_decl = format!("uniform {lower_type} {name};");
                let new_decl = format!("uniform {target_type} {name};");
                replacements.push((old_decl, new_decl));
            }
        }
    }
    for (old_decl, new_decl) in replacements {
        vert_glsl = vert_glsl.replace(&old_decl, &new_decl);
        frag_glsl = frag_glsl.replace(&old_decl, &new_decl);
    }

    let mut seen = HashSet::new();
    let mut deduped_uniforms = Vec::new();

    for p in &params {
        if p.utype != "sampler2D" && !p.name.starts_with("g_Texture") {
            if !matches!(
                p.name.as_str(),
                "g_Time"
                    | "g_Daytime"
                    | "g_ModelViewProjectionMatrix"
                    | "g_EffectModelViewProjectionMatrix"
                    | "g_PointerPosition"
                    | "g_PointerPositionLast"
            ) {
                if seen.insert(p.name.clone()) {
                    deduped_uniforms.push(p.clone());
                }
            }
        }
    }
    vert_glsl = balance_preprocessor_directives(&vert_glsl);
    frag_glsl = balance_preprocessor_directives(&frag_glsl);

    Ok(ProcessedProgram {
        vert_glsl,
        frag_glsl,
        combos,
        uniforms: deduped_uniforms,
        params,
    })
}

fn balance_preprocessor_directives(src: &str) -> String {
    let mut depth: i32 = 0;
    let mut out_lines = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#if") || trimmed.starts_with("#ifdef") || trimmed.starts_with("#ifndef") {
            depth += 1;
            out_lines.push(line.to_string());
        } else if trimmed.starts_with("#endif") {
            if depth > 0 {
                depth -= 1;
                out_lines.push(line.to_string());
            } else {
                out_lines.push(format!("// extra #endif dropped: {line}"));
            }
        } else {
            out_lines.push(line.to_string());
        }
    }

    while depth > 0 {
        out_lines.push("#endif // auto-closed".to_string());
        depth -= 1;
    }

    out_lines.join("\n")
}

pub fn coerce_value(raw: &Value, utype: &str) -> Value {
    match raw {
        Value::String(s) => {
            let parts: Vec<&str> = s.split_whitespace().collect();
            let mut nums = Vec::new();
            for p in parts {
                if let Ok(f) = p.parse::<f64>() {
                    nums.push(f);
                }
            }
            if !nums.is_empty() {
                if utype.starts_with("vec") || utype.starts_with("float") {
                    if nums.len() == 1 {
                        return serde_json::json!(nums[0]);
                    } else {
                        return serde_json::json!(nums);
                    }
                }
                return serde_json::json!(nums);
            }
            raw.clone()
        }
        _ => raw.clone(),
    }
}
