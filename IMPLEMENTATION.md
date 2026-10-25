# HyprWpE: Pure Rust Architectural Mapping & Implementation Dynamics

This document details the architectural specification and subsystem mapping of the pure-Rust port **HyprWpE** with visual and functional parity to Wallpaper Engine on Arch Linux / Wayland (Hyprland).

## 1. High-Level Workspace Architecture

HyprWpE is structured as a modular Rust workspace (`Cargo.toml`) across 7 specialized crates:

| Crate | Purpose & Role | Key Technologies |
| :--- | :--- | :--- |
| **`hyprwpe-core`** | Configuration management, properties model, monitor state, Steam Workshop catalog indexer | `serde`, `serde_yaml`, `serde_json`, `walkdir` |
| **`hyprwpe-unpacker`** | Pure-Rust `.pkg` archive extraction and proprietary `.tex` texture decoder (DXT1/BC1, DXT5/BC3, RGBA8, LZ4 decompression) | `lz4_flex`, `byteorder`, `image`, `rayon` |
| **`hyprwpe-scene`** | Wallpaper Engine scene graph compiler, shader transpiler (SM30/HLSL to GLSL ES 300), effect pass resolution, and WebGL2 HTML generation | `regex`, `serde_json`, pure-Rust GLSL transpiler |
| **`hyprwpe-runner`** | Multi-backend wallpaper process lifecycle supervisor, local HTTP streaming daemon, and monitor dispatch | `tiny_http`, `nix`, `mpvpaper`, `hyprwpe-web` |
| **`hyprwpe-gui`** | Native Wayland GTK4 wallpaper browser, monitor switcher, properties sidebar, margin configuration, and filter controls | `gtk4`, `gtk4-layer-shell`, `glib`, `gio` |
| **`hyprwpe-web`** | Standalone GTK4 + WebKitGTK background layer window implementing Wallpaper Engine Web Wallpaper API | `gtk4`, `gtk4-layer-shell`, `webkit6` |
| **`hyprwpe-eval`** | Multi-engine visual inspection and automated scoring suite (color histograms, structural gradients, motion dynamics, artifact detection) | `clap`, `rayon`, `image`, Playwright harness |

## 2. Rendering Engines by Wallpaper Type

Wallpaper Engine handles different wallpaper types by dispatching them to specialized internal engines. HyprWpE replicates this through pure-Rust runners:

### A. Video Wallpapers
- **Original Engine:** Windows Media Foundation / DirectX video decoder.
- **HyprWpE:** Uses **`mpvpaper`**, an efficient Wayland background video player. Settings from the GUI (speed, mute, scaling mode: cover/fill/contain) are applied natively as `mpv` flags (e.g., `--speed=1.0`, `--no-audio`, `--panscan=1.0`).

### B. Web Wallpapers
- **Original Engine:** Embedded Chromium Embedded Framework (CEF) view interacting via `window.wallpaperPropertyListener` and `window.wallpaperRegisterAudioListener`.
- **HyprWpE:** Uses **`hyprwpe-web`**, a dedicated Wayland background surface running `WebKitGTK` attached via `gtk4-layer-shell` to the `Layer::Background` surface. Injects the complete Wallpaper Engine JavaScript runtime API and user properties.

### C. Scene Wallpapers (2D/3D & Effects)
- **Original Engine:** DirectX / HLSL scene graph engine evaluating `scene.json`, particle systems, multi-pass post-processing effects, and audio responsiveness.
- **HyprWpE:** Compiles scenes to standalone WebGL2 applications via `hyprwpe-scene`:
  - **Shader Transpiler:** Transpiles HLSL shaders, `#include` headers (`common_composite.h`, `common_blur.h`, etc.), macros, and HLSL combos to GLSL ES 300.
  - **Multi-Pass Effects:** Multi-pass ping-pong FBO execution with straight-alpha blending (`gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA)`).
  - **Input Buffering & Prev Slots:** Unbound combine passes and `_rt_*` buffers automatically bind the effect pass input (`src`) to prevent black screen dropouts.
  - **Background Sampling:** Adjustment layers (`copybackground: true`) sample live background buffers using GPU texture copies.

## 3. Visual Parity Verification & Scoring Engine (`hyprwpe-eval`)

Visual quality and feature parity are mechanically verified using the `hyprwpe-eval` suite:
- **Color Similarity:** Normalized histogram intersection against official author preview artwork.
- **Structural Similarity:** Gradient magnitude correlation identifying edges, shapes, and geometry.
- **Motion Dynamics:** Multi-frame temporal variance analysis verifying active animation execution (>90% target).
- **Artifact Detection:** Automated screening for solid color fallbacks, static noise, oscillations, and console errors.
- **Collage Generation:** High-resolution side-by-side diagnostic collages saved to `/tmp/hyprwpe_eval/out_<id>/comparison.png`.
