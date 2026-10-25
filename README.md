# HyprWpE: Native Pure-Rust Wallpaper Engine for Hyprland

A modular, pure-Rust application suite bringing Steam Wallpaper Engine's video, web, and animated 2D/3D scene wallpapers natively to the Hyprland compositor on Wayland.

HyprWpE eliminates all reliance on Wine, Proton, and external C++ binary emulators (such as `linux-wallpaperengine`). By compiling Wallpaper Engine scenes into standalone WebGL2/GLSL ES 300 pipelines and rendering them via native Wayland layer shell surfaces, HyprWpE achieves low resource overhead, high framerates, and exact visual parity with native Wallpaper Engine.

---

## Architecture Overview

HyprWpE is structured as an idiomatic Rust workspace (`Cargo.toml`) composed of 7 specialized crates:

| Crate | Binary / Library | Core Responsibilities & Key Crates |
| :--- | :--- | :--- |
| **[`hyprwpe-core`](crates/hyprwpe-core)** | Library | Data models (`Wallpaper`, `WallpaperType`, `ScaleMode`, `PanelMargins`), YAML configuration persistence (`~/.config/HyprWpE/`), monitor discovery via Hyprland IPC (`hyprctl monitors -j`) with fallback to `wlr-randr`, and Steam Workshop indexer (`walkdir`, `serde`, `serde_yaml`, `serde_json`). |
| **[`hyprwpe-unpacker`](crates/hyprwpe-unpacker)** | `unpacker`, Lib | Pure-Rust PKGv1/v2 archive extraction and `.tex` proprietary texture decoding (DXT1/BC1, DXT5/BC3, RGBA8, RLE) with LZ4 block decompression (`lz4_flex`, `image`, `byteorder`, `rayon`). |
| **[`hyprwpe-scene`](crates/hyprwpe-scene)** | Library | Wallpaper Engine scene graph compiler and GLSL ES 300 shader transpiler. Parses `scene.json`, resolves particle systems, builds multi-pass render graphs (`_rt_*` textures, ping-pong FBOs), transpiles SM30/HLSL/GLSL headers, and generates standalone WebGL2 runtime HTML pages (`regex`, `serde_json`). |
| **[`hyprwpe-runner`](crates/hyprwpe-runner)** | `hyprwpe-runner`, `verify_live_desktop`, Lib | Multi-backend wallpaper process supervisor, embedded zero-config HTTP streaming server (`tiny_http`) with full CORS support, process lifecycle management, lockfiles (`/tmp/hyprwpe-rust-<monitor>.lock`), and PID tracking. |
| **[`hyprwpe-gui`](crates/hyprwpe-gui)** | `hyprwpe-gui` | Native Wayland GTK4 wallpaper management center. Asynchronously indexes installed wallpapers, caches thumbnail previews, provides real-time search and tag filtering, edits wallpaper properties, configures panel margins, and assigns monitors (`gtk4`, `gtk4-layer-shell`, `glib`, `gio`). |
| **[`hyprwpe-web`](crates/hyprwpe-web)** | `hyprwpe-web` | Dedicated Wayland background surface using WebKitGTK attached via `gtk4-layer-shell` to `Layer::Background`. Injects Wallpaper Engine JavaScript APIs (`window.wallpaperPropertyListener`, audio listeners), handles window sizing, and manages smooth crossfading (`gtk4`, `gtk4-layer-shell`, `webkit6`). |
| **[`hyprwpe-eval`](crates/hyprwpe-eval)** | `hyprwpe-eval`, Lib | Automated visual regression, render scoring, and artifact screening engine. Computes normalized color histogram intersections, structural gradient correlation, motion variance (>90% target), detects anomalies (static noise, solid-color fallbacks, oscillating stripes), and outputs side-by-side diagnostic collages (`image`, `rayon`, `clap`). |

---

## Key Features & Completed Improvements

### 1. Pure-Rust Pipeline & Zero External Binary Dependencies
- Replaces legacy Python scripts and external C++ binaries with high-performance native Rust binaries.
- Archive extraction and texture decompression run in parallel via Rayon, reducing wallpaper cold-start time from multiple seconds to sub-200 milliseconds.

### 2. Wallpaper Engine Rendering Parity
- **Video Wallpapers:** Efficient playback using `mpvpaper` on Wayland surfaces, applying user properties (speed, volume, scaling: cover/contain/fill) directly via IPC flags.
- **Web Wallpapers:** Embedded WebKitGTK runtime with two-way property injection, audio listeners, and CORS-enabled local HTTP streaming.
- **Scene Wallpapers (2D/3D):** Transpiles original DirectX/HLSL and GLSL shaders directly to WebGL2 (`#version 300 es`), supporting multi-pass post-processing effects, particle systems, and audio-reactive properties.

### 3. Engine-Level Fidelity (Strict Anti-Hardcoding Standard)
All engine improvements are generalized to match official Wallpaper Engine specifications:
- **Particle System Emitters:** Emitter position translation offsets (`em.origin`) are cleanly decoupled from non-uniform object scale transformations.
- **Particle Alpha Fading:** `alphafade` parameters (`fadeintime`, `fadeouttime`) operate as normalized fractions of particle lifetime ($0.0 \dots 1.0$) rather than raw seconds, with symmetric fallback ($1.0 - \text{fadeintime}$).
- **Random Operators:** `alpharandom` supports non-linear power distribution exponents.
- **Rope & Trail Particles:** Custom geometry shader simulation (`genericropeparticle`) connecting historical position points with smooth UV generation for dynamic ribbon, water, and comet trails.
- **Multi-Pass Post-Processing:** Unbound effect pass inputs (`_rt_*` buffers) automatically link to source passes, preventing black screen dropouts.
- **Adjustment Layers:** Background copying (`copybackground: true`) utilizes GPU texture blitting to sample the live background layer.
- **Alpha Blending:** Exact straight-alpha separate blend equations matching DirectX/OpenGL reference targets (`gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA)`).

### 4. Compositor & Shell Integration
- **Layer Stacking:** Renders on Wayland `Layer::Background` (Layer 0), cooperating seamlessly with desktop shells (such as Ryoku / Quickshell on Layer 1) without flickering or backdrop occlusion.
- **Namespace & Rules:** Surface namespace `hyprwpe-web` configured with Hyprland `animation = "fade"` layer rules for smooth transitions.

---

## Prerequisites

- **Compositor:** Hyprland on Wayland.
- **Steam:** Steam client with Wallpaper Engine purchased and wallpapers downloaded.
- **Rust Toolchain:** Stable Rust (1.80+ / 2021 Edition) with `cargo`.
- **System Libraries (Arch Linux):**
  ```bash
  sudo pacman -S gtk4 gtk4-layer-shell webkitgtk-6.0 mpvpaper lz4 ffmpeg
  ```

---

## Building the Workspace

Build all crates in release mode:

```bash
cargo build --release
```

Run workspace unit and integration tests:

```bash
cargo test --workspace
```

The resulting binaries are located in `target/release/`:
- `hyprwpe-gui`: Main GTK4 graphical control application.
- `hyprwpe-runner`: Background supervisor and CLI launcher.
- `hyprwpe-web`: Wayland background layer WebKitGTK viewer.
- `unpacker`: Standalone PKG archive and TEX image extractor.
- `hyprwpe-eval`: Visual parity and regression testing suite.

---

## Usage Guide

### 1. Graphical Interface (Recommended)

Launch the GTK4 wallpaper browser:

```bash
cargo run --release -p hyprwpe-gui
```

- **Browse & Search:** Instantly filters wallpapers across Video, Web, and Scene types with cached thumbnails.
- **Per-Monitor Setup:** Select a target display (`DP-1`, `HDMI-A-1`) or apply across all monitors.
- **Live Properties:** Adjust playback speed, audio volume, and scaling modes with instant live reload.
- **Panel Margins:** Configure pixel offsets (top, bottom, left, right) to prevent wallpapers from being obscured by status bars.

### 2. Command-Line Runner (CLI)

The CLI supervisor allows launching wallpapers by Workshop ID or local directory path:

```bash
# Launch a wallpaper by Workshop ID on the default monitor
cargo run --release -p hyprwpe-runner --bin hyprwpe-runner -- 2548825361

# Launch on a specific monitor
cargo run --release -p hyprwpe-runner --bin hyprwpe-runner -- 2548825361 DP-1

# Stop wallpapers on a specific monitor
cargo run --release -p hyprwpe-runner --bin hyprwpe-runner -- stop DP-1

# Stop wallpapers across all monitors
cargo run --release -p hyprwpe-runner --bin hyprwpe-runner -- stop
```

### 3. Standalone Web & Scene Viewer

Launch a compiled WebGL HTML file or remote URL directly onto the desktop layer:

```bash
cargo run --release -p hyprwpe-web -- /path/to/scene_index.html DP-1
```

### 4. PKG Archive & TEX Texture Unpacker

Extract assets and decode proprietary texture formats directly from the command line:

```bash
cargo run --release -p hyprwpe-unpacker --bin unpacker -- ~/.steam/steam/steamapps/workshop/content/431960/<id>/scene.pkg /tmp/unpacked_scene
```

### 5. Automated Evaluation & Regression Scoring

Inspect rendering accuracy and benchmark a wallpaper against its official Steam preview artwork:

```bash
cargo run --release -p hyprwpe-eval -- ~/.steam/steam/steamapps/workshop/content/431960/<id>
```

- Produces an automated scoring breakdown (0-100) based on histogram color similarity, edge correlation, and motion dynamics.
- Flags rendering anomalies (static screens, solid color fallbacks, high-frequency noise).
- Saves a high-resolution side-by-side comparison collage to `/tmp/hyprwpe_eval/out_<id>/comparison.png`.

---

## Configuration Files

HyprWpE stores persistent user configurations under `~/.config/HyprWpE/`:

- `wallpapers.yaml`: Maps monitor names to active Wallpaper Engine Workshop IDs.
- `properties.yaml`: Stores per-wallpaper customization settings (speed, volume, scaling) and global panel margins.

---

## License & Disclaimer

This project is a community-driven open-source initiative and is not affiliated with, endorsed by, or associated with Wallpaper Engine or its developers. Steam and Wallpaper Engine are registered trademarks of their respective owners.
