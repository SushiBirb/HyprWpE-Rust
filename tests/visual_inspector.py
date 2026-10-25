#!/usr/bin/env python3
"""
HyprWpE Automated Visual Inspection & 1-100 Quality Scoring Harness

Calculates a comprehensive 1-100 quality score for scene wallpapers by:
1. Rendering WebGL scene frames at multiple timestamps (t=0s, t=2s, t=5s).
2. Comparing rendered frames against official Steam preview assets (preview.jpg/preview.gif).
3. Analyzing animation motion dynamics across frames.
4. Detecting visual artifacts (solid color masks, vertical line stretching, WebGL errors).
5. Generating side-by-side comparison images.

Usage:
  python3 tests/visual_inspector.py --id <wallpaper_id> [--out <dir>]
  python3 tests/visual_inspector.py --all [--limit N]
"""

import os
import sys
import json
import math
import argparse
import subprocess
import shutil
from PIL import Image, ImageChops, ImageDraw, ImageFont
import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "config"))
from unpacker import unpack_pkg
from scene_generator import generate_scene_html

STEAM_WORKSHOP = "/home/scushi/.local/share/Steam/steamapps/workshop/content/431960"
DEFAULT_OUT = "/tmp/hyprwpe_inspector"


def calc_color_histogram_similarity(img_a, img_b):
    """Calculate color distribution correlation in range [0, 100]."""
    a = img_a.resize((256, 256)).convert("RGB")
    b = img_b.resize((256, 256)).convert("RGB")

    hist_a = np.array(a.histogram(), dtype=np.float64)
    hist_b = np.array(b.histogram(), dtype=np.float64)

    # Normalize
    sa, sb = np.sum(hist_a), np.sum(hist_b)
    if sa > 0: hist_a /= sa
    if sb > 0: hist_b /= sb

    # Cosine similarity
    dot = np.dot(hist_a, hist_b)
    norm_a = np.linalg.norm(hist_a)
    norm_b = np.linalg.norm(hist_b)

    if norm_a == 0 or norm_b == 0:
        return 0.0

    cos_sim = dot / (norm_a * norm_b)
    return float(np.clip(cos_sim * 100.0, 0.0, 100.0))


def calc_structural_gradient_similarity(img_a, img_b):
    """Calculate Sobel gradient edge structure similarity in range [0, 100]."""
    a = np.array(img_a.resize((256, 256)).convert("L"), dtype=np.float64)
    b = np.array(img_b.resize((256, 256)).convert("L"), dtype=np.float64)

    # Simple Sobel gradients
    gx_a = np.abs(np.diff(a, axis=1))
    gx_b = np.abs(np.diff(b, axis=1))

    gy_a = np.abs(np.diff(a, axis=0))
    gy_b = np.abs(np.diff(b, axis=0))

    # Mean absolute error of normalized gradients
    max_g = 255.0
    err_x = np.mean(np.abs(gx_a - gx_b)) / max_g
    err_y = np.mean(np.abs(gy_a - gy_b)) / max_g

    avg_err = (err_x + err_y) / 2.0
    score = (1.0 - avg_err) * 100.0
    return float(np.clip(score, 0.0, 100.0))


def calc_motion_dynamics(frame0_path, frame2_path, frame5_path):
    """Calculate animation motion score in range [0, 100] based on pixel deltas."""
    if not (os.path.isfile(frame0_path) and os.path.isfile(frame2_path)):
        return 0.0

    f0 = np.array(Image.open(frame0_path).convert("RGB"), dtype=np.float64)
    f2 = np.array(Image.open(frame2_path).convert("RGB"), dtype=np.float64)
    
    diff = np.abs(f0 - f2)
    max_mad = np.max(diff)
    mean_mad = np.mean(diff)

    if os.path.isfile(frame5_path):
        f5 = np.array(Image.open(frame5_path).convert("RGB"), dtype=np.float64)
        diff5 = np.abs(f2 - f5)
        mean_mad = (mean_mad + np.mean(diff5)) / 2.0

    # Scale motion score: mean_mad of 5.0+ represents active animation
    motion_score = min(100.0, (mean_mad / 6.0) * 100.0)
    if mean_mad < 0.2:
        # Static scene
        motion_score = 30.0  # static but valid
    return float(motion_score)


def detect_artifact_penalties(img):
    """Detect visual artifacts (solid single-color flat fallback mask or black screen)."""
    arr = np.array(img.convert("RGB"))
    std_dev = np.std(arr)
    penalty = 0.0

    # Standard deviation < 1.0 means an unextracted single flat color fallback or completely empty screen
    if std_dev < 1.0:
        penalty += 60.0

    return float(penalty)


def create_comparison_collage(preview_path, frame_path, score_info, out_collage_path):
    """Generate side-by-side visual comparison image with score breakdown overlay."""
    target_w, target_h = 640, 360
    canvas_w = target_w * 2 + 60
    canvas_h = target_h + 120

    canvas = Image.new("RGB", (canvas_w, canvas_h), (24, 26, 32))
    draw = ImageDraw.Draw(canvas)

    # Load Preview
    if preview_path and os.path.isfile(preview_path):
        try:
            prev_img = Image.open(preview_path).convert("RGB").resize((target_w, target_h))
            canvas.paste(prev_img, (20, 80))
        except Exception:
            pass
    else:
        draw.rectangle([20, 80, 20 + target_w, 80 + target_h], fill=(40, 44, 52))
        draw.text((40, 100), "No Preview Available", fill=(200, 200, 200))

    # Load Rendered Frame
    if frame_path and os.path.isfile(frame_path):
        try:
            rend_img = Image.open(frame_path).convert("RGB").resize((target_w, target_h))
            canvas.paste(rend_img, (target_w + 40, 80))
        except Exception:
            pass

    # Header text
    title_text = f"Wallpaper: {score_info['title']} (ID: {score_info['id']})"
    score_text = f"FINAL QUALITY SCORE: {score_info['final_score']:.1f} / 100"

    draw.text((20, 15), title_text, fill=(255, 255, 255))
    draw.text((20, 45), f"Steam Preview Asset", fill=(160, 160, 160))
    draw.text((target_w + 40, 45), f"Rendered WebGL2 Scene", fill=(160, 160, 160))

    # Score breakdown footer
    breakdown = (
        f"Color Sim: {score_info['color_score']:.1f}% | "
        f"Structure: {score_info['struct_score']:.1f}% | "
        f"Motion: {score_info['motion_score']:.1f}% | "
        f"Stability: {score_info['stability_score']:.1f}% | "
        f"Penalty: -{score_info['artifact_penalty']:.1f}"
    )
    draw.text((20, canvas_h - 30), breakdown, fill=(200, 220, 255))
    draw.text((canvas_w - 260, 15), score_text, fill=(80, 220, 120) if score_info['final_score'] >= 70 else (240, 80, 80))

    os.makedirs(os.path.dirname(out_collage_path), exist_ok=True)
    canvas.save(out_collage_path)


def inspect_wallpaper(wid, steam_dir=STEAM_WORKSHOP, out_base=DEFAULT_OUT):
    """Run full automated visual inspection and scoring on a single wallpaper ID."""
    wp_path = os.path.join(steam_dir, wid)
    if not os.path.isdir(wp_path):
        return {"id": wid, "error": "Wallpaper directory not found"}

    title = wid
    pjson = os.path.join(wp_path, "project.json")
    if os.path.isfile(pjson):
        try:
            with open(pjson, "r", encoding="utf-8") as f:
                title = json.load(f).get("title", wid)
        except Exception:
            pass

    work_dir = os.path.join(out_base, f"work_{wid}")
    out_dir = os.path.join(out_base, f"out_{wid}")
    if os.path.isdir(work_dir):
        shutil.rmtree(work_dir, ignore_errors=True)
    os.makedirs(work_dir, exist_ok=True)
    os.makedirs(out_dir, exist_ok=True)

    # 1. Unpack PKG archives
    pkg_files = [f for f in os.listdir(wp_path) if f.endswith(".pkg")]
    for pkg in pkg_files:
        try:
            unpack_pkg(os.path.join(wp_path, pkg), work_dir)
        except Exception as e:
            pass

    # Copy project.json if missing
    pjson_dst = os.path.join(work_dir, "project.json")
    if os.path.isfile(pjson) and not os.path.isfile(pjson_dst):
        try: shutil.copyfile(pjson, pjson_dst)
        except Exception: pass

    # 2. Generate Scene HTML
    gen_ok = generate_scene_html(work_dir)
    if not gen_ok:
        shutil.rmtree(work_dir, ignore_errors=True)
        return {
            "id": wid,
            "title": title,
            "final_score": 0.0,
            "error": "Scene generator failed to compile WebGL HTML"
        }

    # 3. Find preview asset
    preview_src = os.path.join(wp_path, "preview.jpg")
    if not os.path.isfile(preview_src):
        preview_src = os.path.join(wp_path, "preview.gif")

    # 4. Run Playwright WebGL Harness
    harness_path = os.path.join(os.path.dirname(__file__), "harness.js")
    cmd = [
        "node", harness_path,
        work_dir,
        preview_src if os.path.isfile(preview_src) else "",
        "--out", out_dir,
        "--fast"
    ]

    harness_passed = False
    try:
        proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=25)
        harness_passed = (proc.returncode == 0)
    except Exception as e:
        print(f"Harness error for {wid}: {e}", file=sys.stderr)

    # 5. Evaluate Visual Metrics & Score
    rep_file = os.path.join(out_dir, "report.json")
    rep_data = {}
    if os.path.isfile(rep_file):
        try:
            with open(rep_file, "r", encoding="utf-8") as f:
                rep_data = json.load(f)
        except Exception:
            pass

    frame0_path = os.path.join(out_dir, "frame_0.png")
    frame2_path = os.path.join(out_dir, "frame_1p5.png")
    if not os.path.isfile(frame2_path):
        frame2_path = os.path.join(out_dir, "frame_1.5.png")
    if not os.path.isfile(frame2_path):
        frame2_path = os.path.join(out_dir, "frame_2.png")
    frame5_path = os.path.join(out_dir, "frame_3.png")
    if not os.path.isfile(frame5_path):
        frame5_path = os.path.join(out_dir, "frame_5.png")
    if not os.path.isfile(frame0_path):
        frame0_path = os.path.join(out_dir, "frame_initial.png")

    checks_passed = rep_data.get("checksPassed", 0)
    checks_total = rep_data.get("checksTotal", 0)
    if checks_total > 0:
        stability_score = (checks_passed / checks_total) * 100.0
    else:
        stability_score = 100.0 if harness_passed else 50.0

    color_score = 0.0
    struct_score = 0.0
    motion_score = 0.0
    artifact_penalty = 0.0

    if os.path.isfile(frame0_path):
        rend_img = Image.open(frame0_path)
        artifact_penalty = detect_artifact_penalties(rend_img)

        if os.path.isfile(preview_src):
            try:
                prev_img = Image.open(preview_src)
                color_score = calc_color_histogram_similarity(rend_img, prev_img)
                struct_score = calc_structural_gradient_similarity(rend_img, prev_img)
            except Exception as e:
                print(f"Preview comparison error: {e}", file=sys.stderr)
        else:
            color_score = 80.0
            struct_score = 80.0

    motion_score = calc_motion_dynamics(frame0_path, frame2_path, frame5_path)

    # Weighted Composite Score (1 - 100)
    raw_score = (
        0.40 * color_score +
        0.20 * struct_score +
        0.30 * motion_score +
        0.10 * stability_score -
        artifact_penalty
    )
    final_score = float(np.clip(raw_score, 1.0, 100.0))

    res = {
        "id": wid,
        "title": title,
        "final_score": final_score,
        "color_score": color_score,
        "struct_score": struct_score,
        "motion_score": motion_score,
        "stability_score": stability_score,
        "artifact_penalty": artifact_penalty,
        "harness_checks_passed": rep_data.get("allChecksPassed", False),
        "frame_captured": os.path.isfile(frame0_path),
        "preview_found": os.path.isfile(preview_src)
    }

    # Generate visual comparison collage
    collage_path = os.path.join(out_dir, "comparison.png")
    create_comparison_collage(
        preview_src if os.path.isfile(preview_src) else None,
        frame0_path if os.path.isfile(frame0_path) else None,
        res,
        collage_path
    )
    res["collage_path"] = collage_path

    # Cleanup temporary work directory
    # shutil.rmtree(work_dir, ignore_errors=True)
    return res


def main():
    parser = argparse.ArgumentParser(description="HyprWpE Visual Inspector & 1-100 Quality Scoring Runner")
    parser.add_argument("--id", type=str, help="Single wallpaper ID to inspect")
    parser.add_argument("--all", action="store_true", help="Inspect all Workshop scene wallpapers")
    parser.add_argument("--limit", type=int, default=10, help="Max wallpapers to inspect when --all is set")
    parser.add_argument("--out", type=str, default=DEFAULT_OUT, help="Output directory for reports & collages")
    args = parser.parse_args()

    if args.id:
        print(f"=== Running Visual Inspection on Wallpaper {args.id} ===")
        report = inspect_wallpaper(args.id, out_base=args.out)
        print(json.dumps(report, indent=2))
        print(f"\nFinal Score: {report.get('final_score', 0):.1f} / 100")
        print(f"Comparison Collage saved to: {report.get('collage_path')}")
        return

    if args.all:
        if not os.path.isdir(STEAM_WORKSHOP):
            print(f"Error: Steam Workshop dir not found at {STEAM_WORKSHOP}")
            sys.exit(1)

        wallpapers = []
        for item in os.listdir(STEAM_WORKSHOP):
            wp_path = os.path.join(STEAM_WORKSHOP, item)
            if os.path.isdir(wp_path):
                if any(f.endswith(".pkg") for f in os.listdir(wp_path)) or os.path.isfile(os.path.join(wp_path, "scene.pkg")):
                    wallpapers.append(item)

        if args.limit > 0:
            wallpapers = wallpapers[:args.limit]

        print(f"=== Running Visual Inspection on {len(wallpapers)} Wallpapers ===")
        print(f"{'ID':<12} | {'Score':<8} | {'Color':<7} | {'Motion':<7} | {'Title'}")
        print("-" * 65)

        results = []
        for wid in wallpapers:
            rep = inspect_wallpaper(wid, out_base=args.out)
            results.append(rep)
            sc = rep.get("final_score", 0.0)
            col = rep.get("color_score", 0.0)
            mot = rep.get("motion_score", 0.0)
            tit = rep.get("title", wid)[:25]
            print(f"{wid:<12} | {sc:<8.1f} | {col:<7.1f} | {mot:<7.1f} | {tit}")

        summary_file = os.path.join(args.out, "visual_scorecard.json")
        os.makedirs(args.out, exist_ok=True)
        with open(summary_file, "w", encoding="utf-8") as f:
            json.dump(results, f, indent=2)

        print(f"\nVisual Scorecard saved to {summary_file}")
        return

    parser.print_help()


if __name__ == "__main__":
    main()
