use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    pub keybindings: KeyBindingsConfig,
    pub taskwarrior: TaskwarriorConfig,
    pub ui: UIConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub colors: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct KeyBindingsConfig {
    pub global: HashMap<String, String>,
    pub task_list: HashMap<String, String>,
    pub task_detail: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TaskwarriorConfig {
    #[serde(deserialize_with = "empty_path_as_none")]
    pub taskrc_path: Option<PathBuf>,
    #[serde(deserialize_with = "empty_path_as_none")]
    pub data_location: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct UIConfig {
    pub default_view: String,
    pub show_help_bar: bool,
    pub task_list_columns: Vec<String>,
    pub refresh_interval: u64,
}

impl TaskwarriorConfig {
    /// The TaskChampion data directory, first match wins: `data_location`,
    /// then `taskdata` (the `TASKDATA` variable), then `~/.task`. A leading
    /// `~` in the winning path expands to `home`.
    pub fn resolve_data_location(&self, taskdata: Option<OsString>, home: &Path) -> PathBuf {
        let location = self
            .data_location
            .clone()
            .or_else(|| taskdata.map(PathBuf::from))
            .unwrap_or_else(|| home.join(".task"));
        match location.strip_prefix("~") {
            Ok(rest) => home.join(rest),
            Err(_) => location,
        }
    }
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

/// A parsed config plus the keys the file set that lazytask does not know.
#[derive(Debug)]
pub struct LoadedConfig {
    pub config: Config,
    /// Dotted paths such as `ui.colour`.
    pub unknown_keys: Vec<String>,
}

impl Config {
    pub fn load(config_path: Option<&str>) -> Result<LoadedConfig> {
        let config_file_path = if let Some(path) = config_path {
            PathBuf::from(path)
        } else {
            Self::default_config_path()?
        };

        let config_contents = match fs::read_to_string(&config_file_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Ok(LoadedConfig {
                    config: Config::default(),
                    unknown_keys: Vec::new(),
                });
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("Failed to read config file: {:?}", config_file_path)
                });
            }
        };

        let mut unknown_keys = Vec::new();
        let config = toml::de::Deserializer::parse(&config_contents)
            .and_then(|document| {
                serde_ignored::deserialize(document, |path| unknown_keys.push(path.to_string()))
            })
            .with_context(|| "Failed to parse config file")?;
        Ok(LoadedConfig {
            config,
            unknown_keys,
        })
    }

    fn default_config_path() -> Result<PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;

        Ok(config_dir.join("lazytask").join("config.toml"))
    }
}
