// Common utility functions

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Expands a leading `~` component to `home`, which is only required when
/// that expansion happens.
pub fn expand_tilde(path: &Path, home: Option<&Path>) -> Result<PathBuf> {
    match path.strip_prefix("~") {
        Ok(rest) => Ok(home.context("Could not find home directory")?.join(rest)),
        Err(_) => Ok(path.to_path_buf()),
    }
}

pub fn calculate_urgency(task: &crate::data::models::Task) -> f64 {
    let mut urgency = 0.0;

    // Base urgency
    urgency += 1.0;

    // Priority urgency
    if let Some(priority) = &task.priority {
        match priority {
            crate::data::models::Priority::High => urgency += 6.0,
            crate::data::models::Priority::Medium => urgency += 3.9,
            crate::data::models::Priority::Low => urgency += 1.8,
        }
    }

    // Project urgency
    if task.project.is_some() {
        urgency += 1.0;
    }

    // Active task urgency
    if task.is_active() {
        urgency += 4.0;
    }

    // Tags urgency
    urgency += task.tags.len() as f64 * 1.0;

    // Due date urgency
    if let Some(due) = task.due {
        let now = chrono::Utc::now();
        let days_until_due = (due - now).num_days();

        if days_until_due < 0 {
            // Overdue
            urgency += 12.0;
        } else if days_until_due < 7 {
            // Due this week
            urgency += 5.0;
        } else if days_until_due < 30 {
            // Due this month
            urgency += 2.0;
        }
    }

    urgency
}
