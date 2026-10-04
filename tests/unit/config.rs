use std::path::{Path, PathBuf};

use lazytask::config::{Config, LoadedConfig};
use tempfile::tempdir;

fn load_toml(contents: &str) -> anyhow::Result<LoadedConfig> {
    let dir = tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, contents)?;
    Config::load(Some(path.to_str().unwrap()))
}

#[test]
fn partial_config_takes_defaults_for_absent_keys() -> anyhow::Result<()> {
    let loaded = load_toml("[ui]\nshow_help_bar = false\n")?;

    let mut expected = Config::default();
    expected.ui.show_help_bar = false;
    assert_eq!(loaded.config, expected);
    assert!(loaded.unknown_keys.is_empty());
    Ok(())
}

#[test]
fn empty_path_strings_resolve_to_unset() -> anyhow::Result<()> {
    let config = load_toml("[taskwarrior]\ntaskrc_path = \"\"\ndata_location = \"\"\n")?.config;

    assert_eq!(config.taskwarrior.taskrc_path, None);
    assert_eq!(config.taskwarrior.data_location, None);
    Ok(())
}

#[test]
fn non_empty_path_strings_load_as_paths() -> anyhow::Result<()> {
    let config = load_toml("[taskwarrior]\ntaskrc_path = \"/home/me/.taskrc\"\n")?.config;

    assert_eq!(
        config.taskwarrior.taskrc_path,
        Some(PathBuf::from("/home/me/.taskrc"))
    );
    Ok(())
}

#[test]
fn missing_config_file_runs_on_defaults_and_writes_nothing() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let config_dir = temp_dir.path().join("lazytask");
    let config_path = config_dir.join("config.toml");

    let config = Config::load(Some(config_path.to_str().unwrap()))?.config;

    assert_eq!(config, Config::default());
    assert!(!config_dir.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unreadable_config_directory_is_an_error_not_defaults() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = tempdir()?;
    let config_dir = temp_dir.path().join("lazytask");
    std::fs::create_dir(&config_dir)?;
    let config_path = config_dir.join("config.toml");
    std::fs::write(&config_path, "[ui]\nshow_help_bar = false\n")?;
    std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(0o000))?;

    let result = Config::load(Some(config_path.to_str().unwrap()));

    std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(0o755))?;
    assert!(result.is_err(), "loaded {:?}", result.map(|l| l.config));
    Ok(())
}

#[test]
fn unknown_keys_load_and_are_reported_by_dotted_path() -> anyhow::Result<()> {
    let loaded = load_toml(
        "[ui]\nshow_help_bar = false\ncolour = \"red\"\n\n\
         [taskwarrior]\nsync_enabled = true\n\n\
         [sync]\nurl = \"https://example.com\"\n",
    )?;

    assert!(!loaded.config.ui.show_help_bar);
    let mut unknown = loaded.unknown_keys;
    unknown.sort();
    assert_eq!(unknown, ["sync", "taskwarrior.sync_enabled", "ui.colour"]);
    Ok(())
}

#[test]
fn shipped_example_config_loads_without_unknown_keys() -> anyhow::Result<()> {
    let loaded = load_toml(include_str!("../../config/default.toml"))?;

    assert_eq!(loaded.unknown_keys, Vec::<String>::new());
    assert_eq!(loaded.config.taskwarrior.taskrc_path, None);
    assert_eq!(loaded.config.taskwarrior.data_location, None);
    Ok(())
}

#[test]
fn data_location_defaults_to_dot_task_in_home() -> anyhow::Result<()> {
    let config = load_toml("")?.config;

    let location = config
        .taskwarrior
        .resolve_data_location(None, Path::new("/home/me"));

    assert_eq!(location, PathBuf::from("/home/me/.task"));
    Ok(())
}

#[test]
fn taskdata_wins_over_the_home_default() -> anyhow::Result<()> {
    let config = load_toml("")?.config;

    let location = config
        .taskwarrior
        .resolve_data_location(Some("/srv/tasks".into()), Path::new("/home/me"));

    assert_eq!(location, PathBuf::from("/srv/tasks"));
    Ok(())
}

#[test]
fn config_data_location_wins_over_taskdata_and_the_home_default() -> anyhow::Result<()> {
    let config = load_toml("[taskwarrior]\ndata_location = \"/opt/tasks\"\n")?.config;

    let location = config
        .taskwarrior
        .resolve_data_location(Some("/srv/tasks".into()), Path::new("/home/me"));

    assert_eq!(location, PathBuf::from("/opt/tasks"));
    Ok(())
}

#[test]
fn leading_tilde_in_data_location_expands_to_home() -> anyhow::Result<()> {
    let config = load_toml("[taskwarrior]\ndata_location = \"~/tasks/db\"\n")?.config;

    let location = config
        .taskwarrior
        .resolve_data_location(None, Path::new("/home/me"));

    assert_eq!(location, PathBuf::from("/home/me/tasks/db"));
    Ok(())
}

#[test]
fn leading_tilde_in_taskdata_expands_to_home() -> anyhow::Result<()> {
    let config = load_toml("")?.config;

    let location = config
        .taskwarrior
        .resolve_data_location(Some("~/tilde".into()), Path::new("/home/me"));

    assert_eq!(location, PathBuf::from("/home/me/tilde"));
    Ok(())
}
