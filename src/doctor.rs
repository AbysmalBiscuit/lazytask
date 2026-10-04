//! `lazytask doctor`: how lazytask resolves its config, taskrc, data
//! directory and sync settings, where each value came from, and whether it
//! works. Every check runs, whatever an earlier one found, and none of them
//! writes anything.

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use taskchampion::server::GetVersionResult;
use taskchampion::storage::AccessMode;
use taskchampion::{Replica, ServerConfig, SqliteStorage, Uuid};

use crate::app::LaunchEnv;
use crate::config::{Config, LoadedConfig, PathSource, ResolvedPath};
use crate::schema::has_schema_header;
use crate::taskchampion::SyncSettings;
use crate::taskrc::Taskrc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// Not run; its finding says why.
    Skip,
    Pass,
    /// Works, but probably not as intended.
    Warn,
    /// Broken: lazytask will not start, or ignores what was asked of it.
    Fail,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Skip => "skip",
            Status::Pass => "pass",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub status: Status,
    pub message: String,
}

/// One resolved setting: its value, where that value came from, and what
/// checking it found.
#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub value: String,
    pub source: String,
    pub findings: Vec<Finding>,
}

impl Check {
    fn new(name: &'static str, value: impl fmt::Display, source: impl Into<String>) -> Self {
        Check {
            name,
            value: value.to_string(),
            source: source.into(),
            findings: Vec::new(),
        }
    }

    /// The worst status among the findings; a check with none passed.
    pub fn status(&self) -> Status {
        self.findings
            .iter()
            .map(|finding| finding.status)
            .max()
            .unwrap_or(Status::Pass)
    }

    fn find(&mut self, status: Status, message: impl Into<String>) {
        self.findings.push(Finding {
            status,
            message: message.into(),
        });
    }

    fn note(&mut self, message: impl Into<String>) {
        self.find(Status::Pass, message);
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.find(Status::Warn, message);
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.find(Status::Fail, message);
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn failed(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status() == Status::Fail)
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for check in &self.checks {
            writeln!(f, "[{}] {}", check.status().label(), check.name)?;
            writeln!(f, "       value:  {}", check.value)?;
            writeln!(f, "       source: {}", check.source)?;
            for finding in &check.findings {
                let label = match finding.status {
                    Status::Skip | Status::Pass => "note",
                    status => status.label(),
                };
                writeln!(f, "       {label}:   {}", finding.message)?;
            }
        }
        Ok(())
    }
}

/// Runs every check against `config_path` (the `--config` flag) and `env`.
/// The sync server is contacted only when `contact_sync_server` is set.
pub async fn run(config_path: Option<&str>, env: &LaunchEnv, contact_sync_server: bool) -> Report {
    let (config_check, config) = check_config(config_path);
    let (taskrc_check, taskrc) = check_taskrc(&config, env);
    let (data_check, data_dir) = check_data_dir(&config, &taskrc, env).await;
    let (settings_check, target) = check_sync_settings(&taskrc, &data_dir.unwrap_or_default());
    let server_check = check_sync_server(target, contact_sync_server).await;
    Report {
        checks: vec![
            config_check,
            taskrc_check,
            data_check,
            settings_check,
            server_check,
        ],
    }
}

/// The config file check, and the config the other checks resolve against:
/// the defaults when the file is missing or does not parse.
fn check_config(flag: Option<&str>) -> (Check, Config) {
    let (path, source) = match flag {
        Some(path) => (PathBuf::from(path), "--config"),
        None => match Config::default_config_path() {
            Ok(path) => (path, "default"),
            Err(err) => {
                let mut check = Check::new("Config file", "none", "default");
                check.fail(format!("{err:#}"));
                return (check, Config::default());
            }
        },
    };
    let mut check = Check::new("Config file", path.display(), source);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            if flag.is_some() {
                check.fail("not found; lazytask would ignore it and run on the defaults");
            } else {
                check.note(
                    "not found, so the defaults apply; `lazytask schema init` writes a starter",
                );
            }
            return (check, Config::default());
        }
        Err(err) => {
            check.fail(format!("cannot be read: {err}"));
            return (check, Config::default());
        }
    };
    if !has_schema_header(&contents) {
        check.warn("no #:schema header, so editors cannot complete or validate it; `lazytask schema init` adds one");
    }
    match Config::parse(&contents) {
        Ok(LoadedConfig {
            config,
            unknown_keys,
        }) => {
            if !unknown_keys.is_empty() {
                check.warn(format!("unknown keys: {}", unknown_keys.join(", ")));
            }
            (check, config)
        }
        Err(err) => {
            check.fail(format!("{err:#}; the other checks use the defaults"));
            (check, Config::default())
        }
    }
}

/// The taskrc check, and the taskrc the later checks read: an empty one
/// when it is missing or does not parse.
fn check_taskrc(config: &Config, env: &LaunchEnv) -> (Check, Taskrc) {
    const NAME: &str = "Taskrc";
    let resolved = config
        .taskwarrior
        .resolve_taskrc_path(env.taskrc_var.clone(), env.home.as_deref());
    let ResolvedPath { path, source } = match resolved {
        Ok(Some(resolved)) => resolved,
        Ok(None) => {
            let mut check = Check::new(NAME, "none", "default");
            check.warn("nothing names a taskrc and there is no home directory");
            return (check, Taskrc::default());
        }
        Err(err) => {
            let mut check = Check::new(NAME, "unresolved", "unresolved");
            check.fail(format!("{err:#}"));
            return (check, Taskrc::default());
        }
    };
    let label = match source {
        PathSource::Config => "config taskwarrior.taskrc_path",
        PathSource::EnvVar => "TASKRC",
        PathSource::Taskrc | PathSource::Default => "default",
    };
    let mut check = Check::new(NAME, path.display(), label);
    if !path.exists() {
        let message = "not found, so Taskwarrior's defaults apply";
        if source == PathSource::Default {
            check.note(message);
        } else {
            check.warn(message);
        }
        return (check, Taskrc::default());
    }
    match Taskrc::load(&path, env) {
        Ok(taskrc) => (check, taskrc),
        Err(err) => {
            check.fail(format!("{err:#}"));
            (check, Taskrc::default())
        }
    }
}

/// The data directory check, and the directory when it resolved.
async fn check_data_dir(
    config: &Config,
    taskrc: &Taskrc,
    env: &LaunchEnv,
) -> (Check, Option<PathBuf>) {
    const NAME: &str = "Data directory";
    let resolved = config.taskwarrior.resolve_data_location(
        env.taskdata_var.clone(),
        taskrc,
        env.home.as_deref(),
    );
    let ResolvedPath { path, source } = match resolved {
        Ok(resolved) => resolved,
        Err(err) => {
            let mut check = Check::new(NAME, "unresolved", "unresolved");
            check.fail(format!("{err:#}"));
            return (check, None);
        }
    };
    let source = match source {
        PathSource::Config => "config taskwarrior.data_location",
        PathSource::EnvVar => "TASKDATA",
        PathSource::Taskrc => "taskrc data.location",
        PathSource::Default => "default",
    };
    let mut check = Check::new(NAME, path.display(), source);
    if !path.exists() {
        check.warn("does not exist; lazytask creates it on first start");
    } else if !path.is_dir() {
        check.fail("is not a directory");
    } else if !path.join(REPLICA_FILE).exists() {
        check.warn(format!(
            "has no {REPLICA_FILE}; lazytask creates an empty replica on first start"
        ));
    } else {
        match count_tasks(&path).await {
            Ok(count) => check.note(format!("replica opens, {count} tasks")),
            Err(err) => check.fail(format!("replica does not open: {err:#}")),
        }
    }
    (check, Some(path))
}

/// The SQLite file TaskChampion keeps a replica in.
const REPLICA_FILE: &str = "taskchampion.sqlite3";

/// Opens the replica in `data_dir` read-only, without creating anything,
/// and counts its tasks.
async fn count_tasks(data_dir: &Path) -> anyhow::Result<usize> {
    let storage = SqliteStorage::new(data_dir, AccessMode::ReadOnly, false).await?;
    Ok(Replica::new(storage).all_task_uuids().await?.len())
}

/// A sync target whose settings are valid.
struct SyncTarget {
    description: String,
    settings: SyncSettings,
    config: ServerConfig,
}

/// The sync settings check, and the target it found when its settings are
/// valid.
fn check_sync_settings(taskrc: &Taskrc, data_dir: &Path) -> (Check, Option<SyncTarget>) {
    const NAME: &str = "Sync settings";
    let settings = match taskrc.sync_settings() {
        Ok(Some(settings)) => settings,
        Ok(None) => {
            let mut check = Check::new(NAME, "none", "taskrc");
            check.note("the taskrc names no sync target");
            return (check, None);
        }
        Err(err) => {
            let mut check = Check::new(NAME, "invalid", "taskrc");
            check.fail(format!("{err:#}"));
            return (check, None);
        }
    };
    let description = describe(&settings, data_dir);
    let mut check = Check::new(NAME, &description, "taskrc");
    match settings.server_config(data_dir) {
        Ok(config) => (
            check,
            Some(SyncTarget {
                description,
                settings,
                config,
            }),
        ),
        Err(err) => {
            check.fail(format!("{err:#}"));
            (check, None)
        }
    }
}

/// Names the sync target without its secrets.
fn describe(settings: &SyncSettings, data_dir: &Path) -> String {
    match settings {
        SyncSettings::Local { server_dir } => {
            let dir = server_dir
                .clone()
                .unwrap_or_else(|| data_dir.join("sync-server"));
            format!("local directory {}", dir.display())
        }
        SyncSettings::Server(server) => format!("{} as client {}", server.url, server.client_id),
        SyncSettings::Gcp { bucket, .. } => format!("GCP bucket {bucket}"),
        SyncSettings::Aws { region, bucket, .. } => format!("AWS bucket {bucket} in {region}"),
    }
}

/// How long `--sync` waits for the sync server to answer.
const SERVER_TIMEOUT: Duration = Duration::from_secs(30);

async fn check_sync_server(target: Option<SyncTarget>, contact: bool) -> Check {
    const NAME: &str = "Sync server";
    let Some(target) = target else {
        let mut check = Check::new(NAME, "none", "taskrc");
        check.find(Status::Skip, "no valid sync target to contact");
        return check;
    };
    let mut check = Check::new(NAME, &target.description, "taskrc");
    if let SyncSettings::Local { .. } = target.settings {
        check.find(Status::Skip, "a local directory has no server to contact");
        return check;
    }
    if !contact {
        check.find(
            Status::Skip,
            "not contacted; `lazytask doctor --sync` contacts it",
        );
        return check;
    }
    match tokio::time::timeout(SERVER_TIMEOUT, first_version(target.config)).await {
        Ok(Ok(GetVersionResult::Version { .. })) => {
            check.note("reachable, and the encryption secret decrypts its history")
        }
        Ok(Ok(GetVersionResult::NoSuchVersion)) => {
            check.note("reachable, with no history yet to check the encryption secret against")
        }
        Ok(Err(err)) => check.fail(format!("unreachable or refused: {err:#}")),
        Err(_) => check.fail(format!(
            "no answer within {} seconds",
            SERVER_TIMEOUT.as_secs()
        )),
    }
    check
}

/// Asks the server for its first version, which reads without changing
/// anything and makes it decrypt with the configured secret.
async fn first_version(config: ServerConfig) -> anyhow::Result<GetVersionResult> {
    let mut server = config.into_server().await?;
    Ok(server.get_child_version(Uuid::nil()).await?)
}
