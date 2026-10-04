// Common utility functions

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use crate::data::models::{Priority, Task, TaskStatus};

/// Expands a leading `~` component to `home`, which is only required when
/// that expansion happens.
pub fn expand_tilde(path: &Path, home: Option<&Path>) -> Result<PathBuf> {
    match path.strip_prefix("~") {
        Ok(rest) => Ok(home.context("Could not find home directory")?.join(rest)),
        Err(_) => Ok(path.to_path_buf()),
    }
}

/// Taskwarrior 3's default urgency coefficients.
mod coefficient {
    pub const NEXT_TAG: f64 = 15.0;
    pub const DUE: f64 = 12.0;
    pub const BLOCKING: f64 = 8.0;
    pub const ACTIVE: f64 = 4.0;
    pub const SCHEDULED: f64 = 5.0;
    pub const AGE: f64 = 2.0;
    pub const ANNOTATIONS: f64 = 1.0;
    pub const TAGS: f64 = 1.0;
    pub const PROJECT: f64 = 1.0;
    pub const BLOCKED: f64 = -5.0;
    pub const WAITING: f64 = -3.0;
    pub const PRIORITY_H: f64 = 6.0;
    pub const PRIORITY_M: f64 = 3.9;
    pub const PRIORITY_L: f64 = 1.8;
    pub const AGE_MAX_DAYS: i64 = 365;
}

const SECONDS_PER_DAY: f64 = 86_400.0;

/// Urgency as Taskwarrior 3 computes it with its default coefficients.
/// `blocked` and `blocking` say whether the task depends on, or is depended
/// on by, another task where neither is completed or deleted.
pub fn calculate_urgency(task: &Task, blocked: bool, blocking: bool, now: DateTime<Utc>) -> f64 {
    let flag = |set: bool| if set { 1.0 } else { 0.0 };
    let count_scale = |count: usize| match count {
        0 => 0.0,
        1 => 0.8,
        2 => 0.9,
        _ => 1.0,
    };
    let is_waiting = task.status == TaskStatus::Pending && task.wait.is_some_and(|w| w > now);

    let priority = match task.priority {
        Some(Priority::High) => coefficient::PRIORITY_H,
        Some(Priority::Medium) => coefficient::PRIORITY_M,
        Some(Priority::Low) => coefficient::PRIORITY_L,
        None => 0.0,
    };

    flag(task.project.is_some()) * coefficient::PROJECT
        + flag(task.start.is_some()) * coefficient::ACTIVE
        + flag(task.scheduled.is_some_and(|s| s < now)) * coefficient::SCHEDULED
        + flag(is_waiting) * coefficient::WAITING
        + flag(blocked) * coefficient::BLOCKED
        + count_scale(task.annotations.len()) * coefficient::ANNOTATIONS
        + count_scale(task.tags.len()) * coefficient::TAGS
        + task.due.map_or(0.0, |due| urgency_due(due, now)) * coefficient::DUE
        + flag(blocking) * coefficient::BLOCKING
        + urgency_age(task.entry, now) * coefficient::AGE
        + flag(task.tags.iter().any(|t| t == "next")) * coefficient::NEXT_TAG
        + priority
}

/// Maps due dates from two weeks out (0.2) to a week overdue (1.0), linearly.
fn urgency_due(due: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let days_overdue = (now - due).num_seconds() as f64 / SECONDS_PER_DAY;
    if days_overdue >= 7.0 {
        1.0
    } else if days_overdue >= -14.0 {
        (days_overdue + 14.0) * 0.8 / 21.0 + 0.2
    } else {
        0.2
    }
}

/// Whole days since entry over the age cap, at most 1.0.
fn urgency_age(entry: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let age_days = (now - entry).num_seconds() / 86_400;
    if age_days > coefficient::AGE_MAX_DAYS {
        1.0
    } else {
        age_days as f64 / coefficient::AGE_MAX_DAYS as f64
    }
}
