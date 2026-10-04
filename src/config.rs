use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    pub keybindings: KeyBindingsConfig,
    pub taskwarrior: TaskwarriorConfig,
    pub ui: UIConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub colors: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct KeyBindingsConfig {
    pub global: HashMap<String, String>,
    pub task_list: HashMap<String, String>,
    pub task_detail: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct TaskwarriorConfig {
    #[serde(deserialize_with = "empty_path_as_none")]
    pub taskrc_path: Option<PathBuf>,
    #[serde(deserialize_with = "empty_path_as_none")]
    pub data_location: Option<PathBuf>,
    pub sync_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct UIConfig {
    pub default_view: String,
    pub show_help_bar: bool,
    pub task_list_columns: Vec<String>,
    pub refresh_interval: u64,
}

fn empty_path_as_none<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<PathBuf>, D::Error> {
    let path = Option::<PathBuf>::deserialize(deserializer)?;
    Ok(path.filter(|p| !p.as_os_str().is_empty()))
}

fn string_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            name: "catppuccin-mocha".to_string(),
            colors: string_map(&[
                ("background", "#1e1e2e"),
                ("foreground", "#cdd6f4"),
                ("primary", "#89b4fa"),
                ("secondary", "#f38ba8"),
            ]),
        }
    }
}

impl Default for KeyBindingsConfig {
    fn default() -> Self {
        KeyBindingsConfig {
            global: string_map(&[("quit", "q"), ("help", "F1"), ("refresh", "F5")]),
            task_list: string_map(&[
                ("add_task", "a"),
                ("edit_task", "e"),
                ("done_task", "d"),
                ("delete_task", "Delete"),
            ]),
            task_detail: HashMap::new(),
        }
    }
}

impl Default for UIConfig {
    fn default() -> Self {
        UIConfig {
            default_view: "task_list".to_string(),
            show_help_bar: true,
            task_list_columns: ["id", "project", "priority", "due", "description"]
                .map(String::from)
                .to_vec(),
            refresh_interval: 1000,
        }
    }
}

impl Config {
    pub fn load(config_path: Option<&str>) -> Result<Self> {
        let config_file_path = if let Some(path) = config_path {
            PathBuf::from(path)
        } else {
            Self::default_config_path()?
        };

        if !config_file_path.exists() {
            return Ok(Config::default());
        }

        let config_contents = fs::read_to_string(&config_file_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_file_path))?;

        toml::from_str(&config_contents).with_context(|| "Failed to parse config file")
    }

    fn default_config_path() -> Result<PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;

        Ok(config_dir.join("lazytask").join("config.toml"))
    }
}
