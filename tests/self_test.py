#!/usr/bin/env python3
"""
HyprWpE Scene Rendering Self-Test Suite

Automatically builds a self-contained synthetic WebGL scene fixture, runs unit
tests on GLSL transpilation and scene generation, and executes the Playwright
WebGL test harness to verify end-to-end scene rendering functionality.

Usage:
    python3 tests/self_test.py [--keep-output] [--json]
"""
import os
import sys
import json
import shutil
import subprocess
import tempfile
import argparse

# Paths
TESTS_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(TESTS_DIR)
CONFIG_DIR = os.path.join(PROJECT_ROOT, 'config')

sys.path.insert(0, CONFIG_DIR)

try:
    from glsl_preprocess import preprocess_program, coerce_value
    from scene_generator import generate_scene_html
except ImportError as e:
    print(f"FAILED to import HyprWpE config modules: {e}")
    sys.exit(1)


def create_minimal_png(filepath, width=64, height=64, color=(100, 150, 200, 255), pattern=False):
    """Create a minimal raw uncompressed PNG image without external imaging libraries."""
    import zlib
    import struct

    def make_chunk(chunk_type, data):
        return struct.pack('>I', len(data)) + chunk_type + data + struct.pack('>I', zlib.crc32(chunk_type + data) & 0xffffffff)

    raw_data = bytearray()
    for y in range(height):
        raw_data.append(0)  # no filter
        for x in range(width):
            if pattern:
                r = int(30 + 190 * (x / width))
                g = int(20 + 180 * (y / height))
                b = int(color[2])
                raw_data.extend([r, g, b, 255])
            else:
                raw_data.extend(color)

    compressed = zlib.compress(raw_data)

    header = b'\x89PNG\r\n\x1a\n'
    ihdr = make_chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
    idat = make_chunk(b'IDAT', compressed)
    iend = make_chunk(b'IEND', b'')

    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    with open(filepath, 'wb') as f:
        f.write(header + ihdr + idat + iend)


def build_synthetic_scene_fixture(target_dir):
    """Generates a complete unpacked Wallpaper Engine scene structure for testing."""
    os.makedirs(target_dir, exist_ok=True)

    # 1. project.json
    project_json = {
        "file": "scene.json",
        "general": {"properties": {}},
        "title": "Synthetic Test Scene",
        "type": "scene",
        "version": 1
    }
    with open(os.path.join(target_dir, "project.json"), "w", encoding="utf-8") as f:
        json.dump(project_json, f, indent=2)

    # 2. scene.json
    scene_json = {
        "general": {
            "ambientcolor": "0.2 0.2 0.2",
            "clearcolor": "0.1 0.15 0.25",
            "orthogonalprojection": {"height": 1080, "width": 1920}
        },
        "objects": [
            {
                "name": "bg_layer",
                "visible": True,
                "image": "models/bg.json",
                "origin": "960 540 0",
                "size": "1920 1080",
                "scale": "1 1 1",
                "angles": "0 0 0",
                "color": "1 1 1",
                "alpha": 1.0,
                "brightness": 1.0,
                "effects": [
                    {
                        "file": "effects/pulse/effect.json",
                        "name": "pulse",
                        "visible": True,
                        "passes": [
                            {
                                "constantshadervalues": {
                                    "g_PulseSpeed": 2.5,
                                    "g_PulseAmount": 0.3
                                },
                                "textures": [None, "materials/effects/mask"]
                            }
                        ]
                    }
                ]
            }
        ]
    }
    with open(os.path.join(target_dir, "scene.json"), "w", encoding="utf-8") as f:
        json.dump(scene_json, f, indent=2)

    # 3. Model & Material files
    os.makedirs(os.path.join(target_dir, "models"), exist_ok=True)
    with open(os.path.join(target_dir, "models", "bg.json"), "w", encoding="utf-8") as f:
        json.dump({"material": "materials/bg_mat.json"}, f, indent=2)

    os.makedirs(os.path.join(target_dir, "materials"), exist_ok=True)
    with open(os.path.join(target_dir, "materials", "bg_mat.json"), "w", encoding="utf-8") as f:
        json.dump({
            "passes": [
                {
                    "blending": "translucent",
                    "textures": ["bg_tex"]
                }
            ]
        }, f, indent=2)

    # 4. Effect definition & Effect Material
    os.makedirs(os.path.join(target_dir, "effects", "pulse"), exist_ok=True)
    with open(os.path.join(target_dir, "effects", "pulse", "effect.json"), "w", encoding="utf-8") as f:
        json.dump({
            "passes": [
                {
                    "material": "materials/effects/pulse_mat.json"
                }
            ]
        }, f, indent=2)

    os.makedirs(os.path.join(target_dir, "materials", "effects"), exist_ok=True)
    with open(os.path.join(target_dir, "materials", "effects", "pulse_mat.json"), "w", encoding="utf-8") as f:
        json.dump({
            "passes": [
                {
                    "shader": "effects/pulse"
                }
            ]
        }, f, indent=2)

    # 5. Shaders
    os.makedirs(os.path.join(target_dir, "shaders", "effects"), exist_ok=True)

    vert_shader = """// Wallpaper Engine GLSL ES Vertex Shader
attribute vec3 a_Position;
attribute vec2 a_TexCoord;
varying vec2 v_TexCoord;
uniform mat4 g_ModelViewProjectionMatrix;

void main() {
    v_TexCoord = a_TexCoord;
    gl_Position = g_ModelViewProjectionMatrix * vec4(a_Position, 1.0);
}
"""
    frag_shader = """// Wallpaper Engine GLSL ES Fragment Shader
// [COMBO] {"MASK": 1}
varying vec2 v_TexCoord;
uniform sampler2D g_Texture0;
uniform sampler2D g_Texture1; // {"combo": "MASK"}
uniform float g_Time;
uniform float g_PulseSpeed; // {"material": "g_PulseSpeed", "default": 2.5}
uniform float g_PulseAmount; // {"material": "g_PulseAmount", "default": 0.3}

void main() {
    vec4 color = texture2D(g_Texture0, v_TexCoord);
    vec4 mask = texture2D(g_Texture1, v_TexCoord);
    float pulse = 1.0 + sin(g_Time * g_PulseSpeed) * g_PulseAmount * mask.r;
    color.rgb *= pulse;
    gl_FragColor = color;
}
"""
    with open(os.path.join(target_dir, "shaders", "effects", "pulse.vert"), "w", encoding="utf-8") as f:
        f.write(vert_shader)
    with open(os.path.join(target_dir, "shaders", "effects", "pulse.frag"), "w", encoding="utf-8") as f:
        f.write(frag_shader)

    # 6. Textures
    create_minimal_png(os.path.join(target_dir, "materials", "bg_tex.png"), 64, 64, (80, 140, 200, 255), pattern=True)
    create_minimal_png(os.path.join(target_dir, "materials", "effects", "mask.png"), 64, 64, (255, 255, 255, 255))
    create_minimal_png(os.path.join(target_dir, "preview.png"), 64, 64, (80, 140, 200, 255), pattern=True)


def run_unit_tests(target_dir):
    """Performs unit testing of GLSL preprocessor and scene generator."""
    print(">>> [Unit Test 1/2] Testing glsl_preprocess.py...")
    vert_path = os.path.join(target_dir, "shaders", "effects", "pulse.vert")
    frag_path = os.path.join(target_dir, "shaders", "effects", "pulse.frag")

    prog = preprocess_program(vert_path, frag_path, bound_textures={1: "materials/effects/mask"})
    assert "version 300 es" in prog["vert_glsl"], "Preprocessed vertex shader missing '#version 300 es'"
    assert "version 300 es" in prog["frag_glsl"], "Preprocessed fragment shader missing '#version 300 es'"
    assert any(u["name"] == "g_PulseSpeed" for u in prog["uniforms"]), "g_PulseSpeed uniform missing from preprocessor output"
    print("    PASS: GLSL ES 300 transpilation & uniform extraction verified.")

    print(">>> [Unit Test 2/2] Testing scene_generator.py...")
    success = generate_scene_html(target_dir)
    assert success, "generate_scene_html returned False"
    index_html = os.path.join(target_dir, "index.html")
    assert os.path.isfile(index_html), "index.html was not generated"

    with open(index_html, "r", encoding="utf-8") as f:
        content = f.read()
    assert "__SCENE_DATA_JSON__" not in content, "Placeholder __SCENE_DATA_JSON__ was not replaced"
    assert "bg_layer" in content, "bg_layer object missing from generated HTML"
    print("    PASS: Native WebGL scene HTML generation verified.")


def run_playwright_harness(target_dir, out_dir):
    """Executes the Playwright headless WebGL harness and parses report.json."""
    print(">>> [Integration Test] Running Playwright WebGL Harness (harness.js)...")
    harness_path = os.path.join(TESTS_DIR, "harness.js")
    preview_path = os.path.join(target_dir, "preview.png")

    cmd = [
        "node", harness_path,
        target_dir, preview_path,
        "--out", out_dir
    ]

    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    report_file = os.path.join(out_dir, "report.json")

    if not os.path.isfile(report_file):
        print(f"FAILED: harness.js did not produce report.json. Stderr:\n{res.stderr}")
        return False, {"error": "no report.json"}

    with open(report_file, "r", encoding="utf-8") as f:
        report = json.load(f)

    return report.get("allChecksPassed", False), report


def main():
    parser = argparse.ArgumentParser(description="HyprWpE Scene Rendering Self-Test")
    parser.add_argument("--keep-output", action="store_true", help="Keep generated test files and output directory")
    parser.add_argument("--json", action="store_true", help="Output results strictly as JSON")
    args = parser.parse_args()

    temp_dir = tempfile.mkdtemp(prefix="HyprWpE_synthetic_")
    out_dir = os.path.join(TESTS_DIR, "output", "synthetic")

    try:
        if not args.json:
            print("==================================================")
            print("        HyprWpE Native Scene Self-Test           ")
            print("==================================================")

        # Step 1: Fixture construction
        build_synthetic_scene_fixture(temp_dir)

        # Step 2: Unit tests
        run_unit_tests(temp_dir)

        # Step 3: Playwright WebGL harness integration test
        harness_passed, report = run_playwright_harness(temp_dir, out_dir)

        # Summary
        if args.json:
            print(json.dumps(report, indent=2))
        else:
            print("\n==================================================")
            print("               TEST SUMMARY RESULTS               ")
            print("==================================================")
            checks = report.get("checks", [])
            for c in checks:
                status = "PASS" if c["pass"] else "FAIL"
                print(f"  [{status}] {c['name']}")
                if not c["pass"]:
                    print(f"         Detail: {c.get('detail')}")

            passed_count = report.get("checksPassed", 0)
            total_count = report.get("checksTotal", 0)
            print("--------------------------------------------------")
            print(f"OVERALL RESULT: {'PASS' if harness_passed else 'FAIL'} ({passed_count}/{total_count} checks passed)")
            print("==================================================")

        sys.exit(0 if harness_passed else 1)

    finally:
        if not args.keep_output:
            shutil.rmtree(temp_dir, ignore_errors=True)


if __name__ == "__main__":
    main()
