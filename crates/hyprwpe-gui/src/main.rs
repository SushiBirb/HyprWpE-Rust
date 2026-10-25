#![allow(deprecated)]

mod app;
mod components;
mod dialogs;
mod ui_builder;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use hyprwpe_core::{ConfigManager, WallpaperProperties};
use hyprwpe_runner::{
    detect_monitors, kill_legacy_python_instances, set_wallpaper, stop_all_wallpapers,
    stop_wallpaper,
};

#[derive(Parser)]
#[command(name = "hyprwpe")]
#[command(about = "Wallpaper Engine For Hyprland on Wayland (Rust Native)", long_about = None)]
struct Cli {
    #[arg(short, long, value_name = "FILE")]
    load_config: Option<Option<PathBuf>>,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Terminate running legacy Python HyprWpE instances and clean up locks
    #[arg(long)]
    kill_python: bool,

    /// Optional Wallpaper ID to run directly from CLI
    wallpaper_id: Option<String>,

    /// Target monitor name (e.g. DP-1)
    monitor: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Stop wallpapers
    Stop {
        /// Monitor name to stop (stops all if omitted)
        monitor: Option<String>,
    },
    /// Load a saved YAML configuration
    LoadConfig {
        /// Config file path
        path: Option<PathBuf>,
    },
    /// Terminate running legacy Python HyprWpE instances and clean up locks
    KillPython,
}

fn main() {
    let cli = Cli::parse();

    // Terminate legacy Python HyprWpE processes and locks on launch
    kill_legacy_python_instances();

    if cli.kill_python {
        println!("Terminated legacy Python HyprWpE processes and cleaned up lock files.");
        return;
    }

    if let Some(config_opt) = cli.load_config {
        let cm = ConfigManager::new().expect("Failed to initialize ConfigManager");
        let path = config_opt.unwrap_or_else(|| cm.wallpapers_yaml_path());
        println!("Loading wallpaper setup from {}", path.display());
        stop_all_wallpapers();

        if let Ok(config) = cm.load_config() {
            let wp_dir = PathBuf::from(&config.wallpaper_dir);
            for (mon, id) in &config.wallpapers {
                let props = WallpaperProperties::default();
                let _ = set_wallpaper(id, mon, &wp_dir, &props);
            }
        }
        return;
    }

    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Stop { monitor } => {
                if let Some(m) = monitor {
                    println!("Stopping wallpaper on monitor {m}...");
                    stop_wallpaper(&m);
                } else {
                    println!("Stopping all active wallpapers...");
                    stop_all_wallpapers();
                }
            }
            Commands::LoadConfig { path } => {
                let cm = ConfigManager::new().expect("Failed to initialize ConfigManager");
                let config_path = path.unwrap_or_else(|| cm.wallpapers_yaml_path());
                println!("Loading wallpaper setup from {}", config_path.display());
                stop_all_wallpapers();

                if let Ok(config) = cm.load_config() {
                    let wp_dir = PathBuf::from(&config.wallpaper_dir);
                    for (mon, id) in &config.wallpapers {
                        let props = WallpaperProperties::default();
                        let _ = set_wallpaper(id, mon, &wp_dir, &props);
                    }
                }
            }
            Commands::KillPython => {
                println!("Terminating legacy Python HyprWpE processes and cleaning up lock files...");
                kill_legacy_python_instances();
                println!("Legacy Python HyprWpE processes stopped.");
            }
        }
        return;
    }

    if let Some(id) = cli.wallpaper_id {
        let cm = ConfigManager::new().expect("Failed to initialize ConfigManager");
        let config = cm.load_config().unwrap_or_default();
        let wp_dir = PathBuf::from(&config.wallpaper_dir);

        let monitors = detect_monitors();
        let target_monitor = cli.monitor.unwrap_or_else(|| monitors.first().cloned().unwrap_or_else(|| "DP-1".to_string()));

        println!("Applying wallpaper {id} to monitor {target_monitor}...");
        let props = WallpaperProperties::default();
        if let Err(e) = set_wallpaper(&id, &target_monitor, &wp_dir, &props) {
            eprintln!("Error applying wallpaper: {e}");
        }
        return;
    }

    // Default: Launch GTK4 GUI Application
    app::run_app();
}
