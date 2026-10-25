# CLAUDE.md - HyprWpE Guidelines

HyprWpE brings Steam Wallpaper Engine (video, web, 2D/3D scene wallpapers) natively to Hyprland on Wayland using GTK4, WebKitGTK, `mpvpaper`, and `linux-wallpaperengine`.

## Tech Stack
- **Language**: Python 3.10+ (GTK4 / PyGObject), Bash (CLI runner)
- **GUI & Layer Shell**: GTK4 (`gi.repository.Gtk`), `gtk4-layer-shell`
- **Renderers**: `mpvpaper` (video), `webkitgtk-6.0` (web), `linux-wallpaperengine` (scene)
- **Test Harness**: Node.js + Playwright (headless WebGL scene evaluation)

## Code Style & Conventions
- **Naming**: `snake_case` for functions/variables/modules; `PascalCase` for GTK classes & dialogs; `UPPER_CASE` for constants (`config/constants.py`).
- **Imports**: standard library first, `gi` (GTK4/Gdk/LayerShell) next, third-party (`yaml`, `playwright`), project modules (`managers`, `ui`, `config`, `data`).
- **GTK UI**: Keep UI components modular in `ui/components.py` and `ui/dialogs.py`. Main window assembly in `ui/ui_builder.py`.
- **Subprocesses & Error Handling**: Non-blocking asynchronous process spawning when invoking renderers (`mpvpaper`, `linux-wallpaperengine`). Always handle missing Steam workshop assets gracefully.

## Commands
- **Run GUI**: `python gui.py`
- **CLI Runner**: `./config/HyprWpE.sh <id_or_path> [monitor]`
- **Stop Wallpapers**: `./config/HyprWpE.sh stop [monitor]`
- **Run Scene Test**: `./tests/run_scene_test.sh <wallpaper_id_or_dir>`
- **Unpack PKG Archive**: `python config/unpacker.py <pkg_path> <out_dir>`
- **Preprocess GLSL**: `python config/glsl_preprocess.py <shader_file>`

## Project Structure
- `gui.py` - GTK4 main window and application entry point
- `ui/` - GTK4 interface components (`ui_builder.py`, `components.py`, `dialogs.py`)
- `managers/` - System and monitor state management (`monitor_manager.py`)
- `config/` - Core execution logic (`HyprWpE.sh`), scene generator (`scene_generator.py`), shader preprocessor (`glsl_preprocess.py`), PKG unpacker (`unpacker.py`), web viewer (`web_viewer.py`), and configuration persistence (`config_manager.py`)
- `data/` - Dataclasses & models (`wallpaper_data.py`, `models.py`)
- `tests/` - Playwright WebGL evaluation harness (`harness.js`, `run_scene_test.sh`)
- `scripts/` - Helper utilities (`preview_capture.sh`)
