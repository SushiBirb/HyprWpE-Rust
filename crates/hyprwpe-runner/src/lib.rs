pub mod monitor;
pub mod process_manager;
pub mod runner;
pub mod web_server;

pub use monitor::detect_monitors;
pub use process_manager::{
    get_lock_file, get_pid_file, kill_legacy_python_instances, read_pid, stop_all_wallpapers,
    stop_wallpaper, write_pid,
};
pub use runner::set_wallpaper;
pub use web_server::start_local_http_server;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_pid_and_lock_file_helpers() {
        let monitor = "DP-TEST";
        write_pid(monitor, 12345);
        let pid = read_pid(monitor);
        assert_eq!(pid, Some(12345));

        stop_wallpaper(monitor);
        assert_eq!(read_pid(monitor), None);
    }

    #[test]
    fn test_local_http_server() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("index.html"), "<h1>HyprWpE Test</h1>").unwrap();

        let port = start_local_http_server(dir.path().to_path_buf()).unwrap();
        assert!(port > 0);

        let url = format!("http://127.0.0.1:{port}/index.html");
        let body = reqwest::blocking::get(url).unwrap().text().unwrap();
        assert_eq!(body, "<h1>HyprWpE Test</h1>");
    }
}
