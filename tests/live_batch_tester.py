#!/usr/bin/env python3
"""
HyprWpE Live Batch Scene Wallpaper Tester & Scorecard Generator

Switches to Hyprland Workspace 5, applies each scene wallpaper live to the desktop,
pauses for visual inspection, runs visual_inspector.py to calculate a 1-100 score,
and logs the real-time scorecard.

Usage:
  python3 tests/live_batch_tester.py [--limit N] [--delay SECS]
"""

import os
import sys
import json
import time
import argparse
import subprocess

STEAM_WORKSHOP = "/home/scushi/.local/share/Steam/steamapps/workshop/content/431960"
HYPRWPE_SH = os.path.join(os.path.dirname(__file__), "..", "config", "HyprWpE.sh")
INSPECTOR = os.path.join(os.path.dirname(__file__), "visual_inspector.py")


def discover_scene_wallpapers(steam_dir):
    scenes = []
    if not os.path.isdir(steam_dir):
        return scenes

    for item in sorted(os.listdir(steam_dir)):
        wp_path = os.path.join(steam_dir, item)
        if os.path.isdir(wp_path):
            pjson = os.path.join(wp_path, "project.json")
            is_scene = False
            title = item

            if os.path.isfile(pjson):
                try:
                    with open(pjson, "r", encoding="utf-8") as f:
                        data = json.load(f)
                        title = data.get("title", item)
                        if data.get("type", "").lower() == "scene":
                            is_scene = True
                except Exception:
                    pass

            if not is_scene:
                # Check for scene.pkg or .tex / .json scene assets inside folder
                if os.path.isfile(os.path.join(wp_path, "scene.pkg")) or os.path.isfile(os.path.join(wp_path, "scene.json")):
                    is_scene = True
                elif any(f.endswith(".pkg") for f in os.listdir(wp_path)):
                    is_scene = True

            if is_scene:
                scenes.append({"id": item, "title": title, "path": wp_path})

    return scenes


def main():
    parser = argparse.ArgumentParser(description="Live Batch Scene Wallpaper Tester for Workspace 5")
    parser.add_argument("--limit", type=int, default=0, help="Limit number of scenes to test (0=all)")
    parser.add_argument("--delay", type=float, default=3.0, help="Delay in seconds per wallpaper on desktop")
    parser.add_argument("--out", type=str, default="/tmp/hyprwpe_inspector", help="Output directory")
    args = parser.parse_args()

    scenes = discover_scene_wallpapers(STEAM_WORKSHOP)
    if not scenes:
        print(f"Error: No scene wallpapers found in {STEAM_WORKSHOP}")
        sys.exit(1)

    if args.limit > 0:
        scenes = scenes[:args.limit]

    print(f"==================================================")
    print(f"   HyprWpE Live Batch Scene Wallpaper Tester      ")
    print(f"==================================================")
    print(f"Total Scenes to Test: {len(scenes)}")
    print(f"Target: Hyprland Workspace 5 (Live Desktop)")
    print(f"Display Delay: {args.delay}s per wallpaper\n")

    # 1. Switch to Workspace 5
    print("[Hyprland] Switching to Workspace 5...")
    mon = "DP-2"
    lua_cmd = f"hl.dsp.workspace.move({{ workspace = 5, monitor = '{mon}' }})"
    res = subprocess.run(["hyprctl", "dispatch", lua_cmd], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if "ok" not in res.stdout:
        subprocess.run(["hyprctl", "dispatch", "workspace", "5"], check=False)

    print(f"\n{'#':<4} | {'ID':<12} | {'Score':<7} | {'Color':<6} | {'Motion':<6} | {'Title'}")
    print("-" * 75)

    results = []
    for idx, sc in enumerate(scenes, 1):
        wid = sc["id"]
        wtitle = sc["title"][:28]

        # Stop previous wallpaper instance
        subprocess.run(["pkill", "-f", "web_viewer.py"], check=False)
        time.sleep(0.3)

        # Launch live wallpaper on Desktop via HyprWpE.sh
        subprocess.Popen([HYPRWPE_SH, sc["path"]], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        # Wait so user can see wallpaper live on Workspace 5
        time.sleep(args.delay)

        # Run visual inspector to compute score & comparison collage
        inspect_proc = subprocess.run(
            ["python3", INSPECTOR, "--id", wid, "--out", args.out],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )

        score = 0.0
        color_sc = 0.0
        motion_sc = 0.0
        report_data = {}

        # Parse inspect output
        try:
            # find JSON in stdout
            lines = inspect_proc.stdout.splitlines()
            json_str = ""
            recording = False
            for line in lines:
                if line.strip().startswith("{"):
                    recording = True
                if recording:
                    json_str += line + "\n"
                if line.strip().startswith("}"):
                    break

            if json_str:
                report_data = json.loads(json_str)
                score = report_data.get("final_score", 0.0)
                color_sc = report_data.get("color_score", 0.0)
                motion_sc = report_data.get("motion_score", 0.0)
        except Exception:
            pass

        record = {
            "index": idx,
            "id": wid,
            "title": sc["title"],
            "score": score,
            "color_score": color_sc,
            "motion_score": motion_sc,
            "details": report_data
        }
        results.append(record)

        score_str = f"{score:.1f}" if score > 0 else "FAIL"
        print(f"[{idx:02d}/{len(scenes)}] | {wid:<12} | {score_str:<7} | {color_sc:<6.1f} | {motion_sc:<6.1f} | {wtitle}")

    # Save summary report
    summary_path = os.path.join(args.out, "live_scorecard.json")
    os.makedirs(args.out, exist_ok=True)
    with open(summary_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)

    print("\n==================================================")
    print(f"BATCH TEST COMPLETE: {len(results)} Wallpapers Evaluated")
    print(f"Live Scorecard saved to: {summary_path}")
    print("==================================================")


if __name__ == "__main__":
    main()
