// Drives the real App event loop on a TestBackend: keys through the input
// channel, replica changes through the file watcher, remote ones via auto-sync.

use std::time::Duration;

mod common;

use common::Driver;
use crossterm::event::KeyCode;
use lazytask::app::Session;
use lazytask::config::Config;
use lazytask::taskchampion::{SyncSettings, TaskChampionIntegration};
use tempfile::TempDir;

struct Harness {
    driver: Driver,
    tmp: TempDir,
}

impl Harness {
    async fn new(config: Config) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let taskchampion = TaskChampionIntegration::new(tmp.path().join("data"))
            .await
            .expect("replica");
        let driver = Driver::new(Session {
            config,
            warnings: Vec::new(),
            taskchampion,
            taskrc_file: None,
        })
        .await
        .expect("app");
        Harness { driver, tmp }
    }

    /// A second handle on the app's replica, writing the way `task` does
    /// while lazytask is open.
    async fn outside_writer(&self) -> TaskChampionIntegration {
        TaskChampionIntegration::new(self.tmp.path().join("data"))
            .await
            .expect("second handle")
    }

    /// The description shown in the task detail panel.
    fn selected_description(&self) -> String {
        self.driver
            .screen()
            .lines()
            .find_map(|line| {
                let (_, rest) = line.split_once("│Description   ")?;
                Some(rest.split('│').next()?.trim().to_string())
            })
            .expect("task detail panel missing")
    }
}

fn local_sync(server_dir: &std::path::Path) -> SyncSettings {
    SyncSettings::Local {
        server_dir: Some(server_dir.to_path_buf()),
    }
}

#[tokio::test]
async fn task_added_outside_lazytask_appears_without_restart() {
    let mut h = Harness::new(Config::default()).await;
    assert!(!h.driver.screen().contains("added outside"));

    h.outside_writer()
        .await
        .add_task("added outside", &[])
        .await
        .expect("add");

    assert!(
        h.driver
            .step_until(Duration::from_secs(5), |s| s.contains("added outside"))
            .await
            .expect("step"),
        "task added outside never appeared:\n{}",
        h.driver.screen()
    );
}

#[tokio::test]
async fn reload_from_outside_change_keeps_the_selected_task() {
    let mut h = Harness::new(Config::default()).await;
    let mut writer = h.outside_writer().await;
    writer.add_task("older task", &[]).await.expect("add");
    writer.add_task("newer task", &[]).await.expect("add");
    assert!(h
        .driver
        .step_until(Duration::from_secs(5), |s| s.contains("Tasks (2)"))
        .await
        .expect("step"));

    h.driver.press(KeyCode::Down);
    h.driver.app.step().await.expect("step");
    let selected = h.selected_description();

    writer.add_task("third task", &[]).await.expect("add");
    assert!(h
        .driver
        .step_until(Duration::from_secs(5), |s| s.contains("Tasks (3)"))
        .await
        .expect("step"));
    assert_eq!(h.selected_description(), selected);
}

#[tokio::test]
async fn auto_sync_pulls_remote_tasks() {
    let mut config = Config::default();
    config.sync.auto_sync_interval = 1;
    let mut h = Harness::new(config).await;
    let server_dir = h.tmp.path().join("server");
    h.driver
        .app
        .taskchampion
        .configure_sync(local_sync(&server_dir))
        .expect("configure app sync");

    let mut peer = TaskChampionIntegration::new(h.tmp.path().join("peer"))
        .await
        .expect("peer");
    peer.configure_sync(local_sync(&server_dir))
        .expect("configure peer sync");
    peer.add_task("from the server", &[]).await.expect("add");
    peer.sync().await.expect("peer sync");

    assert!(
        h.driver
            .step_until(Duration::from_secs(5), |s| s.contains("from the server"))
            .await
            .expect("step"),
        "auto-sync never pulled the remote task:\n{}",
        h.driver.screen()
    );
}

#[tokio::test]
async fn zero_auto_sync_interval_never_syncs() {
    let mut config = Config::default();
    config.sync.auto_sync_interval = 0;
    let mut h = Harness::new(config).await;
    let server_dir = h.tmp.path().join("server");
    h.driver
        .app
        .taskchampion
        .configure_sync(local_sync(&server_dir))
        .expect("configure app sync");

    let mut peer = TaskChampionIntegration::new(h.tmp.path().join("peer"))
        .await
        .expect("peer");
    peer.configure_sync(local_sync(&server_dir))
        .expect("configure peer sync");
    peer.add_task("from the server", &[]).await.expect("add");
    peer.sync().await.expect("peer sync");

    assert!(!h
        .driver
        .step_until(Duration::from_secs(2), |s| s.contains("from the server"))
        .await
        .expect("step"));
}

#[tokio::test]
async fn q_ends_the_event_loop() {
    let mut h = Harness::new(Config::default()).await;
    h.driver.press(KeyCode::Char('q'));
    tokio::time::timeout(Duration::from_secs(5), h.driver.app.run())
        .await
        .expect("run kept going after q")
        .expect("run");
}
