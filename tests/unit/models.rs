// Unit tests for data models

use chrono::Utc;
use lazytask::data::models::{Priority, Task, TaskStatus};
use serde_json::json;

#[test]
fn test_task_creation() {
    let task = Task::new("Test task".to_string());

    assert_eq!(task.description, "Test task");
    assert_eq!(task.status, TaskStatus::Pending);
    assert!(task.id.is_none());
    assert!(!task.uuid.is_empty());
    assert!(task.project.is_none());
    assert!(task.priority.is_none());
    assert!(task.tags.is_empty());
    assert!(task.annotations.is_empty());
}

#[test]
fn test_task_from_json() {
    let json_data = json!({
        "uuid": "test-uuid-123",
        "description": "Test task from JSON",
        "status": "pending",
        "project": "test_project",
        "priority": "H",
        "entry": "2024-01-01T10:00:00Z",
        "tags": ["urgent", "important"]
    });

    let task = Task::from_json(&json_data).expect("Should parse valid JSON");

    assert_eq!(task.uuid, "test-uuid-123");
    assert_eq!(task.description, "Test task from JSON");
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.project, Some("test_project".to_string()));
    assert_eq!(task.priority, Some(Priority::High));
    assert_eq!(task.tags.len(), 2);
    assert!(task.tags.contains(&"urgent".to_string()));
    assert!(task.tags.contains(&"important".to_string()));
}

#[test]
fn test_task_from_json_taskwarrior_compact_date() {
    // Taskwarrior emits dates as YYYYMMDDTHHMMSSZ. Make sure we accept that
    // format alongside RFC3339 — this was previously silently rejected.
    let json_data = json!({
        "uuid": "tw-uuid",
        "description": "Compact-date task",
        "status": "pending",
        "entry": "20251007T192937Z",
        "due":   "20251010T120000Z"
    });
    let task = Task::from_json(&json_data).expect("Should parse compact dates");
    assert_eq!(
        task.entry.format("%Y-%m-%d %H:%M:%S").to_string(),
        "2025-10-07 19:29:37"
    );
    let due = task.due.expect("due should be parsed");
    assert_eq!(
        due.format("%Y-%m-%d %H:%M:%S").to_string(),
        "2025-10-10 12:00:00"
    );
}

#[test]
fn test_task_status_conversion() {
    assert_eq!(TaskStatus::from_str("pending"), TaskStatus::Pending);
    assert_eq!(TaskStatus::from_str("completed"), TaskStatus::Completed);
    assert_eq!(TaskStatus::from_str("deleted"), TaskStatus::Deleted);
    assert_eq!(TaskStatus::from_str("waiting"), TaskStatus::Waiting);
    assert_eq!(TaskStatus::from_str("recurring"), TaskStatus::Recurring);
    assert_eq!(TaskStatus::from_str("invalid"), TaskStatus::Pending); // Default fallback
}

#[test]
fn test_priority_conversion() {
    assert_eq!(Priority::from_str("H"), Some(Priority::High));
    assert_eq!(Priority::from_str("M"), Some(Priority::Medium));
    assert_eq!(Priority::from_str("L"), Some(Priority::Low));
    assert_eq!(Priority::from_str("invalid"), None);

    assert_eq!(Priority::High.as_str(), "H");
    assert_eq!(Priority::Medium.as_str(), "M");
    assert_eq!(Priority::Low.as_str(), "L");

    assert_eq!(Priority::High.as_char(), 'H');
    assert_eq!(Priority::Medium.as_char(), 'M');
    assert_eq!(Priority::Low.as_char(), 'L');
}

#[test]
fn test_task_computed_properties() {
    // Test overdue task
    let mut overdue_task = Task::new("Overdue task".to_string());
    overdue_task.due = Some(Utc::now() - chrono::Duration::days(1));
    assert!(overdue_task.is_overdue());

    // Test future task
    let mut future_task = Task::new("Future task".to_string());
    future_task.due = Some(Utc::now() + chrono::Duration::days(1));
    assert!(!future_task.is_overdue());

    // Test active task
    let mut active_task = Task::new("Active task".to_string());
    active_task.start = Some(Utc::now());
    assert!(active_task.is_active());

    // Test blocked task
    let mut blocked_task = Task::new("Blocked task".to_string());
    blocked_task.depends = vec!["other-uuid".to_string()];
    assert!(blocked_task.is_blocked());
}

// Expected values follow Taskwarrior 3's urgency formula and default
// coefficients (src/Task.cpp and src/Context.cpp in Taskwarrior).
mod urgency {
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use lazytask::data::models::{Annotation, Priority, Task, TaskStatus};
    use lazytask::utils::helpers::calculate_urgency;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap()
    }

    fn fresh_task() -> Task {
        let mut task = Task::new("t".to_string());
        task.entry = now();
        task
    }

    fn urgency(task: &Task) -> f64 {
        calculate_urgency(task, false, false, now())
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "urgency {actual}, expected {expected}"
        );
    }

    #[test]
    fn bare_new_task_is_zero() {
        assert_close(urgency(&fresh_task()), 0.0);
    }

    #[test]
    fn priority_project_and_tags_add_up() {
        let mut task = fresh_task();
        task.priority = Some(Priority::High);
        task.project = Some("home".to_string());
        task.tags = vec!["next".to_string(), "errand".to_string()];
        // priority H 6.0 + project 1.0 + two tags 0.9 + next tag 15.0
        assert_close(urgency(&task), 22.9);
    }

    #[test]
    fn medium_and_low_priority_coefficients() {
        let mut task = fresh_task();
        task.priority = Some(Priority::Medium);
        assert_close(urgency(&task), 3.9);
        task.priority = Some(Priority::Low);
        assert_close(urgency(&task), 1.8);
    }

    #[test]
    fn due_scales_from_two_weeks_out_to_a_week_overdue() {
        let mut task = fresh_task();
        task.due = Some(now() - Duration::days(7));
        assert_close(urgency(&task), 12.0);
        task.due = Some(now());
        assert_close(urgency(&task), 8.8);
        task.due = Some(now() + Duration::days(14));
        assert_close(urgency(&task), 2.4);
        task.due = Some(now() + Duration::days(60));
        assert_close(urgency(&task), 2.4);
    }

    #[test]
    fn age_grows_to_its_cap_at_365_days() {
        let mut task = fresh_task();
        task.entry = now() - Duration::days(73);
        assert_close(urgency(&task), 0.4);
        task.entry = now() - Duration::days(73) + Duration::hours(1);
        assert_close(urgency(&task), 2.0 * 72.0 / 365.0);
        task.entry = now() - Duration::days(400);
        assert_close(urgency(&task), 2.0);
    }

    #[test]
    fn active_scheduled_and_waiting() {
        let mut active = fresh_task();
        active.start = Some(now() - Duration::hours(1));
        assert_close(urgency(&active), 4.0);

        let mut scheduled = fresh_task();
        scheduled.scheduled = Some(now() - Duration::hours(1));
        assert_close(urgency(&scheduled), 5.0);
        scheduled.scheduled = Some(now() + Duration::hours(1));
        assert_close(urgency(&scheduled), 0.0);

        let mut waiting = fresh_task();
        waiting.wait = Some(now() + Duration::days(1));
        assert_close(urgency(&waiting), -3.0);
        waiting.status = TaskStatus::Completed;
        assert_close(urgency(&waiting), 0.0);
    }

    #[test]
    fn annotations_count_up_to_three() {
        let mut task = fresh_task();
        let note = Annotation {
            entry: now(),
            description: "note".to_string(),
        };
        for (count, expected) in [(1, 0.8), (2, 0.9), (3, 1.0), (4, 1.0)] {
            task.annotations = vec![note.clone(); count];
            assert_close(urgency(&task), expected);
        }
    }

    #[test]
    fn blocked_and_blocking() {
        let task = fresh_task();
        assert_close(calculate_urgency(&task, true, false, now()), -5.0);
        assert_close(calculate_urgency(&task, false, true, now()), 8.0);
    }
}

#[test]
fn test_json_parsing_edge_cases() {
    // Test minimal JSON
    let minimal_json = json!({
        "uuid": "minimal-uuid",
        "description": "Minimal task"
    });

    let task = Task::from_json(&minimal_json).expect("Should parse minimal JSON");
    assert_eq!(task.uuid, "minimal-uuid");
    assert_eq!(task.description, "Minimal task");
    assert_eq!(task.status, TaskStatus::Pending); // Default
    assert!(task.project.is_none());
    assert!(task.priority.is_none());

    // Test JSON with invalid priority
    let invalid_priority_json = json!({
        "uuid": "invalid-pri-uuid",
        "description": "Invalid priority task",
        "priority": "INVALID"
    });

    let task_invalid_pri =
        Task::from_json(&invalid_priority_json).expect("Should handle invalid priority");
    assert!(task_invalid_pri.priority.is_none());
}

#[test]
fn test_task_equality_and_cloning() {
    let task1 = Task::new("Test task".to_string());
    let task2 = task1.clone();

    assert_eq!(task1.description, task2.description);
    assert_eq!(task1.status, task2.status);
    assert_eq!(task1.uuid, task2.uuid);

    // Modify clone to ensure they're independent
    let mut task3 = task1.clone();
    task3.description = "Modified description".to_string();

    assert_ne!(task1.description, task3.description);
}
