"""
GLSL preprocessor: transpiles Wallpaper Engine's HLSL-flavoured GLSL effect
shaders (the .frag/.vert files shipped inside wallpaper packages) into WebGL2
GLSL ES 300, replicating what linux-wallpaperengine's C++ ShaderUnit does:

  * resolves #include "name" against the bundled global headers (common.h,
    common_blending.h, common_fragment.h) shipped in config/shaders
  * applies the HLSL->GLSL compatibility shims (mul, saturate, texSample2D,
    CAST3, frac, lerp, ...) that WE/lwe prepend at compile time
  * resolves [COMBO] metadata comments to #define values, including
    texture-driven combos (e.g. MASK) that activate when a sampler slot is bound
  * parses uniform metadata comments to map material constants to shader uniforms
  * rewrites attribute/varying/gl_FragColor to GLSL ES 300 syntax

The output is plain WebGL2 source plus a description of the uniforms/textures
the runtime must bind. This removes linux-wallpaperengine as a runtime dependency
while keeping the rendered maths identical to the original engine.
"""
import os
import sys
import re
import json

SHADERS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'shaders')

# The exact compatibility header lwe prepends (ShaderUnit.cpp SHADER_HEADER),
# trimmed to the subset relevant for GLSL ES. mul() arg order is intentionally
# reversed to match HLSL semantics (matrix * vector written as mul(v, M)).
HLSL_SHIMS = """\
#define mul(x, y) ((y) * (x))
#define lerp(a, b, t) mix(a, b, t)
#define frac(x) fract(x)
#define saturate(x) clamp(x, 0.0, 1.0)
#define CAST2(x) (vec2(x))
#define CAST3(x) (vec3(x))
#define CAST4(x) (vec4(x))
#define CAST3X3(x) (mat3(x))
#define float2 vec2
#define float3 vec3
#define float4 vec4
#define int2 ivec2
#define int3 ivec3
#define int4 ivec4
#define texSample2D(samp, uv) texture(samp, uv)
#define texture2D(samp, uv) texture(samp, uv)
#define textureCube(samp, uv) texture(samp, uv)
#define texSample2DLod(samp, uv, lod) textureLod(samp, uv, lod)
#define log10(x) (log2(x) * 0.301029995663981)
#define atan2(y, x) atan(y, x)
#define fmod(x, y) ((x) - (y) * trunc((x) / (y)))
#define ddx(x) dFdx(x)
#define ddy(x) dFdy(-(x))
#define texSample2DOffset(samp, uv, off) textureOffset(samp, uv, off)
#define GLSL 1
"""

_include_re = re.compile(r'^\s*#include\s+"([^"]+)"\s*$', re.MULTILINE)
_combo_re = re.compile(r'//\s*\[COMBO\]\s*(\{.*\})')
_uniform_re = re.compile(
    r'^\s*uniform\s+([\w]+)\s+(g_\w+)\s*;\s*(?://(.*))?\s*$', re.MULTILINE)


def _read_header(name, scene_dir=None):
    """Resolve a #include target. `name` may be like 'common.h' or 'common_blending.h'."""
    candidates = [
        os.path.join(SHADERS_DIR, name),
        os.path.join(SHADERS_DIR, name + '.h'),
        os.path.join(SHADERS_DIR, name.replace('.h', '') + '.h'),
    ]
    if scene_dir:
        candidates.extend([
            os.path.join(scene_dir, name),
            os.path.join(scene_dir, 'shaders', name),
            os.path.join(scene_dir, name + '.h'),
            os.path.join(scene_dir, 'shaders', name + '.h'),
        ])
    for c in candidates:
        if os.path.isfile(c):
            with open(c, 'r', encoding='utf-8') as f:
                return f.read()
    print(f"Warning: shader include not found: {name}, using empty fallback", file=sys.stderr)
    return ""


def resolve_includes(src, scene_dir=None):
    """Recursively replace #include "x" lines with the header body."""
    def expand(s):
        out = []
        for line in s.splitlines():
            m = re.match(r'^\s*#include\s+"([^"]+)"\s*$', line)
            if m:
                hdr = _read_header(m.group(1), scene_dir=scene_dir)
                out.append(expand(hdr) if hdr else "")
            else:
                out.append(line)
        return '\n'.join(out)
    return expand(src)


def parse_combos(src):
    """Return {COMBO_NAME: default_int} from [COMBO] metadata comments."""
    combos = {}
    for m in _combo_re.finditer(src):
        try:
            meta = json.loads(m.group(1))
        except Exception:
            continue
        name = meta.get('combo')
        if name and 'default' in meta:
            try:
                combos[name] = int(meta['default'])
            except (TypeError, ValueError):
                pass
    return combos


def parse_parameters(src):
    """Parse uniform declarations with trailing metadata comments.

    Returns a list of dicts: {name, type, material, default, combo, slot}.
    For sampler2D uniforms (g_TextureN), slot is the integer N and combo is the
    texture-driven combo (e.g. MASK) if declared.
    """
    params = []
    for m in _uniform_re.finditer(src):
        utype, uname, comment = m.group(1), m.group(2), m.group(3)
        entry = {'name': uname, 'type': utype, 'material': None,
                 'default': None, 'combo': None, 'slot': None}
        if comment:
            cm = re.search(r'\{.*\}', comment)
            if cm:
                try:
                    meta = json.loads(cm.group(0))
                    entry['material'] = meta.get('material')
                    entry['default'] = meta.get('default')
                    entry['combo'] = meta.get('combo')
                except Exception:
                    pass
        if uname.startswith('g_Texture') and uname[len('g_Texture'):].isdigit():
            entry['slot'] = int(uname[len('g_Texture'):])
        params.append(entry)
    return params


def _strip_metadata_comments(src):
    """Remove // [COMBO] {...} and trailing // {...} metadata from uniform lines."""
    out = []
    for line in src.splitlines():
        l = line
        # remove [COMBO] comment lines entirely
        if re.match(r'\s*//\s*\[COMBO\]', l):
            continue
        # strip trailing // {...} metadata on uniform lines
        l = re.sub(r'(uniform\s+[\w]+\s+g_\w+\s*;)\s*//\s*\{.*\}', r'\1', l)
        out.append(l)
    return '\n'.join(out)


# Identifiers that are RESERVED in GLSL ES but legal on desktop GLSL, so WE
# shaders use them freely (e.g. `vec4 sample = ...`). glslang renames these
# silently; we do a whole-word rename to match. `sample` is by far the most
# common offender. Extended with the rest of the ES reserved-for-future set
# that desktop code occasionally repurposes as identifiers.
_ES_RESERVED = ('sample', 'image', 'input', 'output', 'filter', 'partition',
                'active', 'subset', 'superp')
_reserved_re = re.compile(r'\b(' + '|'.join(_ES_RESERVED) + r')\b')


def _rename_es_reserved(src):
    return _reserved_re.sub(lambda m: m.group(1) + '_', src)


def _convert_to_es300(src, stage):
    """Rewrite GLSL ~120/330 constructs to GLSL ES 300."""
    s = src

    # Temporarily protect scientific notation literals (e.g. 1e-10, 1.0e-5)
    sci_matches = []
    def hide_sci(m):
        sci_matches.append(m.group(0))
        return f"__SCI_NOTATION_{len(sci_matches)-1}__"

    s = re.sub(r'\b\d+(?:\.\d+)?[eE][+-]?\d+\b', hide_sci, s)

    # Strip HLSL-style float literal 'f' suffix (e.g., 0.0f -> 0.0, 1f -> 1.0)
    s = re.sub(r'(\d+\.\d+)f\b', r'\1', s)
    s = re.sub(r'\b(\d+)f\b', r'\1.0', s)

    s = re.sub(r'\b(max|min|clamp|mix|smoothstep)\s*\(\s*(\d+)\s*,', r'\1(\2.0,', s)
    s = re.sub(r'\b(max|min|clamp|mix|smoothstep)\s*\(([^,]+),\s*(\d+)\s*([\),])', r'\1(\2, \3.0\4', s)

    def repl_mul_left(m):
        return f"{m.group(1)}.0 {m.group(2)} {m.group(3)}"

    def repl_mul_right(m):
        return f"{m.group(1)} {m.group(2)} {m.group(3)}.0"

    for _ in range(5):
        old_s = s
        s = re.sub(r'(?<!\.)\b(\d+)\b(?!\.)\s*([*+/])\s*([a-zA-Z_])', repl_mul_left, s)
        s = re.sub(r'((?:[a-zA-Z_]\w*(?:\.[a-zA-Z_]\w*)?|\d+\.\d+))\s*([*+/\-])\s*(?<!\.)\b(\d+)\b(?!\.)', repl_mul_right, s)
        if s == old_s:
            break

    # Restore scientific notation literals
    for i, sci in enumerate(sci_matches):
        s = s.replace(f"__SCI_NOTATION_{i}__", sci)

    if stage == 'vertex':
        s = re.sub(r'\battribute\b', 'in', s)
        s = re.sub(r'\bvarying\b', 'out', s)
    else:  # fragment
        s = re.sub(r'\bvarying\b', 'in', s)
        s = s.replace('gl_FragColor', 'out_FragColor')
    s = _rename_es_reserved(s)
    return s


def _build_source(body, stage, combos):
    """Assemble final WebGL2 GLSL ES 300 source from a prepped body."""
    body = _strip_metadata_comments(body)
    body = _convert_to_es300(body, stage)
    defines = [f"#define {k} {v}" for k, v in sorted(combos.items())]
    header_lines = [
        "#version 300 es",
        "precision highp float;",
        "precision highp int;",
    ]
    if stage == 'fragment':
        header_lines.append("out vec4 out_FragColor;")
    full = "\n".join(header_lines) + "\n" + HLSL_SHIMS + "\n" + "\n".join(defines) + "\n" + body
    return re.sub(r'\n{3,}', '\n\n', full)


def preprocess_shader(src_path, stage, bound_textures, extra_combos=None):
    """Transpile one WE shader to WebGL2 GLSL ES 300 (combos derived from this
    stage only). Prefer preprocess_program() for a matched vert/frag pair so the
    two stages share combos.
    """
    with open(src_path, 'r', encoding='utf-8') as f:
        raw = f.read()
    src = resolve_includes(raw)
    combos = parse_combos(raw)
    params = parse_parameters(src)
    bound_slots = set((bound_textures or {}).keys())
    for p in params:
        if p['type'] == 'sampler2D' and p['combo'] and p['slot'] in bound_slots:
            combos[p['combo']] = 1
    if extra_combos:
        combos.update(extra_combos)
    combos.setdefault('HLSL_SM30', 0)

    # Ensure all preprocessor conditional macro identifiers default to 0 if undefined
    cond_lines = re.findall(r'#\s*(?:if|elif)\s+(.*)', src)
    for line in cond_lines:
        for name in re.findall(r'\b[A-Z_][A-Z0-9_]*\b', line):
            if name not in ('GLSL', 'HLSL_SM30', '0', '1', '2', '3'):
                combos.setdefault(name, 0)

    glsl = _build_source(src, stage, combos)
    runtime_uniforms = [p for p in params
                        if p['type'] != 'sampler2D'
                        and not p['name'].startswith('g_Texture')
                        and p['name'] not in ('g_Time', 'g_Daytime',
                                              'g_ModelViewProjectionMatrix',
                                              'g_EffectModelViewProjectionMatrix',
                                              'g_PointerPosition', 'g_PointerPositionLast')]
    return {'glsl': glsl, 'combos': combos, 'uniforms': runtime_uniforms, 'params': params}


def preprocess_program(vert_path, frag_path, bound_textures, extra_combos=None):
    """Transpile a matched vert/frag pair with SHARED combos (mirrors lwe's
    linked shader program, where e.g. MASK is driven by the fragment sampler but
    also gates vertex uniforms).

    Returns {vert_glsl, frag_glsl, combos, uniforms, params}.
    """
    with open(vert_path, 'r', encoding='utf-8') as f:
        vert_raw = f.read()
    with open(frag_path, 'r', encoding='utf-8') as f:
        frag_raw = f.read()

    scene_dir = os.path.dirname(os.path.dirname(os.path.abspath(vert_path)))
    vert_src = resolve_includes(vert_raw, scene_dir=scene_dir)
    frag_src = resolve_includes(frag_raw, scene_dir=scene_dir)

    # union of [COMBO] defaults across both stages
    combos = {}
    combos.update(parse_combos(vert_raw))
    combos.update(parse_combos(frag_raw))

    # texture-driven combos from samplers declared in EITHER stage
    params = parse_parameters(vert_src) + parse_parameters(frag_src)
    bound_slots = set((bound_textures or {}).keys())
    for p in params:
        if p['type'] == 'sampler2D' and p['combo'] and p['slot'] in bound_slots:
            combos[p['combo']] = 1
    if extra_combos:
        combos.update(extra_combos)
    combos.setdefault('HLSL_SM30', 0)

    # Ensure all preprocessor conditional macro identifiers default to 0 if undefined
    cond_lines = re.findall(r'#\s*(?:if|elif)\s+(.*)', vert_src + '\n' + frag_src)
    for line in cond_lines:
        for name in re.findall(r'\b[A-Z_][A-Z0-9_]*\b', line):
            if name not in ('GLSL', 'HLSL_SM30', '0', '1', '2', '3'):
                combos.setdefault(name, 0)

    vert_glsl = _build_source(vert_src, 'vertex', combos)
    frag_glsl = _build_source(frag_src, 'fragment', combos)

    runtime_uniforms = [p for p in params
                        if p['type'] != 'sampler2D'
                        and not p['name'].startswith('g_Texture')
                        and p['name'] not in ('g_Time', 'g_Daytime',
                                              'g_ModelViewProjectionMatrix',
                                              'g_EffectModelViewProjectionMatrix',
                                              'g_PointerPosition', 'g_PointerPositionLast')]
    # de-dup uniforms by name (keep first)
    seen = set()
    deduped = []
    for u in runtime_uniforms:
        if u['name'] in seen:
            continue
        seen.add(u['name'])
        deduped.append(u)
    return {'vert_glsl': vert_glsl, 'frag_glsl': frag_glsl, 'combos': combos,
            'uniforms': deduped, 'params': params}


def coerce_value(raw, utype):
    """Coerce a constantshadervalues/default value to a JSON-friendly form.

    WE encodes vectors as space-separated strings ("1 1 1"); scalars as numbers.
    """
    if raw is None:
        return None
    if isinstance(raw, (int, float)):
        return float(raw)
    if isinstance(raw, list):
        return [float(x) for x in raw]
    if isinstance(raw, str):
        parts = raw.split()
        try:
            nums = [float(p) for p in parts]
        except ValueError:
            return raw
        if utype.startswith('vec') or utype.startswith('float'):
            return nums[0] if len(nums) == 1 else nums
        return nums
    return raw


if __name__ == '__main__':
    # Self-test: preprocess pulse + waterwaves from the unpacked wallpaper.
    import sys
    base = sys.argv[1] if len(sys.argv) > 1 else '/tmp/HyprWpE/2548825361'
    sh = os.path.join(base, 'shaders', 'effects')
    tests = [
        ('pulse', {1: 'util/noise', 2: 'masks/pulse_mask_25ed3d2d'}),
        ('waterwaves', {1: 'masks/waterwaves_mask_af5f5ee5'}),
    ]
    for name, tex in tests:
        vp, fp = os.path.join(sh, name + '.vert'), os.path.join(sh, name + '.frag')
        if not (os.path.isfile(vp) and os.path.isfile(fp)):
            print(f"skip {name}: not found"); continue
        r = preprocess_program(vp, fp, tex)
        print(f"\n===== {name} combos={r['combos']} =====")
        print("--- VERT ---\n" + r['vert_glsl'][:900])
        print("--- FRAG ---\n" + r['frag_glsl'][:900])
        print(f"  uniforms to bind: {[ (u['name'], u['material'], u['default']) for u in r['uniforms'] ]}")
