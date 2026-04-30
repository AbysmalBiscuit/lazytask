// Integration test: two replicas synchronizing through a shared TaskChampion
// `Local` sync-server directory. This exercises the same code path the live
// app uses (TaskChampionIntegration::sync) end-to-end without needing a
// running taskchampion-sync-server.

use lazytask::taskchampion::{SyncSettings, TaskChampionIntegration};

#[tokio::test]
async fn two_replicas_sync_through_local_server() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let server_dir = tmp.path().join("server");
    let dir_a = tmp.path().join("replica_a");
    let dir_b = tmp.path().join("replica_b");

    let mut a = TaskChampionIntegration::new(Some(dir_a.clone()))
        .await
        .unwrap();
    a.configure_sync(SyncSettings {
        local_server_dir: Some(server_dir.clone()),
        ..Default::default()
    })
    .expect("configure_sync A");
    assert!(a.is_sync_configured(), "replica A should be configured");

    let uuid_a = a
        .add_task("Hello from A", &[("project", "shared")])
        .await
        .expect("add_task on A");
    a.sync().await.expect("sync A pushes task to local server");

    let mut b = TaskChampionIntegration::new(Some(dir_b.clone()))
        .await
        .unwrap();
    b.configure_sync(SyncSettings {
        local_server_dir: Some(server_dir.clone()),
        ..Default::default()
    })
    .expect("configure_sync B");

    assert!(b.list_tasks().await.unwrap().is_empty());

    b.sync().await.expect("sync B pulls from local server");

    let tasks_b = b.list_tasks().await.expect("list_tasks on B");
    assert_eq!(tasks_b.len(), 1, "replica B should see 1 task after sync");
    let pulled = &tasks_b[0];
    assert_eq!(pulled.uuid, uuid_a);
    assert_eq!(pulled.description, "Hello from A");
    assert_eq!(pulled.project.as_deref(), Some("shared"));
}

#[tokio::test]
async fn bidirectional_sync_round_trip() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let server_dir = tmp.path().join("server");

    let mut a = TaskChampionIntegration::new(Some(tmp.path().join("a")))
        .await
        .unwrap();
    let mut b = TaskChampionIntegration::new(Some(tmp.path().join("b")))
        .await
        .unwrap();

    let settings = SyncSettings {
        local_server_dir: Some(server_dir.clone()),
        ..Default::default()
    };
    a.configure_sync(settings.clone()).unwrap();
    b.configure_sync(settings).unwrap();

    let t1 = a.add_task("Task one", &[]).await.unwrap();
    a.sync().await.unwrap();

    b.sync().await.unwrap();
    let t2 = b.add_task("Task two", &[]).await.unwrap();
    b.sync().await.unwrap();

    a.sync().await.unwrap();

    let mut a_uuids: Vec<String> = a
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.uuid)
        .collect();
    a_uuids.sort();
    let mut want = vec![t1.clone(), t2.clone()];
    want.sort();
    assert_eq!(
        a_uuids, want,
        "A should see both tasks after bidirectional sync"
    );

    let mut b_uuids: Vec<String> = b
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.uuid)
        .collect();
    b_uuids.sort();
    assert_eq!(b_uuids, want, "B should also see both tasks");
}

#[tokio::test]
async fn sync_propagates_done_status() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let server_dir = tmp.path().join("server");
    let mut a = TaskChampionIntegration::new(Some(tmp.path().join("a")))
        .await
        .unwrap();
    let mut b = TaskChampionIntegration::new(Some(tmp.path().join("b")))
        .await
        .unwrap();

    let settings = SyncSettings {
        local_server_dir: Some(server_dir.clone()),
        ..Default::default()
    };
    a.configure_sync(settings.clone()).unwrap();
    b.configure_sync(settings).unwrap();

    let uuid = a.add_task("Mark me done", &[]).await.unwrap();
    a.sync().await.unwrap();
    b.sync().await.unwrap();

    b.done_task(&uuid).await.unwrap();
    b.sync().await.unwrap();

    a.sync().await.unwrap();
    let task_on_a = a
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.uuid == uuid)
        .expect("task should still exist on A");
    assert_eq!(
        task_on_a.status,
        lazytask::data::models::TaskStatus::Completed,
        "completion should propagate via sync"
    );
}

#[tokio::test]
async fn sync_without_configuration_fails_cleanly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut engine = TaskChampionIntegration::new(Some(tmp.path().to_path_buf()))
        .await
        .unwrap();
    assert!(!engine.is_sync_configured());

    let err = engine.sync().await.unwrap_err();
    assert!(
        format!("{}", err).contains("Sync not configured"),
        "unexpected error: {}",
        err
    );
}
