use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use hyprwpe_core::WallpaperProperties;
use hyprwpe_runner::set_wallpaper;

fn main() {
    let steam_dir = PathBuf::from("/home/scushi/.local/share/Steam/steamapps/workshop/content/431960");
    let scene_ids = vec![
        ("1157849125", "Gurren Lagann Dual Wallpaper"),
        ("3090802097", "Malevolent Shrine"),
        ("2787873159", "Sleeping Kirby"),
        ("3085606089", "Stray Feline"),
    ];

    let props = WallpaperProperties::default();
    let monitor = "DP-2";

    println!("=== Live Desktop Matrix Verification ===");

    for (id, title) in scene_ids {
        println!("Activating '{}' ({}) on {}...", title, id, monitor);
        match set_wallpaper(id, monitor, &steam_dir, &props) {
            Ok(pid) => {
                println!("Successfully set wallpaper {} (PID {})", id, pid);
                sleep(Duration::from_millis(3000));
                let screenshot_path = format!("/tmp/live_desktop_{id}.png");
                let status = std::process::Command::new("grim")
                    .arg(&screenshot_path)
                    .status();
                if status.is_ok() && status.unwrap().success() {
                    println!("Captured live screenshot: {}", screenshot_path);
                }
            }
            Err(e) => println!("Error setting wallpaper {}: {}", id, e),
        }
    }
}
