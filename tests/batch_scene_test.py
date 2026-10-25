#!/usr/bin/env python3
"""
Batch Scene Test Runner for HyprWpE

Scans Steam Workshop scene wallpapers, unpacks PKGs, compiles WebGL index.html,
and runs the Playwright test harness across all scenes to identify failures and
verify rendering fixes.

Usage:
    python3 tests/batch_scene_test.py [--limit N] [--workers W]
"""
import os
import sys
import json
import time
import shutil
import subprocess
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed

TESTS_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(TESTS_DIR)
CONFIG_DIR = os.path.join(PROJECT_ROOT, "config")
STEAM_WORKSHOP = "/home/scushi/.local/share/Steam/steamapps/workshop/content/431960"

sys.path.insert(0, CONFIG_DIR)

try:
    from scene_generator import generate_scene_html
    from unpacker import unpack_pkg
except ImportError as e:
    print(f"Failed to import HyprWpE config modules: {e}")
    sys.exit(1)


def discover_scene_wallpapers(base_dir):
    """Find all scene wallpaper directories in Steam workshop."""
    scenes = []
    if not os.path.isdir(base_dir):
        return scenes

    for wid in sorted(os.listdir(base_dir)):
        pjson = os.path.join(base_dir, wid, "project.json")
        if os.path.isfile(pjson):
            try:
                with open(pjson, "r", encoding="utf-8") as f:
                    data = json.load(f)
                if data.get("type", "").lower() == "scene":
                    scenes.append({
                        "id": wid,
                        "title": data.get("title", "Untitled"),
                        "file": data.get("file", "scene.json"),
                        "src_dir": os.path.join(base_dir, wid)
                    })
            except Exception:
                pass
    return scenes


def test_single_scene(scene_info, out_base_dir):
    """Unpacks, generates WebGL HTML, and runs harness.js on a single scene wallpaper."""
    wid = scene_info["id"]
    src_dir = scene_info["src_dir"]
    work_dir = os.path.join("/tmp/HyprWpE_batch", wid)
    out_dir = os.path.join(out_base_dir, wid)

    os.makedirs(work_dir, exist_ok=True)
    os.makedirs(out_dir, exist_ok=True)

    result = {
        "id": wid,
        "title": scene_info["title"],
        "unpacked": False,
        "generated": False,
        "harness_ran": False,
        "passed": False,
        "checks": [],
        "errors": []
    }

    try:
        # 1. Unpack PKG archives if present
        pkg_files = [f for f in os.listdir(src_dir) if f.endswith(".pkg")]
        if pkg_files:
            for pkg in pkg_files:
                pkg_path = os.path.join(src_dir, pkg)
                try:
                    unpack_pkg(pkg_path, work_dir)
                except Exception as e:
                    result["errors"].append(f"unpack_error: {e}")
        else:
            # Copy contents directly if unpacked
            for item in os.listdir(src_dir):
                s = os.path.join(src_dir, item)
                d = os.path.join(work_dir, item)
                if os.path.isdir(s):
                    shutil.copytree(s, d, dirs_exist_ok=True)
                else:
                    shutil.copyfile(s, d)

        # Copy project.json if outside PKG
        pjson_src = os.path.join(src_dir, "project.json")
        pjson_dst = os.path.join(work_dir, "project.json")
        if os.path.isfile(pjson_src) and not os.path.isfile(pjson_dst):
            shutil.copyfile(pjson_src, pjson_dst)

        result["unpacked"] = True

        # 2. Generate Scene HTML
        gen_ok = generate_scene_html(work_dir)
        result["generated"] = gen_ok
        if not gen_ok:
            result["errors"].append("scene_generator_failed")
            return result

        # 3. Run Playwright WebGL Harness
        harness_path = os.path.join(TESTS_DIR, "harness.js")
        preview_src = os.path.join(src_dir, "preview.gif")
        if not os.path.isfile(preview_src):
            preview_src = os.path.join(src_dir, "preview.jpg")

        cmd = [
            "node", harness_path,
            work_dir,
            preview_src if os.path.isfile(preview_src) else "",
            "--out", out_dir
        ]

        proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=120)
        result["harness_ran"] = True

        report_json = os.path.join(out_dir, "report.json")
        if os.path.isfile(report_json):
            with open(report_json, "r", encoding="utf-8") as f:
                rep = json.load(f)
            result["passed"] = rep.get("allChecksPassed", False)
            result["checks"] = rep.get("checks", [])
            result["errors"].extend(rep.get("errors", []))
        else:
            result["errors"].append(f"harness_no_report: {proc.stderr[:200]}")

    except subprocess.TimeoutExpired:
        result["errors"].append("harness_timeout_45s")
    except Exception as e:
        result["errors"].append(f"exception: {e}")
    finally:
        # Cleanup temporary work directory
        shutil.rmtree(work_dir, ignore_errors=True)

    return result


def main():
    parser = argparse.ArgumentParser(description="Batch Scene Wallpaper Test Runner")
    parser.add_argument("--limit", type=int, default=0, help="Limit number of scenes to test (0=all)")
    parser.add_argument("--workers", type=int, default=4, help="Parallel worker threads")
    args = parser.parse_args()

    scenes = discover_scene_wallpapers(STEAM_WORKSHOP)
    if args.limit > 0:
        scenes = scenes[:args.limit]

    out_base_dir = os.path.join(TESTS_DIR, "output", "batch")
    os.makedirs(out_base_dir, exist_ok=True)

    print(f"=== Starting Batch Scene Testing ({len(scenes)} scenes, {args.workers} workers) ===")
    start_time = time.time()

    results = []
    passed_count = 0

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = {executor.submit(test_single_scene, sc, out_base_dir): sc for sc in scenes}
        for i, future in enumerate(as_completed(futures), 1):
            res = future.result()
            results.append(res)
            status = "PASS" if res["passed"] else "FAIL"
            if res["passed"]:
                passed_count += 1
            print(f"[{i}/{len(scenes)}] [{status}] ID: {res['id']} | Title: {res['title']}")
            if not res["passed"] and res["errors"]:
                print(f"       Errors: {res['errors'][:2]}")

    elapsed = time.time() - start_time
    print("\n==================================================")
    print(f"BATCH TEST COMPLETE in {elapsed:.1f}s")
    print(f"Total Tested: {len(scenes)}")
    print(f"Passed:       {passed_count}")
    print(f"Failed:       {len(scenes) - passed_count}")
    print(f"Pass Rate:    {(passed_count / len(scenes) * 100) if scenes else 0:.1f}%")
    print("==================================================")

    batch_summary = {
        "total": len(scenes),
        "passed": passed_count,
        "failed": len(scenes) - passed_count,
        "elapsed_seconds": elapsed,
        "results": results
    }

    report_path = os.path.join(out_base_dir, "batch_report.json")
    with open(report_path, "w", encoding="utf-8") as f:
        json.dump(batch_summary, f, indent=2)
    print(f"Report saved -> {report_path}")


if __name__ == "__main__":
    main()
