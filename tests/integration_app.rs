// Drives the real App event loop on a TestBackend: keys through the input
// channel, replica changes through the file watcher.

use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazytask::app::{App, Session};
use lazytask::config::Config;
use lazytask::taskchampion::TaskChampionIntegration;
use ratatui::{backend::TestBackend, Terminal};
use tempfile::TempDir;
use tokio::sync::mpsc;

struct Harness {
    app: App<TestBackend>,
    keys: mpsc::UnboundedSender<Event>,
    tmp: TempDir,
}

impl Harness {
    async fn new(config: Config) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (keys, input) = mpsc::unbounded_channel();
        let taskchampion = TaskChampionIntegration::new(tmp.path().join("data"))
            .await
            .expect("replica");
        let app = App::with_terminal(
            Terminal::new(TestBackend::new(160, 40)).expect("terminal"),
            Session {
                config,
                warnings: Vec::new(),
                taskchampion,
            },
            input,
        )
        .await
        .expect("app");
        Harness { app, keys, tmp }
    }

    /// A second handle on the app's replica, writing the way `task` does
    /// while lazytask is open.
    async fn outside_writer(&self) -> TaskChampionIntegration {
        TaskChampionIntegration::new(self.tmp.path().join("data"))
            .await
            .expect("second handle")
    }

    fn press(&self, code: KeyCode) {
        self.keys
            .send(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
            .expect("send key");
    }

    /// Runs the event loop until `done` holds for the screen, or `within`
    /// passes. Returns whether `done` held.
    async fn step_until(&mut self, within: Duration, done: impl Fn(&str) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + within;
        while !done(&self.screen()) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, self.app.step()).await {
                Ok(result) => result.expect("step"),
                Err(_) => return false,
            }
        }
        true
    }

    fn screen(&self) -> String {
        let buf = self.app.terminal.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The description shown in the task detail panel.
    fn selected_description(&self) -> String {
        self.screen()
            .lines()
            .find_map(|line| {
                let (_, rest) = line.split_once("│Description   ")?;
                Some(rest.split('│').next()?.trim().to_string())
            })
            .expect("task detail panel missing")
    }
}

#[tokio::test]
async fn task_added_outside_lazytask_appears_without_restart() {
    let mut h = Harness::new(Config::default()).await;
    assert!(!h.screen().contains("added outside"));

    h.outside_writer()
        .await
        .add_task("added outside", &[])
        .await
        .expect("add");

    assert!(
        h.step_until(Duration::from_secs(5), |s| s.contains("added outside"))
            .await,
        "task added outside never appeared:\n{}",
        h.screen()
    );
}

#[tokio::test]
async fn reload_from_outside_change_keeps_the_selected_task() {
    let mut h = Harness::new(Config::default()).await;
    let mut writer = h.outside_writer().await;
    writer.add_task("older task", &[]).await.expect("add");
    writer.add_task("newer task", &[]).await.expect("add");
    assert!(
        h.step_until(Duration::from_secs(5), |s| s.contains("Tasks (2)"))
            .await
    );

    h.press(KeyCode::Down);
    h.app.step().await.expect("step");
    let selected = h.selected_description();

    writer.add_task("third task", &[]).await.expect("add");
    assert!(
        h.step_until(Duration::from_secs(5), |s| s.contains("Tasks (3)"))
            .await
    );
    assert_eq!(h.selected_description(), selected);
}
