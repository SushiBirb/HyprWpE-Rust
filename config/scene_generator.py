"""
Scene generator: reads an unpacked Wallpaper Engine scene and emits a self-
contained index.html that renders it with WebGL2 using the wallpaper's OWN
effect shaders (transpiled to GLSL ES 300 by glsl_preprocess.py).

This replaces linux-wallpaperengine for scene wallpapers. The generated page
contains the full scene graph, preprocessed shaders and resolved texture paths,
so it has no runtime dependency on anything but the unpacked assets.
"""
import os
import sys
import json
import shutil

from glsl_preprocess import preprocess_program, coerce_value

CONFIG_DIR = os.path.dirname(os.path.abspath(__file__))
TEMPLATE_PATH = os.path.join(CONFIG_DIR, 'scene_template.html')
GLOBAL_UTIL_DIR = os.path.join(CONFIG_DIR, 'util')


def _vec(s, n=3):
    """Parse a WE space-separated vector string into a list of floats."""
    if isinstance(s, list):
        return [float(x) for x in s]
    if isinstance(s, (int, float)):
        return [float(s)]
    return [float(x) for x in str(s).split()]


def _resolve_texture_path(ref, unpacked):
    """Resolve a WE texture reference to a relative path (from the unpacked root)
    that the served page can load. Returns (relative_path, absolute_path).
    Handles package textures (materials/...), global utils (util/...) and bare names.
    """
    if not ref:
        return None, None
    # explicit global util reference (shipped in config/util, not in the pkg)
    if ref.startswith('util/') or ref == 'util':
        name = ref.split('/')[-1]
        src = os.path.join(GLOBAL_UTIL_DIR, name + '.png')
        if os.path.isfile(src):
            dst_dir = os.path.join(unpacked, 'materials', 'util')
            os.makedirs(dst_dir, exist_ok=True)
            dst = os.path.join(dst_dir, name + '.png')
            if not os.path.isfile(dst):
                shutil.copyfile(src, dst)
            return 'materials/util/' + name + '.png', dst

    # particle textures
    clean_path = ref.replace('\\', '/').removeprefix('materials/')
    if clean_path.startswith('particle/'):
        part_name = clean_path.removeprefix('particle/')
        dst_part = os.path.join(unpacked, 'particle', part_name + '.png')
        if not os.path.isfile(dst_part):
            os.makedirs(os.path.dirname(dst_part), exist_ok=True)
            cand_srcs = [
                os.path.join(CONFIG_DIR, 'assets', 'particle', part_name + '.png'),
                os.path.join('/home/scushi/HyprWpE-Rust/config/assets/particle', part_name + '.png'),
                os.path.join('/home/scushi/HyprWpE/config/assets/particle', part_name + '.png'),
                os.path.join('/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets/materials/particle', part_name + '.png'),
                os.path.join('/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets/particle', part_name + '.png'),
            ]
            for cs in cand_srcs:
                if os.path.isfile(cs):
                    shutil.copyfile(cs, dst_part)
                    break
        if os.path.isfile(dst_part):
            return os.path.relpath(dst_part, unpacked).replace(os.sep, '/'), dst_part

    # package textures: prefer an already-extracted .png next to a .tex
    for root_rel in ('materials/' + ref, ref):
        png = os.path.join(unpacked, root_rel + '.png')
        if os.path.isfile(png):
            return os.path.relpath(png, unpacked).replace(os.sep, '/'), png

    # Render target / missing texture fallback: create a 128x128 fallback texture
    clean_ref = ref.replace(':', '_').replace('?', '_')
    dst = os.path.join(unpacked, 'materials', clean_ref + '.png')
    if not os.path.isfile(dst):
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        try:
            from PIL import Image
            img = Image.new('RGBA', (128, 128), (100, 140, 200, 255))
            img.save(dst)
        except Exception:
            pass
    rel = os.path.relpath(dst, unpacked).replace(os.sep, '/')
    return rel, dst


def _load_json(path):
    with open(path, 'r', encoding='utf-8') as f:
        return json.load(f)


def _resolve_object_texture(obj, unpacked):
    """Walk object.image -> models/*.json -> materials/*.json -> base texture name."""
    if not isinstance(obj, dict):
        return _resolve_texture_path('default', unpacked) + ('translucent',)
    image_ref = obj.get('image') or ''
    tex_name = obj.get('name') or 'default'
    try:
        model = _load_json(os.path.join(unpacked, image_ref))
        mat_path = model.get('material')
        if mat_path and os.path.isfile(os.path.join(unpacked, mat_path)):
            mat = _load_json(os.path.join(unpacked, mat_path))
            passes = mat.get('passes', [])
            if passes and passes[0].get('textures'):
                tex_name = passes[0]['textures'][0]
            blend = passes[0].get('blending', 'translucent') if passes else 'translucent'
    except Exception:
        blend = 'translucent'
    rel, abst = _resolve_texture_path(tex_name, unpacked)
    return rel, abst, blend


def _build_effect(effect, unpacked):
    """Transpile one effect (from scene.json object.effects[]) into a render descriptor."""
    eff_file = effect.get('file')  # e.g. "effects/pulse/effect.json"
    if not eff_file:
        return None
    try:
        eff_json = _load_json(os.path.join(unpacked, eff_file))
    except Exception as e:
        print(f"  ! could not read effect {eff_file}: {e}")
        return None

    passes = eff_json.get('passes', [])
    if not passes:
        return None
    pass0 = passes[0]
    material_ref = pass0.get('material')  # e.g. "materials/effects/pulse.json"
    shader_root = None
    try:
        mat = _load_json(os.path.join(unpacked, material_ref))
        shader_root = mat.get('passes', [{}])[0].get('shader')  # e.g. "effects/pulse"
    except Exception as e:
        print(f"  ! could not read effect material {material_ref}: {e}")
        return None
    if not shader_root:
        return None

    vert_path = os.path.join(unpacked, 'shaders', shader_root + '.vert')
    frag_path = os.path.join(unpacked, 'shaders', shader_root + '.frag')
    if not (os.path.isfile(vert_path) and os.path.isfile(frag_path)):
        print(f"  ! shader not found for {shader_root}")
        return None

    # textures bound in this pass (from the scene object's effect definition)
    raw_textures = effect.get('passes', [{}])[0].get('textures', []) if effect.get('passes') else []
    bound = {}
    resolved_tex = {}
    for i, t in enumerate(raw_textures):
        if t:
            bound[i] = t
            rel, _ = _resolve_texture_path(t, unpacked)
            resolved_tex[i] = rel

    # constants provided by the wallpaper author
    constants = {}
    if effect.get('passes'):
        constants = effect['passes'][0].get('constantshadervalues', {}) or {}

    extra_combos = {}
    try:
        mat_combos = mat.get('passes', [{}])[0].get('combos', {}) or {}
        extra_combos.update({k.upper(): int(v) for k, v in mat_combos.items()})
    except Exception:
        pass

    prog = preprocess_program(vert_path, frag_path, bound, extra_combos)

    # resolve uniform values: wallpaper constants override shader defaults
    uniforms = {}
    for u in prog['uniforms']:
        uname = u['name']
        mat_key = u.get('material')
        if mat_key and mat_key in constants:
            uniforms[uname] = coerce_value(constants[mat_key], u['type'])
        elif uname in constants:
            uniforms[uname] = coerce_value(constants[uname], u['type'])
        elif u.get('default') is not None:
            uniforms[uname] = coerce_value(u['default'], u['type'])

    return {
        'name': shader_root.split('/')[-1],
        'vert': prog['vert_glsl'],
        'frag': prog['frag_glsl'],
        'combos': prog['combos'],
        'uniforms': uniforms,
        'textures': resolved_tex,  # {slot: relative_path}; slot 0 is the object buffer
    }


def _resolve_particle_system(obj, unpacked):
    p_ref = obj.get('particle') or obj.get('particlesrc') or ''
    if not p_ref:
        return []
    p_clean = p_ref.replace('\\', '/')
    preset_name = p_clean.replace('particles/', '').replace('presets/', '').removesuffix('.json').split('/')[0]
    we_base = '/home/scushi/.local/share/Steam/steamapps/common/wallpaper_engine/assets'
    cand_paths = [
        os.path.join(unpacked, p_clean),
        os.path.join(unpacked, p_clean + '.json'),
        os.path.join(we_base, p_clean),
        os.path.join(we_base, p_clean + '.json'),
    ]
    if preset_name:
        cand_paths.append(os.path.join(we_base, 'presets', preset_name, p_clean))
        cand_paths.append(os.path.join(we_base, 'presets', preset_name, f"particles/presets/{preset_name}.json"))
        cand_paths.append(os.path.join(we_base, 'presets', preset_name, f"{preset_name}.json"))

    pj = None
    for cand in cand_paths:
        if os.path.isfile(cand):
            try:
                pj = _load_json(cand)
                break
            except Exception:
                pass
    if not pj:
        return []

    def _build_part_desc(p_json, p_id, p_name, parent_id=None, child_type=None):
        mat_ref = (p_json.get('material') or '').replace('\\', '/')
        mat_clean = mat_ref.replace('materials/', '').replace('presets/', '').removesuffix('.json').split('/')[0] if mat_ref else preset_name
        mat_cand = [
            os.path.join(unpacked, mat_ref),
            os.path.join(unpacked, mat_ref + '.json'),
            os.path.join(we_base, mat_ref),
            os.path.join(we_base, mat_ref + '.json'),
        ]
        if mat_clean:
            mat_cand.append(os.path.join(we_base, 'presets', mat_clean, mat_ref))
            mat_cand.append(os.path.join(we_base, 'presets', mat_clean, f"materials/presets/{mat_clean}.json"))
            mat_cand.append(os.path.join(we_base, 'presets', mat_clean, f"{mat_clean}.json"))

        mj = None
        for mc in mat_cand:
            if os.path.isfile(mc):
                try:
                    mj = _load_json(mc)
                    break
                except Exception:
                    pass
        blending = 'additive'
        p_tex = ''
        overbright = 1.0
        if mj and mj.get('passes'):
            p0 = mj['passes'][0]
            blending = p0.get('blending', 'additive')
            overbright = p0.get('constantshadervalues', {}).get('ui_editor_properties_overbright', 1.0)
            if p0.get('textures'):
                tex_ref = p0['textures'][0]
                p_tex, _ = _resolve_texture_path(tex_ref, unpacked)
        if not p_tex:
            p_tex, _ = _resolve_texture_path('util/white', unpacked)

        spritesheet = None
        if tex_ref:
            tex_json_cand = [
                os.path.join(unpacked, tex_ref + '.tex-json'),
                os.path.join(unpacked, 'materials', tex_ref + '.tex-json'),
                os.path.join(we_base, 'materials', tex_ref + '.tex-json'),
                os.path.join(we_base, tex_ref + '.tex-json'),
            ]
            for c in tex_json_cand:
                if os.path.isfile(c):
                    try:
                        with open(c, 'r', encoding='utf-8') as f:
                            tj = json.load(f)
                        seqs = tj.get('spritesheetsequences')
                        if seqs and len(seqs) > 0:
                            seq = seqs[0]
                            frames = seq.get('frames', 1)
                            fw = seq.get('width', 0)
                            fh = seq.get('height', 0)
                            if frames > 1 and fw > 0 and fh > 0:
                                cols = max(1, round(512.0 / fw))
                                rows = max(1, round(512.0 / fh))
                                spritesheet = {
                                    'frames': frames,
                                    'cols': cols,
                                    'rows': rows,
                                    'duration': seq.get('duration', 1.0),
                                }
                                break
                    except Exception:
                        pass

        return {
            'id': p_id,
            'name': p_name,
            'kind': 'particle',
            'visible': obj.get('visible', True),
            'parent_particle_id': parent_id,
            'child_type': child_type,
            'origin': _vec(obj.get('origin', '0 0 0')),
            'scale': _vec(obj.get('scale', '1 1 1')),
            'angles': _vec(obj.get('angles', '0 0 0')),
            'color': _vec(obj.get('color', '1 1 1')),
            'alpha': float(obj.get('alpha', 1.0)),
            'instanceoverride': obj.get('instanceoverride') or {},
            'maxcount': p_json.get('maxcount', 100),
            'starttime': float(p_json.get('starttime', 0.0)),
            'spritesheet': spritesheet,
            'sequencemultiplier': float(p_json.get('sequencemultiplier', 1.0) or 1.0),
            'material': {
                'blending': blending,
                'texture': p_tex,
                'overbright': overbright,
            },
            'emitter': p_json.get('emitter', []),
            'initializer': p_json.get('initializer', []),
            'operator': p_json.get('operator', []),
            'renderer': p_json.get('renderer', []),
        }

    results = []
    main_desc = _build_part_desc(pj, obj.get('id'), obj.get('name', 'particle'))
    results.append(main_desc)

    for ch in pj.get('children') or []:
        ch_ref = (ch.get('name') or '').replace('\\', '/')
        if not ch_ref:
            continue
        ch_preset = ch_ref.replace('particles/', '').replace('presets/', '').removesuffix('.json').split('/')[0]
        ch_cand = [
            os.path.join(unpacked, ch_ref),
            os.path.join(unpacked, ch_ref + '.json'),
            os.path.join(we_base, ch_ref),
            os.path.join(we_base, ch_ref + '.json'),
        ]
        if ch_preset:
            ch_cand.append(os.path.join(we_base, 'presets', ch_preset, ch_ref))
            ch_cand.append(os.path.join(we_base, 'presets', ch_preset, f"particles/presets/{ch_preset}.json"))
            ch_cand.append(os.path.join(we_base, 'presets', ch_preset, f"{ch_preset}.json"))

        ch_pj = None
        for cc in ch_cand:
            if os.path.isfile(cc):
                try:
                    ch_pj = _load_json(cc)
                    break
                except Exception:
                    pass
        if ch_pj:
            ch_desc = _build_part_desc(
                ch_pj,
                ch.get('id'),
                f"{obj.get('name', 'particle')}_{ch.get('id')}",
                parent_id=obj.get('id'),
                child_type=ch.get('type')
            )
            results.append(ch_desc)

    return results


def generate_scene_html(unpacked_dir):
    scene_path = os.path.join(unpacked_dir, 'scene.json')
    if not os.path.exists(scene_path):
        print(f"Error: scene.json not found in {unpacked_dir}")
        return False
    if not os.path.exists(TEMPLATE_PATH):
        print(f"Error: scene_template.html not found at {TEMPLATE_PATH}")
        return False

    scene = _load_json(scene_path)
    general = scene.get('general', {})
    clear = _vec(general.get('clearcolor', '1 1 1'))
    proj = general.get('orthogonalprojection') or {}
    pw = proj.get('width', 1920) if isinstance(proj, dict) else 1920
    ph = proj.get('height', 1080) if isinstance(proj, dict) else 1080

    objects = []
    for obj in scene.get('objects', []):
        if not obj.get('visible', True):
            continue
        if obj.get('sound'):
            objects.append({'kind': 'sound', 'src': obj['sound'][0], 'volume': obj.get('volume', 1.0)})
            continue
        if obj.get('particle') or obj.get('particlesrc') or obj.get('kind') == 'particle':
            parts = _resolve_particle_system(obj, unpacked_dir)
            objects.extend(parts)
            for p in parts:
                print(f"  + particle '{p['name']}' tex={p['material']['texture']}")
            continue
        if not obj.get('image'):
            continue

        tex_rel, _, blend = _resolve_object_texture(obj, unpacked_dir)
        descriptor = {
            'kind': 'image',
            'name': obj.get('name', ''),
            'visible': obj.get('visible', True),
            'texture': tex_rel,
            'origin': _vec(obj.get('origin', '0 0 0'))[:2],
            'size': _vec(obj.get('size', '0 0'))[:2],
            'scale': _vec(obj.get('scale', '1 1 1'))[:2],
            'angles': _vec(obj.get('angles', '0 0 0')),
            'alignment': obj.get('alignment', 'center'),
            'color': _vec(obj.get('color', '1 1 1')),
            'alpha': float(obj.get('alpha', 1.0)),
            'brightness': float(obj.get('brightness', 1.0)),
            'blend': blend,
            'effects': [],
        }

        for eff in obj.get('effects', []):
            if not eff.get('visible', True):
                continue
            built = _build_effect(eff, unpacked_dir)
            if built:
                descriptor['effects'].append(built)

        objects.append(descriptor)
        print(f"  + object '{descriptor['name']}' tex={tex_rel} effects={[e['name'] for e in descriptor['effects']]}")

    with open(TEMPLATE_PATH, 'r', encoding='utf-8') as f:
        template = f.read()

    payload = {
        'clearcolor': clear,
        'projection': [pw, ph],
        'ambient': _vec(general.get('ambientcolor', '0 0 0')),
        'objects': objects,
    }
    rendered = template.replace('__SCENE_DATA_JSON__', json.dumps(payload))
    out_path = os.path.join(unpacked_dir, 'index.html')
    with open(out_path, 'w', encoding='utf-8') as f:
        f.write(rendered)
    print(f"Successfully compiled WebGL2 scene -> {out_path} ({len(objects)} objects)")
    return True


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python scene_generator.py <unpacked_scene_directory>")
        sys.exit(1)
    sys.exit(0 if generate_scene_html(sys.argv[1]) else 1)
