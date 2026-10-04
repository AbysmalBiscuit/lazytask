// TaskChampion replica integration. This is the live data engine for LazyTask.
// Targets the taskchampion 3.0.x API: Replica<S: Storage> with async methods,
// SqliteStorage built directly, ServerConfig::into_server() async.

use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use taskchampion::server::AwsCredentials as TcAwsCredentials;
use taskchampion::storage::AccessMode;
use taskchampion::{Operations, Replica, ServerConfig, SqliteStorage, Status as TcStatus, Tag};
use uuid::Uuid;

use crate::data::models::{Annotation, Priority, Task, TaskStatus};
use crate::utils::helpers::calculate_urgency;

/// Where a replica syncs to, one variant per TaskChampion backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncSettings {
    /// A local sync-server directory, `<data_dir>/sync-server` when `None`.
    Local { server_dir: Option<PathBuf> },
    /// A taskchampion-sync-server; `client_id` must be a UUID.
    Server {
        url: String,
        client_id: String,
        encryption_secret: String,
    },
    /// A Google Cloud Storage bucket, with Application Default Credentials
    /// when `credential_path` is `None`.
    Gcp {
        bucket: String,
        credential_path: Option<String>,
        encryption_secret: String,
    },
    /// An Amazon S3 bucket.
    Aws {
        region: String,
        bucket: String,
        credentials: AwsCredentials,
        encryption_secret: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwsCredentials {
    AccessKey {
        access_key_id: String,
        secret_access_key: String,
    },
    /// A named profile from the AWS config files.
    Profile(String),
    /// The AWS SDK's default credential chain.
    Default,
}

pub struct TaskChampionIntegration {
    replica: Replica<SqliteStorage>,
    sync_settings: Option<SyncSettings>,
    data_dir: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflicts: usize,
}

impl TaskChampionIntegration {
    pub async fn new(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir).context("Failed to create data directory")?;

        let storage = SqliteStorage::new(&data_dir, AccessMode::ReadWrite, true)
            .await
            .context("Failed to open TaskChampion storage")?;

        let replica = Replica::new(storage);

        Ok(TaskChampionIntegration {
            replica,
            sync_settings: None,
            data_dir,
        })
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub async fn list_tasks(&mut self) -> Result<Vec<Task>> {
        let tasks = self
            .replica
            .all_tasks()
            .await
            .context("Failed to load tasks")?;

        let working_set = self
            .replica
            .working_set()
            .await
            .context("Failed to load working set")?;

        let mut result = Vec::with_capacity(tasks.len());
        for (uuid, tc_task) in tasks {
            let mut task = map_task(uuid, &tc_task);
            task.id = working_set
                .by_uuid(uuid)
                .and_then(|i| u32::try_from(i).ok());
            result.push(task);
        }
        apply_urgency(&mut result, Utc::now());
        Ok(result)
    }

    pub async fn add_task(
        &mut self,
        description: &str,
        attributes: &[(&str, &str)],
    ) -> Result<String> {
        let uuid = Uuid::new_v4();
        let mut ops = Operations::new();
        let mut task = self
            .replica
            .create_task(uuid, &mut ops)
            .await
            .context("Failed to create task")?;
        task.set_description(description.to_string(), &mut ops)
            .context("Failed to set task description")?;
        task.set_status(TcStatus::Pending, &mut ops)
            .context("Failed to set task status")?;
        task.set_entry(Some(Utc::now()), &mut ops)
            .context("Failed to set task entry")?;

        apply_attributes(&mut task, attributes, &mut ops)?;

        self.replica
            .commit_operations(ops)
            .await
            .context("Failed to commit new task")?;
        Ok(uuid.to_string())
    }

    pub async fn modify_task(&mut self, uuid: &str, attributes: &[(&str, &str)]) -> Result<()> {
        let task_uuid = uuid.parse::<Uuid>().context("Invalid task UUID")?;
        let mut ops = Operations::new();
        let mut task = self
            .replica
            .get_task(task_uuid)
            .await
            .context("Failed to get task")?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", uuid))?;

        apply_attributes(&mut task, attributes, &mut ops)?;

        self.replica
            .commit_operations(ops)
            .await
            .context("Failed to commit modifications")?;
        Ok(())
    }

    pub async fn done_task(&mut self, uuid: &str) -> Result<()> {
        self.set_status(uuid, TcStatus::Completed).await
    }

    pub async fn delete_task(&mut self, uuid: &str) -> Result<()> {
        // Soft-delete: mark status=deleted. Use `purge_task` for permanent removal.
        self.set_status(uuid, TcStatus::Deleted).await
    }

    pub async fn purge_task(&mut self, uuid: &str) -> Result<()> {
        let task_uuid = uuid.parse::<Uuid>().context("Invalid task UUID")?;
        let mut ops = Operations::new();
        let mut data = self
            .replica
            .get_task_data(task_uuid)
            .await
            .context("Failed to get task")?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", uuid))?;
        data.delete(&mut ops);
        self.replica
            .commit_operations(ops)
            .await
            .context("Failed to purge task")?;
        Ok(())
    }

    async fn set_status(&mut self, uuid: &str, status: TcStatus) -> Result<()> {
        let task_uuid = uuid.parse::<Uuid>().context("Invalid task UUID")?;
        let mut ops = Operations::new();
        let mut task = self
            .replica
            .get_task(task_uuid)
            .await
            .context("Failed to get task")?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", uuid))?;
        task.set_status(status, &mut ops)
            .context("Failed to set status")?;
        self.replica
            .commit_operations(ops)
            .await
            .context("Failed to commit status change")?;
        Ok(())
    }

    pub fn configure_sync(&mut self, settings: SyncSettings) -> Result<()> {
        // Validate the server settings by trying to build a ServerConfig.
        let _ = build_server_config(&settings, &self.data_dir)?;
        self.sync_settings = Some(settings);
        Ok(())
    }

    pub fn is_sync_configured(&self) -> bool {
        self.sync_settings.is_some()
    }

    pub fn sync_settings(&self) -> Option<&SyncSettings> {
        self.sync_settings.as_ref()
    }

    pub async fn sync(&mut self) -> Result<SyncResult> {
        let settings = self
            .sync_settings
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sync not configured"))?
            .clone();

        let pending_before = self.replica.num_local_operations().await.unwrap_or(0);

        let mut server = build_server_config(&settings, &self.data_dir)?
            .into_server()
            .await
            .context("Failed to construct sync server")?;
        self.replica
            .sync(&mut server, false)
            .await
            .context("Failed to sync with server")?;

        let pending_after = self.replica.num_local_operations().await.unwrap_or(0);
        let uploaded = pending_before.saturating_sub(pending_after);

        Ok(SyncResult {
            uploaded,
            downloaded: 0, // TaskChampion sync doesn't expose download counts.
            conflicts: 0,
        })
    }
}

fn build_server_config(settings: &SyncSettings, data_dir: &Path) -> Result<ServerConfig> {
    Ok(match settings.clone() {
        SyncSettings::Local { server_dir } => {
            let server_dir = server_dir.unwrap_or_else(|| data_dir.join("sync-server"));
            std::fs::create_dir_all(&server_dir)
                .with_context(|| format!("Failed to create sync server dir: {:?}", server_dir))?;
            ServerConfig::Local { server_dir }
        }
        SyncSettings::Server {
            url,
            client_id,
            encryption_secret,
        } => ServerConfig::Remote {
            url,
            client_id: client_id
                .parse::<Uuid>()
                .context("Sync client_id must be a UUID")?,
            encryption_secret: encryption_secret.into_bytes(),
        },
        SyncSettings::Gcp {
            bucket,
            credential_path,
            encryption_secret,
        } => ServerConfig::Gcp {
            bucket,
            credential_path,
            encryption_secret: encryption_secret.into_bytes(),
        },
        SyncSettings::Aws {
            region,
            bucket,
            credentials,
            encryption_secret,
        } => ServerConfig::Aws {
            region: Some(region),
            bucket,
            endpoint_url: None,
            force_path_style: false,
            credentials: match credentials {
                AwsCredentials::AccessKey {
                    access_key_id,
                    secret_access_key,
                } => TcAwsCredentials::AccessKey {
                    access_key_id,
                    secret_access_key,
                },
                AwsCredentials::Profile(profile_name) => TcAwsCredentials::Profile { profile_name },
                AwsCredentials::Default => TcAwsCredentials::Default,
            },
            encryption_secret: encryption_secret.into_bytes(),
        },
    })
}

fn apply_attributes(
    task: &mut taskchampion::Task,
    attributes: &[(&str, &str)],
    ops: &mut Operations,
) -> Result<()> {
    for (key, value) in attributes {
        match *key {
            "description" => {
                if !value.is_empty() {
                    task.set_description((*value).to_string(), ops)
                        .context("Failed to set description")?;
                }
            }
            "project" => {
                let v = if value.is_empty() {
                    None
                } else {
                    Some((*value).to_string())
                };
                task.set_value("project", v, ops)
                    .context("Failed to set project")?;
            }
            "priority" => {
                let v = if value.is_empty() {
                    None
                } else {
                    Some((*value).to_string())
                };
                task.set_value("priority", v, ops)
                    .context("Failed to set priority")?;
            }
            "due" => {
                let v = if value.is_empty() {
                    None
                } else {
                    Some(parse_due_date(value)?)
                };
                task.set_due(v, ops).context("Failed to set due")?;
            }
            "tags" if value.is_empty() => {
                // Clear all user tags.
                let existing: Vec<Tag> = task.get_tags().filter(|t| !t.is_synthetic()).collect();
                for tag in existing {
                    task.remove_tag(&tag, ops).context("Failed to remove tag")?;
                }
            }
            other if other.starts_with('+') => {
                let raw = &other[1..];
                let tag: Tag = raw
                    .try_into()
                    .map_err(|e| anyhow::anyhow!("Invalid tag '{}': {}", raw, e))?;
                task.add_tag(&tag, ops).context("Failed to add tag")?;
            }
            other if other.starts_with('-') => {
                let raw = &other[1..];
                let tag: Tag = raw
                    .try_into()
                    .map_err(|e| anyhow::anyhow!("Invalid tag '{}': {}", raw, e))?;
                task.remove_tag(&tag, ops).context("Failed to remove tag")?;
            }
            _ => {
                // Treat any other key as a UDA passthrough.
                let v = if value.is_empty() {
                    None
                } else {
                    Some((*value).to_string())
                };
                task.set_value(*key, v, ops)
                    .context("Failed to set attribute")?;
            }
        }
    }
    Ok(())
}

fn parse_due_date(value: &str) -> Result<chrono::DateTime<Utc>> {
    // Accept YYYY-MM-DD (treated as end-of-day UTC) or RFC3339.
    if let Ok(naive) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let dt = naive
            .and_hms_opt(23, 59, 59)
            .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
        return Ok(chrono::DateTime::from_naive_utc_and_offset(dt, Utc));
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| anyhow::anyhow!("Invalid due date '{}': {}", value, e))
}

fn map_task(uuid: Uuid, tc: &taskchampion::Task) -> Task {
    let status = match tc.get_status() {
        TcStatus::Pending => TaskStatus::Pending,
        TcStatus::Completed => TaskStatus::Completed,
        TcStatus::Deleted => TaskStatus::Deleted,
        TcStatus::Recurring => TaskStatus::Recurring,
        TcStatus::Unknown(_) => TaskStatus::Pending,
    };

    let priority = match tc.get_priority() {
        "H" => Some(Priority::High),
        "M" => Some(Priority::Medium),
        "L" => Some(Priority::Low),
        _ => None,
    };

    let tags: Vec<String> = tc
        .get_tags()
        .filter(|t| !t.is_synthetic())
        .map(|t| t.to_string())
        .collect();

    let project = tc.get_value("project").map(|s| s.to_string());

    let entry = tc.get_entry().unwrap_or_else(Utc::now);
    let modified = tc.get_modified();
    let due = tc.get_due();
    let wait = tc.get_wait();

    let start = tc.get_value("start").and_then(|s| parse_unix_or_rfc3339(s));

    let end = tc.get_value("end").and_then(|s| parse_unix_or_rfc3339(s));

    Task {
        id: None,
        uuid: uuid.to_string(),
        status,
        description: tc.get_description().to_string(),
        project,
        priority,
        due,
        entry,
        modified,
        end,
        start,
        wait,
        scheduled: tc.get_value("scheduled").and_then(parse_unix_or_rfc3339),
        until: None,
        depends: tc.get_dependencies().map(|u| u.to_string()).collect(),
        tags,
        annotations: tc
            .get_annotations()
            .map(|a| Annotation {
                entry: a.entry,
                description: a.description,
            })
            .collect(),
        urgency: 0.0,
        udas: std::collections::HashMap::new(),
    }
}

/// Sets each task's urgency. Blocking follows Taskwarrior: a dependency
/// counts between working-set tasks when neither is completed or deleted.
fn apply_urgency(tasks: &mut [Task], now: chrono::DateTime<Utc>) {
    let is_open = |t: &Task| {
        t.id.is_some() && !matches!(t.status, TaskStatus::Completed | TaskStatus::Deleted)
    };
    let open: HashSet<String> = tasks
        .iter()
        .filter(|t| is_open(t))
        .map(|t| t.uuid.clone())
        .collect();
    let blocking: HashSet<String> = tasks
        .iter()
        .filter(|t| is_open(t))
        .flat_map(|t| t.depends.iter().filter(|d| open.contains(*d)).cloned())
        .collect();
    for task in tasks.iter_mut() {
        let blocked = is_open(task) && task.depends.iter().any(|d| open.contains(d));
        task.urgency = calculate_urgency(task, blocked, blocking.contains(&task.uuid), now);
    }
}

fn parse_unix_or_rfc3339(s: &str) -> Option<chrono::DateTime<Utc>> {
    if let Ok(ts) = s.parse::<i64>() {
        return chrono::DateTime::from_timestamp(ts, 0);
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
