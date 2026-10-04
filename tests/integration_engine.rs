// Integration test: full CRUD round-trip against a real TaskChampion replica
// in a temp directory. Exercises add → list → modify → done → delete via the
// public TaskChampionIntegration API.

use lazytask::data::models::{Priority, TaskStatus};
use lazytask::taskchampion::TaskChampionIntegration;
use tempfile::TempDir;

async fn engine() -> (TaskChampionIntegration, TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let engine = TaskChampionIntegration::new(tmp.path().to_path_buf())
        .await
        .expect("engine");
    (engine, tmp)
}

#[tokio::test]
async fn add_and_list_task() {
    let (mut engine, _tmp) = engine().await;

    let uuid = engine
        .add_task(
            "Write report",
            &[("project", "work"), ("priority", "H"), ("+urgent", "")],
        )
        .await
        .expect("add");

    let tasks = engine.list_tasks().await.expect("list");
    assert_eq!(tasks.len(), 1, "exactly one task should exist");

    let t = &tasks[0];
    assert_eq!(t.uuid, uuid);
    assert_eq!(t.description, "Write report");
    assert_eq!(t.status, TaskStatus::Pending);
    assert_eq!(t.project.as_deref(), Some("work"));
    assert_eq!(t.priority, Some(Priority::High));
    assert!(
        t.tags.contains(&"urgent".to_string()),
        "user tag 'urgent' missing from {:?}",
        t.tags
    );
    assert!(
        !t.tags
            .iter()
            .any(|tag| tag.chars().all(|c| c.is_ascii_uppercase())),
        "synthetic tags leaked: {:?}",
        t.tags
    );
}

#[tokio::test]
async fn modify_task_changes_fields() {
    let (mut engine, _tmp) = engine().await;
    let uuid = engine
        .add_task("Old", &[("project", "personal")])
        .await
        .unwrap();

    engine
        .modify_task(
            &uuid,
            &[
                ("description", "New description"),
                ("project", "work"),
                ("priority", "M"),
            ],
        )
        .await
        .expect("modify");

    let tasks = engine.list_tasks().await.unwrap();
    let t = tasks.iter().find(|t| t.uuid == uuid).unwrap();
    assert_eq!(t.description, "New description");
    assert_eq!(t.project.as_deref(), Some("work"));
    assert_eq!(t.priority, Some(Priority::Medium));
}

#[tokio::test]
async fn done_marks_task_completed() {
    let (mut engine, _tmp) = engine().await;
    let uuid = engine.add_task("Finish thing", &[]).await.unwrap();

    engine.done_task(&uuid).await.expect("done");

    let t = engine
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.uuid == uuid)
        .unwrap();
    assert_eq!(t.status, TaskStatus::Completed);
}

#[tokio::test]
async fn delete_marks_task_deleted_not_purged() {
    let (mut engine, _tmp) = engine().await;
    let uuid = engine.add_task("Throwaway", &[]).await.unwrap();

    engine.delete_task(&uuid).await.expect("delete");

    let t = engine
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.uuid == uuid)
        .expect("soft-deleted task should still be visible");
    assert_eq!(t.status, TaskStatus::Deleted);
}

#[tokio::test]
async fn purge_removes_task_completely() {
    let (mut engine, _tmp) = engine().await;
    let uuid = engine.add_task("Bye", &[]).await.unwrap();

    engine.purge_task(&uuid).await.expect("purge");

    let still_present = engine
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .any(|t| t.uuid == uuid);
    assert!(!still_present, "purge should remove the task entirely");
}

#[tokio::test]
async fn tag_add_and_remove_round_trip() {
    let (mut engine, _tmp) = engine().await;
    let uuid = engine
        .add_task("Tag me", &[("+alpha", ""), ("+beta", "")])
        .await
        .unwrap();

    let tags_before: Vec<String> = engine
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.uuid == uuid)
        .unwrap()
        .tags;
    assert!(tags_before.contains(&"alpha".to_string()));
    assert!(tags_before.contains(&"beta".to_string()));

    engine.modify_task(&uuid, &[("-alpha", "")]).await.unwrap();

    let tags_after: Vec<String> = engine
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.uuid == uuid)
        .unwrap()
        .tags;
    assert!(
        !tags_after.contains(&"alpha".to_string()),
        "alpha not removed"
    );
    assert!(
        tags_after.contains(&"beta".to_string()),
        "beta should remain"
    );
}

#[tokio::test]
async fn modify_with_empty_tags_clears_all_user_tags() {
    let (mut engine, _tmp) = engine().await;
    let uuid = engine
        .add_task("Tagged", &[("+one", ""), ("+two", ""), ("+three", "")])
        .await
        .unwrap();

    engine.modify_task(&uuid, &[("tags", "")]).await.unwrap();

    let tags = engine
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.uuid == uuid)
        .unwrap()
        .tags;
    assert!(
        tags.is_empty(),
        "user tags should be cleared, got {:?}",
        tags
    );
}

#[tokio::test]
async fn invalid_uuid_modify_returns_error() {
    let (mut engine, _tmp) = engine().await;
    let err = engine.modify_task("not-a-uuid", &[]).await.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("Invalid task UUID") || msg.contains("UUID"),
        "expected UUID-parse error, got: {}",
        msg
    );
}
