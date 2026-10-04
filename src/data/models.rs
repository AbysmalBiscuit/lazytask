use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Option<u32>,
    pub uuid: String,
    pub status: TaskStatus,
    pub description: String,
    pub project: Option<String>,
    pub priority: Option<Priority>,
    pub due: Option<DateTime<Utc>>,
    pub entry: DateTime<Utc>,
    pub modified: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub start: Option<DateTime<Utc>>,
    pub wait: Option<DateTime<Utc>>,
    pub scheduled: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub depends: Vec<String>,
    pub tags: Vec<String>,
    pub annotations: Vec<Annotation>,
    pub urgency: f64,
    pub udas: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Completed,
    Deleted,
    Waiting,
    Recurring,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub entry: DateTime<Utc>,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub task_count: u32,
    pub completed_count: u32,
    pub pending_count: u32,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub task_count: u32,
}

impl Task {
    pub fn new(description: String) -> Self {
        Task {
            id: None,
            uuid: uuid::Uuid::new_v4().to_string(),
            status: TaskStatus::Pending,
            description,
            project: None,
            priority: None,
            due: None,
            entry: Utc::now(),
            modified: None,
            end: None,
            start: None,
            wait: None,
            scheduled: None,
            until: None,
            depends: Vec::new(),
            tags: Vec::new(),
            annotations: Vec::new(),
            urgency: 0.0,
            udas: HashMap::new(),
        }
    }

    pub fn from_json(json: &Value) -> Result<Self> {
        let id = json.get("id").and_then(|v| v.as_u64()).map(|v| v as u32);

        let uuid = json
            .get("uuid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Task UUID is required"))?
            .to_string();

        let status = json
            .get("status")
            .and_then(|v| v.as_str())
            .map(TaskStatus::from_str)
            .unwrap_or(TaskStatus::Pending);

        let description = json
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Task description is required"))?
            .to_string();

        let project = json
            .get("project")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let priority = json
            .get("priority")
            .and_then(|v| v.as_str())
            .and_then(Priority::from_str);

        let entry = json
            .get("entry")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s))
            .unwrap_or_else(Utc::now);

        let due = json
            .get("due")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s));

        let modified = json
            .get("modified")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s));

        let start = json
            .get("start")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s));

        let end = json
            .get("end")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s));

        let wait = json
            .get("wait")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s));

        let scheduled = json
            .get("scheduled")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s));

        let until = json
            .get("until")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_taskwarrior_date(s));

        let tags = json
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_else(Vec::new);

        let annotations = json
            .get("annotations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| Annotation::from_json(v).ok())
                    .collect()
            })
            .unwrap_or_else(Vec::new);

        let urgency = json.get("urgency").and_then(|v| v.as_f64()).unwrap_or(0.0);

        Ok(Task {
            id,
            uuid,
            status,
            description,
            project,
            priority,
            due,
            entry,
            modified,
            end,
            start,
            wait,
            scheduled,
            until,
            depends: Vec::new(),
            tags,
            annotations,
            urgency,
            udas: HashMap::new(),
        })
    }

    pub fn is_active(&self) -> bool {
        self.start.is_some() && self.status == TaskStatus::Pending
    }

    pub fn is_overdue(&self) -> bool {
        if let Some(due) = self.due {
            due < Utc::now() && self.status == TaskStatus::Pending
        } else {
            false
        }
    }

    pub fn is_blocked(&self) -> bool {
        !self.depends.is_empty()
    }

    fn parse_taskwarrior_date(date_str: &str) -> Option<DateTime<Utc>> {
        parse_taskwarrior_datetime(date_str)
    }
}

fn parse_taskwarrior_datetime(date_str: &str) -> Option<DateTime<Utc>> {
    let trimmed = date_str.trim();

    // First try RFC3339 (e.g. 2024-01-01T10:00:00Z or with offset)
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Utc));
    }

    // Then Taskwarrior compact form: YYYYMMDDTHHMMSSZ (16 chars)
    if trimmed.len() == 16 && trimmed.ends_with('Z') && trimmed.as_bytes().get(8) == Some(&b'T') {
        let formatted = format!(
            "{}-{}-{}T{}:{}:{}Z",
            &trimmed[0..4],
            &trimmed[4..6],
            &trimmed[6..8],
            &trimmed[9..11],
            &trimmed[11..13],
            &trimmed[13..15]
        );
        if let Ok(dt) = DateTime::parse_from_rfc3339(&formatted) {
            return Some(dt.with_timezone(&Utc));
        }
    }

    // Unix timestamp (seconds since epoch)
    if let Ok(ts) = trimmed.parse::<i64>() {
        if let Some(dt) = DateTime::from_timestamp(ts, 0) {
            return Some(dt);
        }
    }

    None
}

impl TaskStatus {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => TaskStatus::Pending,
            "completed" => TaskStatus::Completed,
            "deleted" => TaskStatus::Deleted,
            "waiting" => TaskStatus::Waiting,
            "recurring" => TaskStatus::Recurring,
            _ => TaskStatus::Pending,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Completed => "completed",
            TaskStatus::Deleted => "deleted",
            TaskStatus::Waiting => "waiting",
            TaskStatus::Recurring => "recurring",
        }
    }

    /// Capitalized name for display.
    pub fn label(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "Pending",
            TaskStatus::Completed => "Completed",
            TaskStatus::Deleted => "Deleted",
            TaskStatus::Waiting => "Waiting",
            TaskStatus::Recurring => "Recurring",
        }
    }
}

impl Priority {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "H" => Some(Priority::High),
            "M" => Some(Priority::Medium),
            "L" => Some(Priority::Low),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::High => "H",
            Priority::Medium => "M",
            Priority::Low => "L",
        }
    }

    pub fn as_char(&self) -> char {
        match self {
            Priority::High => 'H',
            Priority::Medium => 'M',
            Priority::Low => 'L',
        }
    }
}

impl Annotation {
    pub fn from_json(json: &Value) -> Result<Self> {
        let entry = json
            .get("entry")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_taskwarrior_datetime(s))
            .ok_or_else(|| anyhow::anyhow!("Annotation entry time is required"))?;

        let description = json
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Annotation description is required"))?
            .to_string();

        Ok(Annotation { entry, description })
    }
}
