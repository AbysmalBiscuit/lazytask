// Opt-in integration test that exercises the *real* taskchampion-sync-server
// HTTP protocol path. Skipped automatically when env vars are absent so the
// test suite still runs cleanly in CI.
//
// To run:
//
//   CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')
//   podman run -d --name=lazytask-sync-test -p 8810:8080 \
//     -e CLIENT_ID=$CLIENT_ID \
//     ghcr.io/gothenburgbitfactory/taskchampion-sync-server:0.7.1
//
//   LAZYTASK_REMOTE_SYNC_URL=http://localhost:8810 \
//   LAZYTASK_REMOTE_CLIENT_ID=$CLIENT_ID \
//   cargo test --test integration_remote_sync -- --nocapture

use lazytask::data::models::TaskStatus;
use lazytask::taskchampion::{SyncSettings, TaskChampionIntegration};

fn env_settings() -> Option<(String, String, String)> {
    let url = std::env::var("LAZYTASK_REMOTE_SYNC_URL").ok()?;
    let client_id = std::env::var("LAZYTASK_REMOTE_CLIENT_ID").ok()?;
    let secret = std::env::var("LAZYTASK_REMOTE_SECRET")
        .unwrap_or_else(|_| "lazytask-test-secret".to_string());
    Some((url, client_id, secret))
}

#[tokio::test]
async fn remote_sync_round_trip() {
    let Some((url, client_id, secret)) = env_settings() else {
        eprintln!(
            "SKIPPED: set LAZYTASK_REMOTE_SYNC_URL + LAZYTASK_REMOTE_CLIENT_ID to run \
             this test against a live taskchampion-sync-server"
        );
        return;
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let mut a = TaskChampionIntegration::new(tmp.path().join("a"))
        .await
        .expect("replica A");
    let mut b = TaskChampionIntegration::new(tmp.path().join("b"))
        .await
        .expect("replica B");

    let settings = SyncSettings::Server {
        url: url.clone(),
        client_id: client_id.clone(),
        encryption_secret: secret.clone(),
    };
    a.configure_sync(settings.clone()).expect("configure A");
    b.configure_sync(settings).expect("configure B");

    let uuid_a = a
        .add_task(
            "Remote-sync round trip",
            &[("project", "lazytask"), ("priority", "H")],
        )
        .await
        .expect("add_task on A");

    a.sync().await.expect("A.sync push");
    b.sync().await.expect("B.sync pull");

    let on_b = b
        .list_tasks()
        .await
        .expect("list on B")
        .into_iter()
        .find(|t| t.uuid == uuid_a)
        .expect("B should have pulled A's task");
    assert_eq!(on_b.description, "Remote-sync round trip");
    assert_eq!(on_b.project.as_deref(), Some("lazytask"));
    assert_eq!(on_b.status, TaskStatus::Pending);

    b.done_task(&uuid_a).await.expect("done on B");
    b.sync().await.expect("B.sync push completion");
    a.sync().await.expect("A.sync pull completion");

    let on_a = a
        .list_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.uuid == uuid_a)
        .unwrap();
    assert_eq!(on_a.status, TaskStatus::Completed);

    // Cleanup so successive test runs don't pile up data on the server.
    a.purge_task(&uuid_a).await.ok();
    a.sync().await.ok();
}

#[tokio::test]
async fn remote_sync_rejects_unauthorized_client_id() {
    let Some((url, _, secret)) = env_settings() else {
        eprintln!("SKIPPED: remote sync env vars not set");
        return;
    };

    let bogus_id = uuid::Uuid::new_v4().to_string();

    let tmp = tempfile::tempdir().expect("tempdir");
    let mut engine = TaskChampionIntegration::new(tmp.path().to_path_buf())
        .await
        .expect("engine");

    engine
        .configure_sync(SyncSettings::Server {
            url,
            client_id: bogus_id,
            encryption_secret: secret,
        })
        .expect("configure");

    engine
        .add_task("should never reach the server", &[])
        .await
        .unwrap();
    let err = engine
        .sync()
        .await
        .expect_err("server must reject unauthorized client");
    let msg = format!("{:#}", err);
    eprintln!("expected sync failure: {}", msg);
    assert!(!msg.is_empty(), "expected non-empty error message");
}
