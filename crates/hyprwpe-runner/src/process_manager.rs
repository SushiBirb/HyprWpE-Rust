use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use fs2::FileExt;

pub fn get_pid_file(monitor: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/hyprwpe-rust-{monitor}.pid"))
}

pub fn get_lock_file(monitor: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/hyprwpe-rust-{monitor}.lock"))
}

pub fn write_pid(monitor: &str, pid: u32) {
    let lock_path = get_lock_file(monitor);
    let pid_path = get_pid_file(monitor);

    if let Ok(lock_file) = File::create(&lock_path) {
        let _ = lock_file.lock_exclusive();
        let _ = fs::write(&pid_path, pid.to_string());
        let _ = lock_file.unlock();
    }
}

pub fn read_pid(monitor: &str) -> Option<u32> {
    let lock_path = get_lock_file(monitor);
    let pid_path = get_pid_file(monitor);

    if !pid_path.exists() {
        return None;
    }

    if let Ok(lock_file) = File::open(&lock_path) {
        let _ = lock_file.lock_shared();
        let mut content = String::new();
        if let Ok(mut pid_file) = File::open(&pid_path) {
            let _ = pid_file.read_to_string(&mut content);
        }
        let _ = lock_file.unlock();
        return content.trim().parse::<u32>().ok();
    }

    None
}

/// Terminate any legacy Python HyprWpE processes and clean up locks
pub fn kill_legacy_python_instances() {
    // 1. Clean legacy Python pid and lock files (/tmp/HyprWpE-*)
    if let Ok(entries) = fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("HyprWpE-") && name.ends_with(".pid") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(pid) = content.trim().parse::<u32>() {
                            let is_rust = fs::read_to_string(format!("/proc/{pid}/cmdline"))
                                .map(|c| c.contains("hyprwpe-") || c.contains("HyprWpE-Rust"))
                                .unwrap_or(false);
                            if !is_rust {
                                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
                            }
                        }
                    }
                    let _ = fs::remove_file(&path);
                } else if name.starts_with("HyprWpE-") && name.ends_with(".lock") {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }

    // 2. Targeted pkill for legacy python scripts
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("web_viewer.py").output();
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("HyprWpE/config/HyprWpE.sh").output();
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("HyprWpE.*gui.py").output();
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("HyprWpE.*scene_generator.py").output();

    // 3. Proc scan targeting legacy python directory
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(cmdline) = fs::read_to_string(entry.path().join("cmdline")) {
                let cmd = cmdline.replace('\0', " ");
                if (cmd.contains("/HyprWpE/") || cmd.contains("web_viewer.py"))
                    && (cmd.contains("python") || cmd.contains("bash") || cmd.contains("sh"))
                    && !cmd.contains("hyprwpe-")
                    && !cmd.contains("HyprWpE-Rust")
                {
                    if let Some(pid_str) = entry.file_name().to_str() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
                        }
                    }
                }
            }
        }
    }
}

pub fn kill_process_tree(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(format!("-{pid}")).output();
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();

    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(proc_pid) = name.parse::<u32>() {
                        if proc_pid == pid {
                            continue;
                        }
                        if let Ok(stat) = fs::read_to_string(entry.path().join("stat")) {
                            if let Some(close_paren) = stat.rfind(')') {
                                let rest = &stat[close_paren + 2..];
                                let fields: Vec<&str> = rest.split_whitespace().collect();
                                if fields.len() >= 3 {
                                    let ppid = fields[1].parse::<u32>().unwrap_or(0);
                                    let pgrp = fields[2].parse::<u32>().unwrap_or(0);
                                    if pgrp == pid || ppid == pid {
                                        let _ = Command::new("kill").arg("-9").arg(proc_pid.to_string()).output();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn stop_wallpaper(monitor: &str) {
    if let Some(pid) = read_pid(monitor) {
        kill_process_tree(pid);
    }
    let _ = fs::remove_file(get_pid_file(monitor));
    let _ = fs::remove_file(get_lock_file(monitor));

    let pkill_pattern = format!(r"(mpvpaper|hyprwpe-web).*{monitor}");
    let _ = Command::new("pkill").arg("-9").arg("-f").arg(pkill_pattern).output();
}

pub fn stop_all_wallpapers() {
    kill_legacy_python_instances();

    if let Ok(entries) = fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("hyprwpe-rust-") && name.ends_with(".pid") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(pid) = content.trim().parse::<u32>() {
                            kill_process_tree(pid);
                        }
                    }
                    let _ = fs::remove_file(&path);
                } else if (name.starts_with("hyprwpe-rust-") || name.starts_with("HyprWpE-")) && name.ends_with(".lock") {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }

    let _ = Command::new("pkill").arg("-9").arg("-f").arg("hyprwpe-web").output();
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("mpvpaper").output();
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("linux-wallpaperengine").output();
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("hyprwpe-viewer").output();
    let _ = Command::new("pkill").arg("-9").arg("-f").arg("webkitgtk-6.0/WebKitWebProcess").output();

    if let Ok(entries) = fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("hyprwpe-rust-") || name.starts_with("HyprWpE-") {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

