# HyprWpE Agent Instructions & Engineering Principles

## 1. Engine Fidelity & Anti-Hardcoding Policy
- **Never Hardcode Wallpaper-Specific Workarounds:**
  Do not introduce checks matching specific wallpaper titles, Steam Workshop IDs, asset filenames (e.g. `emberglow`, `Main BGR`), layer IDs, or arbitrary magic numbers tailored to a single scene.
- **Engine-Level Generalizations Only:**
  Every fix or feature must be implemented at the core engine level following the official Wallpaper Engine specification and WebGL2 standards. If a particle system or shader looks incorrect, find and resolve the mathematical, architectural, or specification flaw in the engine simulation (e.g., coordinate transformations, lifetime normalization, blending, or UV mapping) so that all wallpapers benefit.
- **No Regressions Across Wallpapers:**
  A change that fixes one wallpaper must never break, distort, or degrade another. Always verify that changes hold across diverse wallpaper categories (2D scenes, particle simulations, sprite sheets, video layers, interactive effects).

## 2. Minimal & Disciplined Changes (Ponytail Senior Dev Mode)
- **Smallest Complete Change:** Touch only the code necessary to solve the root cause. Avoid wide churn or modifying unrelated files.
- **Root Cause Resolution:** When an asset behaves incorrectly, trace the data flow from `scene.json` and preset JSON definitions through `scene_generator.py` and `scene_template.html` to find why the standard behavior diverged.
- **Zero-Emoji Policy:** Strictly enforce the machine-wide zero-emoji rule across all comments, docstrings, commit messages, and logs.
