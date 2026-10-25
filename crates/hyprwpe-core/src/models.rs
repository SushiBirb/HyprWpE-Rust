use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperType {
    Video,
    Scene,
    Web,
    Unknown(String),
}

impl Default for WallpaperType {
    fn default() -> Self {
        WallpaperType::Video
    }
}

impl WallpaperType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "video" => WallpaperType::Video,
            "scene" => WallpaperType::Scene,
            "web" => WallpaperType::Web,
            other => WallpaperType::Unknown(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            WallpaperType::Video => "video",
            WallpaperType::Scene => "scene",
            WallpaperType::Web => "web",
            WallpaperType::Unknown(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleMode {
    Cover,
    Contain,
    Fill,
}

impl Default for ScaleMode {
    fn default() -> Self {
        ScaleMode::Cover
    }
}

impl ScaleMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "contain" => ScaleMode::Contain,
            "fill" => ScaleMode::Fill,
            _ => ScaleMode::Cover,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ScaleMode::Cover => "Cover",
            ScaleMode::Contain => "Contain",
            ScaleMode::Fill => "Fill",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallpaper {
    pub id: String,
    pub title: String,
    pub wallpaper_type: WallpaperType,
    pub preview_path: String,
    pub title_lower: String,
    pub properties: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperProperties {
    #[serde(default)]
    pub audio: bool,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default)]
    pub scale: String,
    #[serde(default)]
    pub span: bool,
    #[serde(default)]
    pub scene_props: HashMap<String, Value>,
}

fn default_speed() -> f32 {
    1.0
}

impl Default for WallpaperProperties {
    fn default() -> Self {
        Self {
            audio: false,
            speed: 1.0,
            scale: "Cover".to_string(),
            span: false,
            scene_props: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelMargins {
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

impl Default for PanelMargins {
    fn default() -> Self {
        Self {
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
        }
    }
}
