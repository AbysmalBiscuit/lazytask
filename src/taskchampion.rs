use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use taskchampion::{Replica, Server, Storage};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TaskChampionIntegration {
    replica: Replica,
    server: Option<Server>,
    data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub uuid: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: Option<Priority>,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub due: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub urgency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Completed,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl TaskChampionIntegration {
    pub fn new(data_dir: Option<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("lazytask")
        });

        // Ensure data directory exists
        std::fs::create_dir_all(&data_dir)
            .context("Failed to create data directory")?;

        let storage = Storage::new(data_dir.clone())
            .context("Failed to create TaskChampion storage")?;

        let replica = Replica::new(Uuid::new_v4(), storage)
            .context("Failed to create TaskChampion replica")?;

        Ok(TaskChampionIntegration {
            replica,
            server: None,
            data_dir,
        })
    }

    pub async fn configure_sync(&mut self, server_url: &str, client_id: &str) -> Result<()> {
        let server = Server::new(server_url, client_id)
            .context("Failed to create TaskChampion server")?;
        
        self.server = Some(server);
        Ok(())
    }

    pub async fn list_tasks(&mut self) -> Result<Vec<Task>> {
        let tasks = self.replica
            .all_tasks()
            .context("Failed to list tasks")?;

        let mut result = Vec::new();
        for task in tasks {
            let task_data = self.replica
                .get_task(task)
                .context("Failed to get task data")?;

            if let Some(task_data) = task_data {
                let tc_task = taskchampion::Task::from(task_data);
                
                let status = match tc_task.status() {
                    taskchampion::Status::Pending => TaskStatus::Pending,
                    taskchampion::Status::Completed => TaskStatus::Completed,
                    taskchampion::Status::Deleted => TaskStatus::Deleted,
                };

                let priority = tc_task.priority().map(|p| match p {
                    taskchampion::Priority::High => Priority::High,
                    taskchampion::Priority::Medium => Priority::Medium,
                    taskchampion::Priority::Low => Priority::Low,
                });

                let task = Task {
                    uuid: task.to_string(),
                    description: tc_task.description().to_string(),
                    status,
                    priority,
                    project: tc_task.project().map(|s| s.to_string()),
                    tags: tc_task.tags().iter().map(|s| s.to_string()).collect(),
                    due: tc_task.due().map(|d| DateTime::from(d)),
                    created: DateTime::from(tc_task.created()),
                    modified: DateTime::from(tc_task.modified()),
                    urgency: tc_task.urgency(),
                };

                result.push(task);
            }
        }

        Ok(result)
    }

    pub async fn add_task(&mut self, description: &str) -> Result<String> {
        let mut builder = self.replica
            .new_task(description)
            .context("Failed to create new task")?;

        let task = builder
            .build()
            .context("Failed to build task")?;

        Ok(task.to_string())
    }

    pub async fn update_task(&mut self, uuid: &str, updates: TaskUpdate) -> Result<()> {
        let task_uuid = uuid.parse::<Uuid>()
            .context("Invalid task UUID")?;

        let mut task_data = self.replica
            .get_task(task_uuid)
            .context("Failed to get task")?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;

        if let Some(description) = updates.description {
            task_data.description = description;
        }

        if let Some(status) = updates.status {
            task_data.status = match status {
                TaskStatus::Pending => taskchampion::Status::Pending,
                TaskStatus::Completed => taskchampion::Status::Completed,
                TaskStatus::Deleted => taskchampion::Status::Deleted,
            };
        }

        if let Some(priority) = updates.priority {
            task_data.priority = priority.map(|p| match p {
                Priority::High => taskchampion::Priority::High,
                Priority::Medium => taskchampion::Priority::Medium,
                Priority::Low => taskchampion::Priority::Low,
            });
        }

        if let Some(project) = updates.project {
            task_data.project = project;
        }

        if let Some(tags) = updates.tags {
            task_data.tags = tags;
        }

        if let Some(due) = updates.due {
            task_data.due = Some(due.into());
        }

        self.replica
            .update_task(task_uuid, task_data)
            .context("Failed to update task")?;

        Ok(())
    }

    pub async fn delete_task(&mut self, uuid: &str) -> Result<()> {
        let task_uuid = uuid.parse::<Uuid>()
            .context("Invalid task UUID")?;

        self.replica
            .delete_task(task_uuid)
            .context("Failed to delete task")?;

        Ok(())
    }

    pub async fn sync(&mut self) -> Result<SyncResult> {
        if let Some(server) = &self.server {
            let sync_result = self.replica
                .sync(server)
                .await
                .context("Failed to sync with server")?;

            Ok(SyncResult {
                uploaded: sync_result.uploaded,
                downloaded: sync_result.downloaded,
                conflicts: sync_result.conflicts,
            })
        } else {
            Err(anyhow::anyhow!("No server configured"))
        }
    }

    pub fn is_sync_configured(&self) -> bool {
        self.server.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct TaskUpdate {
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub priority: Option<Option<Priority>>,
    pub project: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub due: Option<Option<DateTime<Utc>>>,
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflicts: usize,
}


