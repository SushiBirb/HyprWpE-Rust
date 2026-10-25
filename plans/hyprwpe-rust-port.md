# Blueprint Plan: HyprWpE Python to Rust Port

Objective: Complete port of HyprWpE from Python 3.10 (GTK4/PyGObject) to idiomatic Rust, achieving 100% feature parity and render parity.

---

## Architecture Overview

```
HyprWpE-Rust Workspace
├── Cargo.toml                          # Workspace root
├── crates/
│   ├── hyprwpe-pkg/                    # PKG unpacker + TEX image decoder
│   ├── hyprwpe-shader/                 # HLSL/GLSL -> WebGL ES 3.0 preprocessor
│   ├── hyprwpe-scene/                  # scene.json parser + WebGL HTML generator
│   ├── hyprwpe-core/                   # Data models, config YAML, monitor IPC, subprocess manager
│   ├── hyprwpe-web/                    # WebKitGTK + Gtk4LayerShell web/scene wallpaper runner
│   └── hyprwpe-gui/                    # GTK4 control center GUI application
└── tests/                              # E2E integration tests & Playwright WebGL parity harness
```

---

## Step Dependency Graph

```
[Step 1: Workspace & hyprwpe-pkg] ──┐
                                     ├──> [Step 3: hyprwpe-scene] ──┐
[Step 2: hyprwpe-shader] ───────────┘                              │
                                                                   ├──> [Step 5: hyprwpe-web] ──┐
[Step 4: hyprwpe-core] ───────────────────────────────────────────┘                            ├──> [Step 7: E2E Parity & Optimization]
                                                                                                │
[Step 6: hyprwpe-gui] ──────────────────────────────────────────────────────────────────────────┘
```

Parallel Execution Opportunities:
- Step 1 (PKG/TEX) and Step 2 (Shader Preprocessor) can run in parallel.
- Step 4 (Core/Monitor/Config) can run in parallel with Step 3 (Scene Generator).

---

## Execution Steps

### Step 1: Workspace Cargo Setup & `hyprwpe-pkg` Crate
- **Context Brief**: Port `config/walleng_pkg/core.py`, `config/walleng_pkg/tex.py`, `config/unpacker.py`. Parse PKGv1/v2 binary archives, decompress LZ4 payload, decode `.tex` textures (RLE, DXT1/3/5, BC5, BC7) to standard PNG/JPG files.
- **Dependencies**: None.
- **Model Tier**: Default (Sonnet).
- **Target Files**:
  - `Cargo.toml`
  - `crates/hyprwpe-pkg/Cargo.toml`
  - `crates/hyprwpe-pkg/src/lib.rs`
  - `crates/hyprwpe-pkg/src/pkg.rs`
  - `crates/hyprwpe-pkg/src/tex.rs`
  - `crates/hyprwpe-pkg/src/lz4.rs`
  - `crates/hyprwpe-pkg/src/bin/unpacker.rs`
- **Tasks**:
  1. Create Cargo workspace definition and `hyprwpe-pkg` crate.
  2. Implement binary PKG header and file entry reader (`nom` or `byteorder`).
  3. Implement LZ4 decompression block reader (`lz4_flex`).
  4. Implement `.tex` format parser and texture format converters (RLE, raw RGBA, DXT/BC using `image` / `image-tex` crates).
  5. Build CLI binary `hyprwpe-pkg-unpack`.
- **Verification Commands**:
  - `cargo test -p hyprwpe-pkg`
  - `cargo run -p hyprwpe-pkg --bin unpacker -- <test_pkg_path> /tmp/test_unpack_out`
- **Exit Criteria**:
  - 100% binary unpack and texture extraction match with Python `unpacker.py` on test `.pkg` files.

---

### Step 2: `hyprwpe-shader` GLSL/HLSL Preprocessor Crate
- **Context Brief**: Port `config/glsl_preprocess.py` and `config/shaders/`. Convert Wallpaper Engine HLSL/GLSL shaders into WebGL ES 3.0 shaders (`#version 300 es`). Resolve `#include`, parse `#define COMBO_` and `// [PROPERTY_...]` metadata comments, replace reserved ES keywords.
- **Dependencies**: None (Can run parallel with Step 1).
- **Model Tier**: Default (Sonnet).
- **Target Files**:
  - `crates/hyprwpe-shader/Cargo.toml`
  - `crates/hyprwpe-shader/src/lib.rs`
  - `crates/hyprwpe-shader/src/includes.rs`
  - `crates/hyprwpe-shader/src/combos.rs`
  - `crates/hyprwpe-shader/src/es300.rs`
  - `crates/hyprwpe-shader/src/bin/glsl_preprocess.rs`
- **Tasks**:
  1. Implement `#include` resolution for shader headers (`common.h`, `common_fragment.h`, `common_blending.h`).
  2. Implement combo variable and uniform property comment parser.
  3. Implement AST/regex transformation to ES 3.0 (`in`/`out`, `texture()`, keyword renaming).
  4. Build CLI binary `hyprwpe-glsl-preprocess`.
- **Verification Commands**:
  - `cargo test -p hyprwpe-shader`
  - `cargo run -p hyprwpe-shader --bin glsl_preprocess -- config/shaders/common.h`
- **Exit Criteria**:
  - Generated shader strings match Python `glsl_preprocess.py` output line-for-line or AST-equivalent.

---

### Step 3: `hyprwpe-scene` WebGL HTML Compiler Crate
- **Context Brief**: Port `config/scene_generator.py` and `config/scene_template.html`. Parse `scene.json` (layers, particle systems, 2D/3D objects, animations, material properties). Use `hyprwpe-pkg` and `hyprwpe-shader` to extract assets and preprocess shaders, generating runnable WebGL `index.html`.
- **Dependencies**: Step 1, Step 2.
- **Model Tier**: Strongest (Opus/Sonnet).
- **Target Files**:
  - `crates/hyprwpe-scene/Cargo.toml`
  - `crates/hyprwpe-scene/src/lib.rs`
  - `crates/hyprwpe-scene/src/scene_json.rs`
  - `crates/hyprwpe-scene/src/particle.rs`
  - `crates/hyprwpe-scene/src/compiler.rs`
  - `crates/hyprwpe-scene/templates/scene_template.html`
- **Tasks**:
  1. Define Serde structs for `scene.json` schema (layers, objects, particles, vectors, blend modes).
  2. Implement texture and material path resolution logic.
  3. Implement particle effect builder and shader program assembly.
  4. Inject preprocessed GLSL shaders and JSON configurations into template.
- **Verification Commands**:
  - `cargo test -p hyprwpe-scene`
  - `./tests/run_scene_test.sh <wallpaper_id>` (verifying HTML generated by Rust compiler)
- **Exit Criteria**:
  - Generated `index.html` passes Playwright WebGL rendering test with 0 visual discrepancies vs Python generated scene.

---

### Step 4: `hyprwpe-core` Data Models, Config & Monitor IPC Crate
- **Context Brief**: Port `data/models.py`, `data/wallpaper_data.py`, `config/config_manager.py`, `managers/monitor_manager.py`, and `config/HyprWpE.sh` execution logic.
- **Dependencies**: None (Can run parallel with Step 3).
- **Model Tier**: Default (Sonnet).
- **Target Files**:
  - `crates/hyprwpe-core/Cargo.toml`
  - `crates/hyprwpe-core/src/lib.rs`
  - `crates/hyprwpe-core/src/models.rs`
  - `crates/hyprwpe-core/src/config.rs`
  - `crates/hyprwpe-core/src/monitors.rs`
  - `crates/hyprwpe-core/src/process.rs`
  - `crates/hyprwpe-core/src/data_manager.rs`
- **Tasks**:
  1. Define Rust types for `Wallpaper`, `WallpaperType`, `ScaleMode`, `PanelMargins`, `WallpaperProperties`.
  2. Implement YAML config loading/saving (`~/.config/hyprwpe/config.yaml`).
  3. Implement Hyprland IPC monitor detection via `hyprctl monitors -j` (with `wlr-randr` fallback).
  4. Implement process lifecycle manager spawning and tracking `mpvpaper` / `hyprwpe-web` processes per monitor.
- **Verification Commands**:
  - `cargo test -p hyprwpe-core`
- **Exit Criteria**:
  - Monitor detection correctly parses live `hyprctl` JSON output.
  - Config serialization round-trips with 100% fidelity.

---

### Step 5: `hyprwpe-web` WebKitGTK LayerShell Runner Crate
- **Context Brief**: Port `config/web_viewer.py`. Spawns `WebKitGTK` web view inside `gtk4-layer-shell` desktop background window. Serves local assets via Axum/Warp HTTP server. Handles real-time JS property injection (`window.wallpaperPropertyListener`).
- **Dependencies**: Step 3, Step 4.
- **Model Tier**: Default (Sonnet).
- **Target Files**:
  - `crates/hyprwpe-web/Cargo.toml`
  - `crates/hyprwpe-web/src/main.rs`
  - `crates/hyprwpe-web/src/server.rs`
  - `crates/hyprwpe-web/src/window.rs`
  - `crates/hyprwpe-web/src/ipc.rs`
- **Tasks**:
  1. Build local embedded async HTTP server for serving static wallpaper directories.
  2. Implement GTK4 window initialization with `gtk4-layer-shell` (`Layer::Background`, full monitor anchor).
  3. Embed `WebKitWebView` loading wallpaper URI.
  4. Implement IPC/JS injection for updating wallpaper properties dynamically.
- **Verification Commands**:
  - `cargo test -p hyprwpe-web`
  - `cargo run -p hyprwpe-web -- <wallpaper_dir> <monitor_name>`
- **Exit Criteria**:
  - Web wallpaper renders on background layer without visual artifacts or memory leaks.

---

### Step 6: `hyprwpe-gui` Control Center Application
- **Context Brief**: Port `gui.py`, `ui/ui_builder.py`, `ui/components.py`, `ui/dialogs.py`, `style.css`. GTK4 application for browsing Steam Workshop wallpapers, setting per-monitor wallpapers, and tweaking properties.
- **Dependencies**: Step 4.
- **Model Tier**: Strongest (Opus/Sonnet).
- **Target Files**:
  - `crates/hyprwpe-gui/Cargo.toml`
  - `crates/hyprwpe-gui/src/main.rs`
  - `crates/hyprwpe-gui/src/app.rs`
  - `crates/hyprwpe-gui/src/widgets/wallpaper_card.rs`
  - `crates/hyprwpe-gui/src/widgets/monitor_selector.rs`
  - `crates/hyprwpe-gui/src/widgets/property_editors.rs`
  - `crates/hyprwpe-gui/src/dialogs/unpack_dialog.rs`
  - `crates/hyprwpe-gui/src/dialogs/settings_dialog.rs`
- **Tasks**:
  1. Build GTK4 Application framework and main window layout.
  2. Implement async wallpaper grid loading with thumbnail caching.
  3. Implement search filter and wallpaper category filter bars.
  4. Implement dynamic Property Editor widgets (color pickers, sliders, checkboxes, combo boxes).
  5. Implement Unpack Dialog with progress bar for PKG extraction.
  6. Apply CSS styles (`style.css`).
- **Verification Commands**:
  - `cargo test -p hyprwpe-gui`
  - `cargo run -p hyprwpe-gui`
- **Exit Criteria**:
  - Complete GUI functionality matching Python GTK4 app with zero crashes and fast startup (<200ms).

---

### Step 7: E2E Integration, Visual Parity & Performance Benchmarks
- **Context Brief**: End-to-end integration testing, visual render parity audit, and performance profiling across all wallpaper types (Video, Web, 2D/3D Scene).
- **Dependencies**: Step 1..Step 6.
- **Model Tier**: Strongest (Opus).
- **Target Files**:
  - `tests/e2e_test.rs`
  - `tests/batch_scene_test.py`
- **Tasks**:
  1. Run full Playwright WebGL rendering test suite across batch workshop wallpapers.
  2. Verify 100% visual frame match against Python baseline outputs (`tests/output/`).
  3. Measure memory footprint and CPU utilization (target: >50% reduction vs Python).
  4. Final documentation update (`README.md`, `CLAUDE.md`).
- **Verification Commands**:
  - `cargo test --workspace`
  - `python3 tests/batch_scene_test.py`
- **Exit Criteria**:
  - All unit, integration, and WebGL Playwright tests pass green.
  - Zero Python runtime dependencies remaining.

---

## Adversarial Review Checklist

- [x] **Completeness**: All Python files (`gui.py`, `managers/`, `config/`, `data/`, `ui/`) mapped to Rust crates.
- [x] **Dependency Edges**: Clear DAG order without cycles. Steps 1 & 2 can run concurrently.
- [x] **Render Parity**: Step 3 and Step 7 explicitly mandate Playwright WebGL frame-by-frame verification.
- [x] **Cold Start Readiness**: Each step defines target files, context brief, tasks, verification commands, exit criteria.
- [x] **Rollback Strategy**: Git commit per step milestone allows reverting isolated crates if needed.
