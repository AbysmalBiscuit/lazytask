use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::taskrc::Taskrc;
use crate::ui::theme::DEFAULT_THEME_NAME;
use crate::utils::helpers::expand_tilde;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    pub keybindings: KeyBindingsConfig,
    pub taskwarrior: TaskwarriorConfig,
    pub ui: UIConfig,
    pub sync: SyncConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    /// Role name to color, overriding the named theme's palette.
    pub colors: HashMap<String, String>,
    /// Draw without color. Set from the `NO_COLOR` environment variable,
    /// never from the config file.
    #[serde(skip)]
    pub no_color: bool,
}

/// Key overrides per section, action name to key string. Actions left out
/// keep their default keys; see [`crate::utils::keybindings::Binding`].
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct KeyBindingsConfig {
    pub global: HashMap<String, String>,
    pub task_list: HashMap<String, String>,
    pub reports: HashMap<String, String>,
    pub form: HashMap<String, String>,
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
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// Seconds between automatic syncs with the sync server; 0 turns it off.
    pub auto_sync_interval: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            auto_sync_interval: 5,
        }
    }
}

impl TaskwarriorConfig {
    /// The taskrc to read, first match wins: `taskrc_path`, then `taskrc_var`
    /// (the `TASKRC` variable), then `~/.taskrc`. `None` when nothing names
    /// one and there is no home directory.
    pub fn resolve_taskrc_path(
        &self,
        taskrc_var: Option<OsString>,
        home: Option<&Path>,
    ) -> Result<Option<PathBuf>> {
        let named = self
            .taskrc_path
            .clone()
            .or_else(|| taskrc_var.filter(|v| !v.is_empty()).map(PathBuf::from));
        match (named, home) {
            (Some(path), _) => expand_tilde(&path, home).map(Some),
            (None, Some(home)) => Ok(Some(home.join(".taskrc"))),
            (None, None) => Ok(None),
        }
    }

    /// The TaskChampion data directory, first match wins: `data_location`,
    /// then `taskdata_var` (the `TASKDATA` variable), then the taskrc's
    /// `data.location`, then `~/.task`. A leading `~` in the winning path
    /// expands to `home`, which is only required when that expansion happens.
    pub fn resolve_data_location(
        &self,
        taskdata_var: Option<OsString>,
        taskrc: &Taskrc,
        home: Option<&Path>,
    ) -> Result<PathBuf> {
        let location = self
            .data_location
            .clone()
            .or_else(|| taskdata_var.map(PathBuf::from))
            .or_else(|| taskrc.data_location())
            .unwrap_or_else(|| PathBuf::from("~/.task"));
        expand_tilde(&location, home)
    }
}

fn empty_path_as_none<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<PathBuf>, D::Error> {
    let path = Option::<PathBuf>::deserialize(deserializer)?;
    Ok(path.filter(|p| !p.as_os_str().is_empty()))
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            name: DEFAULT_THEME_NAME.to_string(),
            colors: HashMap::new(),
            no_color: false,
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
