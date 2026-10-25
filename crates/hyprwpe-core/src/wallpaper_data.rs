use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde_json::Value;

use crate::models::{Wallpaper, WallpaperType};

pub struct WallpaperDataManager {
    wallpaper_dir: PathBuf,
    all_wallpapers: Vec<Wallpaper>,
    cache_dir: PathBuf,
}

impl WallpaperDataManager {
    pub fn new(wallpaper_dir: impl Into<PathBuf>) -> Self {
        let cache_dir = PathBuf::from("/tmp/HyprWpE/thumbnails");
        let _ = fs::create_dir_all(&cache_dir);
        Self {
            wallpaper_dir: wallpaper_dir.into(),
            all_wallpapers: Vec::new(),
            cache_dir,
        }
    }

    pub fn wallpaper_dir(&self) -> &Path {
        &self.wallpaper_dir
    }

    pub fn load_wallpaper_data(&mut self) -> &[Wallpaper] {
        self.all_wallpapers.clear();

        if !self.wallpaper_dir.is_dir() {
            return &self.all_wallpapers;
        }

        if let Ok(entries) = fs::read_dir(&self.wallpaper_dir) {
            for entry in entries.flatten() {
                let wallpaper_path = entry.path();
                if !wallpaper_path.is_dir() {
                    continue;
                }

                let wallpaper_id = match wallpaper_path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };

                let project_json_path = wallpaper_path.join("project.json");
                if !project_json_path.exists() {
                    continue;
                }

                if let Ok(content) = fs::read_to_string(&project_json_path) {
                    if let Ok(data) = serde_json::from_str::<Value>(&content) {
                        let title = data.get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("No Title")
                            .to_string();

                        let type_str = data.get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");

                        let wp_type = WallpaperType::from_str(type_str);

                        // Resolve preview image path
                        let mut found_src_preview: Option<PathBuf> = None;

                        if let Some(p_val) = data.get("preview").and_then(|v| v.as_str()) {
                            let p_path = wallpaper_path.join(p_val);
                            if p_path.is_file() {
                                found_src_preview = Some(p_path);
                            }
                        }

                        if found_src_preview.is_none() {
                            for alt in &["preview.jpg", "preview.png", "preview.gif", "preview.jpeg"] {
                                let alt_path = wallpaper_path.join(alt);
                                if alt_path.is_file() {
                                    found_src_preview = Some(alt_path);
                                    break;
                                }
                            }
                        }

                        let resolved_preview_str = if let Some(src_p) = found_src_preview {
                            let ext = src_p.extension().and_then(|s| s.to_str()).unwrap_or("");
                            if ext.eq_ignore_ascii_case("gif") {
                                let cached_png = self.cache_dir.join(format!("{wallpaper_id}.png"));
                                if cached_png.is_file() {
                                    cached_png.to_string_lossy().to_string()
                                } else {
                                    if let Ok(img) = image::open(&src_p) {
                                        let resized = img.thumbnail(300, 200);
                                        if resized.save(&cached_png).is_ok() {
                                            cached_png.to_string_lossy().to_string()
                                        } else {
                                            src_p.to_string_lossy().to_string()
                                        }
                                    } else {
                                        src_p.to_string_lossy().to_string()
                                    }
                                }
                            } else {
                                src_p.to_string_lossy().to_string()
                            }
                        } else {
                            String::new()
                        };

                        let properties = data.get("general")
                            .and_then(|g| g.get("properties"))
                            .and_then(|p| p.as_object())
                            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default();

                        let wallpaper = Wallpaper {
                            id: wallpaper_id,
                            title: title.clone(),
                            wallpaper_type: wp_type,
                            preview_path: resolved_preview_str,
                            title_lower: title.to_lowercase(),
                            properties,
                        };

                        self.all_wallpapers.push(wallpaper);
                    }
                }
            }
        }

        &self.all_wallpapers
    }

    pub fn apply_filters(
        &self,
        search_term: &str,
        type_filters: &HashMap<String, bool>,
    ) -> Vec<Wallpaper> {
        let search_lower = search_term.to_lowercase();
        self.all_wallpapers
            .iter()
            .filter(|wp| {
                let matches_search = if search_lower.is_empty() {
                    true
                } else {
                    wp.title_lower.contains(&search_lower)
                };

                let type_str = wp.wallpaper_type.as_str();
                let matches_type = type_filters.get(type_str).copied().unwrap_or(true);

                matches_search && matches_type
            })
            .cloned()
            .collect()
    }
}
