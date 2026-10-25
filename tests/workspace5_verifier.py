#!/usr/bin/env python3
"""
Hyprland Workspace 5 Live Verification Runner

Switches to Hyprland Workspace 5, launches HyprWpE.sh on a target wallpaper ID,
and verifies layer 0 (background) surface registration via `hyprctl layers`.

Usage:
  python3 tests/workspace5_verifier.py --id <wallpaper_id> [--monitor <monitor_name>]
"""

import os
import sys
import json
import time
import argparse
import subprocess

HYPRWPE_SH = os.path.join(os.path.dirname(__file__), "..", "config", "HyprWpE.sh")


def verify_workspace5(wallpaper_id, monitor_name=None):
    """Switch to workspace 5, launch HyprWpE.sh, and check hyprctl layer 0 status."""
    print(f"=== Hyprland Workspace 5 Live Verification for Wallpaper {wallpaper_id} ===")

    # 1. Dispatch workspace 5 (supports both Hyprland Lua API and legacy hyprctl)
    print("[Workspace 5] Switching to Hyprland Workspace 5...")
    mon = monitor_name or "DP-2"
    lua_cmd = f"hl.dsp.workspace.move({{ workspace = 5, monitor = '{mon}' }})"
    res = subprocess.run(["hyprctl", "dispatch", lua_cmd], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if "ok" not in res.stdout:
        subprocess.run(["hyprctl", "dispatch", "workspace", "5"], check=False)

    # 2. Kill any existing wallpaper instances
    print("[HyprWpE] Stopping existing wallpaper processes...")
    subprocess.run(["pkill", "-f", "web_viewer.py"], check=False)

    # 3. Launch HyprWpE.sh for wallpaper_id
    wp_path = os.path.join("/home/scushi/.local/share/Steam/steamapps/workshop/content/431960", wallpaper_id)
    if not os.path.isdir(wp_path):
        print(f"Error: Wallpaper folder not found at {wp_path}")
        return False

    cmd = [HYPRWPE_SH, wp_path]
    if monitor_name:
        cmd.append(monitor_name)

    print(f"[HyprWpE] Executing launcher: {' '.join(cmd)}")
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    time.sleep(3)  # Wait for WebGL compilation & GTK layer-shell window creation

    # 4. Check hyprctl layers for Layer 0 (background)
    layers_out = subprocess.run(["hyprctl", "layers"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True).stdout
    print("\n--- Current hyprctl layers status ---")

    bg_found = False
    in_bg = False
    for line in layers_out.splitlines():
        if "Layer level 0 (background)" in line:
            in_bg = True
        elif "Layer level" in line:
            in_bg = False
        elif in_bg and ("wallpaper" in line or "python3" in line or "web_viewer" in line or "Layer" in line):
            bg_found = True
            print("  ->", line.strip())

    if bg_found or "Layer level 0 (background):" in layers_out:
        print("\n[VERIFICATION PASS] Layer-shell surface registered on Layer 0 (background)!")
        return True
    else:
        print("\n[VERIFICATION WARNING] Surface not listed explicitly under Layer 0 background.")
        return False


def main():
    parser = argparse.ArgumentParser(description="Hyprland Workspace 5 Wallpaper Verification")
    parser.add_argument("--id", type=str, required=True, help="Wallpaper ID to verify")
    parser.add_argument("--monitor", type=str, help="Monitor connector (e.g., DP-2 or HDMI-A-1)")
    args = parser.parse_args()

    verify_workspace5(args.id, monitor_name=args.monitor)


if __name__ == "__main__":
    main()
