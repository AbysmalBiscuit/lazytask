use anyhow::{Context, Result};
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use strum::{EnumString, IntoEnumIterator};

use crate::taskrc::Taskrc;
use crate::ui::components::task_list::Column;
use crate::utils::helpers::expand_tilde;
use crate::utils::keybindings::{
    Bindable, FormAction, GlobalAction, ReportsAction, TaskListAction,
};

/// lazytask's `config.toml`. Every table and key is optional; a key left
/// out keeps its default.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Config {
    /// Color theme.
    pub theme: ThemeConfig,
    /// Keys for each action, by the section the action belongs to.
    pub keybindings: KeyBindingsConfig,
    /// Where the Taskwarrior taskrc and task data live.
    pub taskwarrior: TaskwarriorConfig,
    /// Layout and startup view.
    pub ui: UIConfig,
    /// Automatic sync with the sync target the taskrc configures.
    pub sync: SyncConfig,
}

/// The only built-in theme, and the one an unknown `theme.name` falls back
/// to.
pub const DEFAULT_THEME_NAME: &str = "catppuccin-mocha";

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ThemeConfig {
    /// Theme name.
    pub name: String,
    /// Role name to color, overriding the named theme's palette.
    pub colors: HashMap<String, String>,
    /// Draw without color. Set from the `NO_COLOR` environment variable,
    /// never from the config file.
    #[serde(skip)]
    pub no_color: bool,
}

/// Key overrides, action name to a key such as `q`, `Ctrl+s`,
/// `Shift+Tab` or `F1`. Actions left out keep their default keys.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default)]
pub struct KeyBindingsConfig {
    /// Actions active in every view outside a form.
    #[schemars(schema_with = "action_keys::<GlobalAction>")]
    pub global: HashMap<String, String>,
    /// Actions in the task list.
    #[schemars(schema_with = "action_keys::<TaskListAction>")]
    pub task_list: HashMap<String, String>,
    /// Actions in the reports and calendar view.
    #[schemars(schema_with = "action_keys::<ReportsAction>")]
    pub reports: HashMap<String, String>,
    /// Actions while a form or the filter panel has focus.
    #[schemars(schema_with = "action_keys::<FormAction>")]
    pub form: HashMap<String, String>,
}

/// A keybindings section's schema: every action in `A` with its help text
/// and default key, so a new action reaches the schema on its own.
fn action_keys<A: Bindable + IntoEnumIterator>(_: &mut SchemaGenerator) -> Schema {
    let actions: serde_json::Map<String, serde_json::Value> = A::iter()
        .map(|action| {
            let mut schema = serde_json::json!({
                "type": "string",
                "description": action.description(),
            });
            if let Some(key) = action.default_key() {
                schema["default"] = key.into();
            }
            (action.name().to_string(), schema)
        })
        .collect();
    json_schema!({
        "type": "object",
        "properties": actions,
        "additionalProperties": { "type": "string" },
    })
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TaskwarriorConfig {
    /// The taskrc to read. Empty or absent means the `TASKRC` variable, then
    /// `~/.taskrc`, or `$XDG_CONFIG_HOME/task/taskrc` (`~/.config/task/taskrc`
    /// when that variable is unset or empty) when only that file exists.
    #[serde(deserialize_with = "empty_path_as_none")]
    pub taskrc_path: Option<PathBuf>,
    /// The task data directory. Empty or absent means the `TASKDATA`
    /// variable, then the taskrc's `data.location`, then `~/.task`.
    #[serde(deserialize_with = "empty_path_as_none")]
    pub data_location: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(default)]
pub struct UIConfig {
    /// The view lazytask opens on.
    #[schemars(with = "DefaultView")]
    pub default_view: String,
    /// Show keybinding hints in the footer.
    pub show_help_bar: bool,
    /// Task list columns, in order.
    #[schemars(with = "Vec<Column>")]
    pub task_list_columns: Vec<String>,
}

/// A view lazytask can open on, named in config by `ui.default_view`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, JsonSchema)]
#[strum(serialize_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum DefaultView {
    TaskList,
    Reports,
    Calendar,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
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

/// Where a resolved taskrc or data directory path came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSource {
    /// lazytask's config file.
    Config,
    /// `TASKRC` for the taskrc, `TASKDATA` for the data directory.
    EnvVar,
    /// The taskrc's `data.location`.
    Taskrc,
    /// A location Taskwarrior reads when nothing names one.
    Default,
}

/// A resolved taskrc or data directory path and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    pub path: PathBuf,
    pub source: PathSource,
}

impl TaskwarriorConfig {
    /// The taskrc to read, found the way Taskwarrior finds it: `taskrc_path`,
    /// then `taskrc_var` (the `TASKRC` variable), then `~/.taskrc` if it
    /// exists, then `xdg_config_home_var` (the `XDG_CONFIG_HOME` variable,
    /// `~/.config` when unset or empty) joined with `task/taskrc` if that
    /// exists, then `~/.taskrc`. `None` when none of these is named or found
    /// and there is no home directory.
    pub fn resolve_taskrc_path(
        &self,
        taskrc_var: Option<OsString>,
        xdg_config_home_var: Option<OsString>,
        home: Option<&Path>,
    ) -> Result<Option<ResolvedPath>> {
        let named = self
            .taskrc_path
            .clone()
            .map(|path| (path, PathSource::Config))
            .or_else(|| {
                taskrc_var
                    .filter(|v| !v.is_empty())
                    .map(|var| (PathBuf::from(var), PathSource::EnvVar))
            });
        if let Some((path, source)) = named {
            let path = expand_tilde(&path, home)?;
            return Ok(Some(ResolvedPath { path, source }));
        }
        let home_taskrc = home.map(|home| home.join(".taskrc"));
        if home_taskrc.as_ref().is_some_and(|path| path.exists()) {
            return Ok(home_taskrc.map(|path| ResolvedPath {
                path,
                source: PathSource::Default,
            }));
        }
        let config_home = match xdg_config_home_var.filter(|v| !v.is_empty()) {
            Some(var) => Some(expand_tilde(Path::new(&var), home)?),
            None => home.map(|home| home.join(".config")),
        };
        let xdg_taskrc = config_home
            .map(|config_home| config_home.join("task").join("taskrc"))
            .filter(|path| path.exists());
        Ok(xdg_taskrc.or(home_taskrc).map(|path| ResolvedPath {
            path,
            source: PathSource::Default,
        }))
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
    ) -> Result<ResolvedPath> {
        let (location, source) = self
            .data_location
            .clone()
            .map(|path| (path, PathSource::Config))
            .or_else(|| taskdata_var.map(|var| (PathBuf::from(var), PathSource::EnvVar)))
            .or_else(|| {
                taskrc
                    .data_location()
                    .map(|path| (path, PathSource::Taskrc))
            })
            .unwrap_or_else(|| (PathBuf::from("~/.task"), PathSource::Default));
        Ok(ResolvedPath {
            path: expand_tilde(&location, home)?,
            source,
        })
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

        Self::parse(&config_contents)
    }

    /// Parses the contents of a config file.
    pub fn parse(contents: &str) -> Result<LoadedConfig> {
        let mut unknown_keys = Vec::new();
        let config = toml::de::Deserializer::parse(contents)
            .and_then(|document| {
                serde_ignored::deserialize(document, |path| unknown_keys.push(path.to_string()))
            })
            .with_context(|| "Failed to parse config file")?;
        Ok(LoadedConfig {
            config,
            unknown_keys,
        })
    }

    /// `config.toml` under the platform config directory, such as
    /// `~/.config/lazytask/config.toml` on Linux.
    pub fn default_config_path() -> Result<PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;

        Ok(config_dir.join("lazytask").join("config.toml"))
    }
}
