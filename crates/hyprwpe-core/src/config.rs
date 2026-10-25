use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::models::PanelMargins;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to determine user home directory")]
    HomeNotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_wallpaper_dir")]
    pub wallpaper_dir: String,
    #[serde(default)]
    pub wallpapers: HashMap<String, String>,
}

fn default_wallpaper_dir() -> String {
    ConfigManager::default_wallpaper_dir().to_string_lossy().to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            wallpaper_dir: default_wallpaper_dir(),
            wallpapers: HashMap::new(),
        }
    }
}

pub struct ConfigManager {
    config_dir: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self, ConfigError> {
        let home = dirs::home_dir().ok_or(ConfigError::HomeNotFound)?;
        let config_dir = home.join(".config").join("HyprWpE");
        Ok(Self { config_dir })
    }

    pub fn with_config_dir(path: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: path.into(),
        }
    }

    pub fn default_wallpaper_dir() -> PathBuf {
        if let Some(home) = dirs::home_dir() {
            let p1 = home.join(".local/share/Steam/steamapps/workshop/content/431960");
            if p1.is_dir() { return p1; }
            let p2 = home.join(".steam/steam/steamapps/workshop/content/431960");
            if p2.is_dir() { return p2; }
            let p3 = home.join(".steam/root/steamapps/workshop/content/431960");
            if p3.is_dir() { return p3; }
            return p1;
        }
        PathBuf::from("/home/scushi/.local/share/Steam/steamapps/workshop/content/431960")
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn wallpapers_yaml_path(&self) -> PathBuf {
        self.config_dir.join("wallpapers.yaml")
    }

    pub fn properties_yaml_path(&self) -> PathBuf {
        self.config_dir.join("properties.yaml")
    }

    pub fn ensure_config_dir(&self) -> Result<(), ConfigError> {
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir)?;
        }
        Ok(())
    }

    pub fn load_config(&self) -> Result<AppConfig, ConfigError> {
        self.ensure_config_dir()?;
        let path = self.wallpapers_yaml_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let mut config: AppConfig = serde_yaml::from_str(&content)?;
            if config.wallpaper_dir.is_empty() {
                config.wallpaper_dir = Self::default_wallpaper_dir().to_string_lossy().to_string();
            }
            Ok(config)
        } else {
            let config = AppConfig::default();
            self.save_config(&config)?;
            Ok(config)
        }
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), ConfigError> {
        self.ensure_config_dir()?;
        let path = self.wallpapers_yaml_path();
        let content = serde_yaml::to_string(config)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn load_properties(&self) -> Result<HashMap<String, Value>, ConfigError> {
        self.ensure_config_dir()?;
        let path = self.properties_yaml_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let props: HashMap<String, Value> = serde_yaml::from_str(&content).unwrap_or_default();
            Ok(props)
        } else {
            let mut props = HashMap::new();
            let default_margins = serde_json::to_value(PanelMargins::default()).unwrap();
            props.insert("panel_margins".to_string(), default_margins);
            self.save_properties(&props)?;
            Ok(props)
        }
    }

    pub fn save_properties(&self, properties: &HashMap<String, Value>) -> Result<(), ConfigError> {
        self.ensure_config_dir()?;
        let path = self.properties_yaml_path();
        let content = serde_yaml::to_string(properties)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn get_panel_margins(&self, properties: &HashMap<String, Value>) -> PanelMargins {
        if let Some(margins_val) = properties.get("panel_margins") {
            if let Ok(margins) = serde_json::from_value::<PanelMargins>(margins_val.clone()) {
                return margins;
            }
        }
        PanelMargins::default()
    }
}
