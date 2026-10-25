pub mod config;
pub mod models;
pub mod wallpaper_data;

pub use config::{AppConfig, ConfigError, ConfigManager};
pub use models::{PanelMargins, ScaleMode, Wallpaper, WallpaperProperties, WallpaperType};
pub use wallpaper_data::WallpaperDataManager;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_wallpaper_type_conversion() {
        assert_eq!(WallpaperType::from_str("video"), WallpaperType::Video);
        assert_eq!(WallpaperType::from_str("SCENE"), WallpaperType::Scene);
        assert_eq!(WallpaperType::from_str("Web"), WallpaperType::Web);
        assert_eq!(
            WallpaperType::from_str("custom"),
            WallpaperType::Unknown("custom".to_string())
        );

        assert_eq!(WallpaperType::Video.as_str(), "video");
        assert_eq!(WallpaperType::Scene.as_str(), "scene");
        assert_eq!(WallpaperType::Web.as_str(), "web");
    }

    #[test]
    fn test_scale_mode_conversion() {
        assert_eq!(ScaleMode::from_str("cover"), ScaleMode::Cover);
        assert_eq!(ScaleMode::from_str("CONTAIN"), ScaleMode::Contain);
        assert_eq!(ScaleMode::from_str("fill"), ScaleMode::Fill);
        assert_eq!(ScaleMode::from_str("other"), ScaleMode::Cover);

        assert_eq!(ScaleMode::Cover.as_str(), "Cover");
        assert_eq!(ScaleMode::Contain.as_str(), "Contain");
        assert_eq!(ScaleMode::Fill.as_str(), "Fill");
    }

    #[test]
    fn test_config_manager_file_ops() {
        let dir = tempdir().unwrap();
        let cm = ConfigManager::with_config_dir(dir.path());

        let mut config = AppConfig::default();
        config.wallpaper_dir = "/tmp/test_wallpapers".to_string();
        config.wallpapers.insert("DP-1".to_string(), "12345".to_string());

        cm.save_config(&config).unwrap();
        assert!(cm.wallpapers_yaml_path().exists());

        let loaded = cm.load_config().unwrap();
        assert_eq!(loaded.wallpaper_dir, "/tmp/test_wallpapers");
        assert_eq!(loaded.wallpapers.get("DP-1").unwrap(), "12345");
    }

    #[test]
    fn test_wallpaper_data_scanner_and_filtering() {
        let dir = tempdir().unwrap();
        let wp_folder = dir.path().join("822865320");
        fs::create_dir_all(&wp_folder).unwrap();

        let project_json = r#"{
            "title": "Anime Sunset",
            "type": "video",
            "preview": "preview.mp4"
        }"#;
        fs::write(wp_folder.join("project.json"), project_json).unwrap();
        fs::write(wp_folder.join("preview.jpg"), "fake image data").unwrap();

        let mut manager = WallpaperDataManager::new(dir.path());
        let wallpapers = manager.load_wallpaper_data();

        assert_eq!(wallpapers.len(), 1);
        let wp = &wallpapers[0];
        assert_eq!(wp.id, "822865320");
        assert_eq!(wp.title, "Anime Sunset");
        assert_eq!(wp.wallpaper_type, WallpaperType::Video);
        assert!(wp.preview_path.ends_with("preview.jpg"));

        let mut type_filters = HashMap::new();
        type_filters.insert("video".to_string(), true);
        type_filters.insert("scene".to_string(), false);

        let filtered = manager.apply_filters("anime", &type_filters);
        assert_eq!(filtered.len(), 1);

        let filtered_no_match = manager.apply_filters("cyberpunk", &type_filters);
        assert_eq!(filtered_no_match.len(), 0);
    }
}
