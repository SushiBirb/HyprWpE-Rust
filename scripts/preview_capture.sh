#!/bin/bash
# Helper script to launch a wallpaper and capture a screenshot for visual inspection.

WALLPAPER_ID="${1:-2548825361}"
MONITOR="${2:-DP-1}"
OUT_FILE="/tmp/hyprwpe_preview.png"

# Detect active monitor if default is not connected
ACTUAL_MONITOR=$(hyprctl monitors -j | jq -r '.[0].name' 2>/dev/null)
if [ -n "$ACTUAL_MONITOR" ] && [ "$ACTUAL_MONITOR" != "null" ]; then
    MONITOR="$ACTUAL_MONITOR"
fi

echo "Launching HyprWpE for wallpaper $WALLPAPER_ID on monitor $MONITOR..."
/home/scushi/HyprWpE/config/HyprWpE.sh "$WALLPAPER_ID" "$MONITOR"

echo "Waiting for wallpaper rendering..."
sleep 3.5

echo "Capturing screenshot of $MONITOR with grim..."
grim -o "$MONITOR" "$OUT_FILE" 2>/dev/null || grim -m "$MONITOR" "$OUT_FILE" 2>/dev/null || grim "$OUT_FILE"

if [ -f "$OUT_FILE" ]; then
    echo "Screenshot saved successfully to $OUT_FILE"
else
    echo "Failed to capture screenshot."
fi
