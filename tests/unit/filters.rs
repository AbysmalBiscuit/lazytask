// Unit tests for filter system

use chrono::Utc;
use lazytask::data::filters::TaskFilter;
use lazytask::data::models::{Priority, Task, TaskStatus};

#[test]
fn test_basic_status_filtering() {
    let mut tasks = create_test_tasks();
    
    // Test pending filter
    let pending_filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        ..TaskFilter::default()
    };
    let pending_results = pending_filter.apply(&tasks);
    assert!(pending_results.iter().all(|t| t.status == TaskStatus::Pending));
    
    // Test completed filter
    let completed_filter = TaskFilter {
        status: Some(TaskStatus::Completed),
        ..TaskFilter::default()
    };
    let completed_results = completed_filter.apply(&tasks);
    assert!(completed_results.iter().all(|t| t.status == TaskStatus::Completed));
}

#[test]
fn test_priority_filtering() {
    let tasks = create_test_tasks();
    
    let high_priority_filter = TaskFilter {
        priority: Some(Priority::High),
        ..TaskFilter::default()
    };
    let high_priority_results = high_priority_filter.apply(&tasks);
    
    for task in &high_priority_results {
        assert_eq!(task.priority, Some(Priority::High), 
            "Task '{}' should have high priority", task.description);
    }
    
    assert!(!high_priority_results.is_empty(), "Should find high priority tasks");
}

#[test]
fn test_project_filtering() {
    let tasks = create_test_tasks();
    
    let project_filter = TaskFilter {
        project: Some("test".to_string()),
        ..TaskFilter::default()
    };
    let project_results = project_filter.apply(&tasks);
    
    for task in &project_results {
        assert_eq!(task.project, Some("test".to_string()),
            "Task '{}' should be in test project", task.description);
    }
}

#[test]
fn test_tag_filtering() {
    let tasks = create_test_tasks();
    
    let tag_filter = TaskFilter {
        tags: vec!["urgent".to_string()],
        ..TaskFilter::default()
    };
    let tag_results = tag_filter.apply(&tasks);
    
    for task in &tag_results {
        assert!(task.tags.contains(&"urgent".to_string()),
            "Task '{}' should have 'urgent' tag", task.description);
    }
}

#[test]
fn test_description_filtering() {
    let tasks = create_test_tasks();
    
    let desc_filter = TaskFilter {
        description_contains: Some("test".to_string()),
        ..TaskFilter::default()
    };
    let desc_results = desc_filter.apply(&tasks);
    
    for task in &desc_results {
        assert!(task.description.to_lowercase().contains("test"),
            "Task '{}' should contain 'test'", task.description);
    }
}

#[test]
fn test_active_filtering() {
    let mut tasks = create_test_tasks();
    
    // Make one task active
    tasks[0].start = Some(Utc::now());
    
    let active_filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        is_active: Some(true),
        ..TaskFilter::default()
    };
    let active_results = active_filter.apply(&tasks);
    
    assert_eq!(active_results.len(), 1, "Should find exactly one active task");
    assert!(active_results[0].is_active());
}

#[test]
fn test_overdue_filtering() {
    let mut tasks = create_test_tasks();
    
    // Make one task overdue
    tasks[0].due = Some(Utc::now() - chrono::Duration::days(1));
    tasks[0].status = TaskStatus::Pending;
    
    let overdue_filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        is_overdue: Some(true),
        ..TaskFilter::default()
    };
    let overdue_results = overdue_filter.apply(&tasks);
    
    assert_eq!(overdue_results.len(), 1, "Should find exactly one overdue task");
    assert!(overdue_results[0].is_overdue());
}

#[test]
fn test_combined_filtering() {
    let tasks = create_test_tasks();
    
    let combined_filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        priority: Some(Priority::High),
        project: Some("test".to_string()),
        ..TaskFilter::default()
    };
    
    let combined_results = combined_filter.apply(&tasks);
    
    for task in &combined_results {
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.priority, Some(Priority::High));
        assert_eq!(task.project, Some("test".to_string()));
    }
}

#[test]
fn test_empty_filter() {
    let tasks = create_test_tasks();
    let empty_filter = TaskFilter::default();
    
    // Default filter should show pending tasks only
    let results = empty_filter.apply(&tasks);
    assert!(results.iter().all(|t| t.status == TaskStatus::Pending));
}

// Helper function to create test tasks
fn create_test_tasks() -> Vec<Task> {
    let mut tasks = Vec::new();
    
    // Pending high priority task with project and tags
    let mut task1 = Task::new("High priority test task".to_string());
    task1.status = TaskStatus::Pending;
    task1.priority = Some(Priority::High);
    task1.project = Some("test".to_string());
    task1.tags = vec!["urgent".to_string(), "important".to_string()];
    tasks.push(task1);
    
    // Completed medium priority task
    let mut task2 = Task::new("Completed test task".to_string());
    task2.status = TaskStatus::Completed;
    task2.priority = Some(Priority::Medium);
    task2.project = Some("test".to_string());
    tasks.push(task2);
    
    // Pending low priority task without project
    let mut task3 = Task::new("Low priority pending task".to_string());
    task3.status = TaskStatus::Pending;
    task3.priority = Some(Priority::Low);
    tasks.push(task3);
    
    // Pending task without priority but with tags
    let mut task4 = Task::new("No priority task with tags".to_string());
    task4.status = TaskStatus::Pending;
    task4.tags = vec!["urgent".to_string()];
    tasks.push(task4);
    
    // Deleted task
    let mut task5 = Task::new("Deleted task".to_string());
    task5.status = TaskStatus::Deleted;
    tasks.push(task5);
    
    tasks
}


