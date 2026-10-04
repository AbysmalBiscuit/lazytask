// Startup reads the user's taskrc: these tests open a session the way the
// binary does, with the taskrc, TASKDATA and home passed in explicitly.

use std::path::{Path, PathBuf};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazytask::app::{LaunchEnv, Session};
use lazytask::handlers::sync::SyncHandler;
use lazytask::taskchampion::{SyncSettings, TaskChampionIntegration};
use lazytask::ui::app_ui::AppUI;
use ratatui::{backend::TestBackend, Terminal};
use tempfile::TempDir;

struct Fixture {
    tmp: TempDir,
}

impl Fixture {
    fn new() -> Result<Self> {
        let tmp = tempfile::tempdir()?;
        std::fs::create_dir(tmp.path().join("home"))?;
        Ok(Fixture { tmp })
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    fn write(&self, rel: &str, contents: &str) -> Result<PathBuf> {
        let path = self.path(rel);
        std::fs::write(&path, contents)?;
        Ok(path)
    }

    /// Opens a session with no lazytask config file, `taskrc_var` as `TASKRC`
    /// and `taskdata_var` as `TASKDATA`.
    async fn open(&self, taskrc_var: &Path, taskdata_var: Option<&Path>) -> Result<Session> {
        let missing_config = self.path("no-config.toml");
        Session::open(
            Some(missing_config.to_str().unwrap()),
            LaunchEnv {
                taskrc_var: Some(taskrc_var.into()),
                taskdata_var: taskdata_var.map(Into::into),
                home: Some(self.path("home")),
            },
        )
        .await
    }
}

#[tokio::test]
async fn sync_from_a_fresh_launch_pushes_to_the_taskrc_local_server() -> Result<()> {
    let fx = Fixture::new()?;
    let server_dir = fx.path("server");
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "data.location={}\nsync.local.server_dir={}\n",
            fx.path("data").display(),
            server_dir.display()
        ),
    )?;

    let mut session = fx.open(&taskrc, None).await?;
    let uuid = session
        .taskchampion
        .add_task("Pushed via taskrc", &[])
        .await?;

    let mut ui = AppUI::new(&session.config)?;
    let mut sync_handler = SyncHandler::new();
    sync_handler.initialize(&session.taskchampion)?;
    let action = ui.action(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    ui.handle_action(action, &mut session.taskchampion, &mut sync_handler)
        .await?;
    assert!(!ui.has_active_form(), "sync opened a modal");

    let mut other = TaskChampionIntegration::new(fx.path("other")).await?;
    other.configure_sync(SyncSettings {
        local_server_dir: Some(server_dir),
        ..Default::default()
    })?;
    other.sync().await?;
    let pulled: Vec<String> = other
        .list_tasks()
        .await?
        .into_iter()
        .map(|t| t.uuid)
        .collect();
    assert_eq!(pulled, [uuid]);
    Ok(())
}

#[tokio::test]
async fn includes_are_followed_and_later_values_override_earlier_ones() -> Result<()> {
    let fx = Fixture::new()?;
    std::fs::create_dir(fx.path("rc.d"))?;
    fx.write(
        "rc.d/data.rc",
        &format!("data.location={}\n", fx.path("included").display()),
    )?;
    fx.write(
        "rc.d/sync.rc",
        &format!(
            "sync.local.server_dir={}\n",
            fx.path("overridden-server").display()
        ),
    )?;
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "data.location={}\n\
             include rc.d/data.rc\n\
             include {}\n\
             sync.local.server_dir={}\n",
            fx.path("earlier").display(),
            fx.path("rc.d/sync.rc").display(),
            fx.path("server").display(),
        ),
    )?;

    let mut session = fx.open(&taskrc, None).await?;

    assert_eq!(session.taskchampion.data_dir(), &fx.path("included"));
    session.taskchampion.sync().await?;
    assert!(fx.path("server").exists(), "sync used the overridden dir");
    assert!(!fx.path("overridden-server").exists());
    Ok(())
}

#[tokio::test]
async fn taskrc_data_location_applies_only_when_config_and_taskdata_are_unset() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write(
        "taskrc",
        &format!("data.location={}\n", fx.path("from-taskrc").display()),
    )?;

    let session = fx.open(&taskrc, None).await?;
    assert_eq!(session.taskchampion.data_dir(), &fx.path("from-taskrc"));
    drop(session);

    let session = fx.open(&taskrc, Some(&fx.path("from-taskdata"))).await?;
    assert_eq!(session.taskchampion.data_dir(), &fx.path("from-taskdata"));
    drop(session);

    let config = fx.write(
        "config.toml",
        &format!(
            "[taskwarrior]\ndata_location = \"{}\"\n",
            fx.path("from-config").display()
        ),
    )?;
    let session = Session::open(
        Some(config.to_str().unwrap()),
        LaunchEnv {
            taskrc_var: Some(taskrc.into()),
            taskdata_var: Some(fx.path("from-taskdata").into()),
            home: Some(fx.path("home")),
        },
    )
    .await?;
    assert_eq!(session.taskchampion.data_dir(), &fx.path("from-config"));
    Ok(())
}

#[tokio::test]
async fn malformed_line_reports_its_file_and_line_number() -> Result<()> {
    let fx = Fixture::new()?;
    let included = fx.write("extra.rc", "# comment\n\nsync.local.server_dir\n")?;
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "data.location={}\ninclude extra.rc\n",
            fx.path("data").display()
        ),
    )?;

    let err = match fx.open(&taskrc, None).await {
        Ok(_) => panic!("malformed taskrc opened"),
        Err(err) => format!("{:#}", err),
    };

    assert!(
        err.contains(&format!("{}:3", included.display())),
        "error lacks file:line: {err}"
    );
    assert!(
        err.contains("sync.local.server_dir"),
        "error lacks the line: {err}"
    );
    Ok(())
}

#[tokio::test]
async fn leading_tilde_in_local_server_dir_expands_to_home() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "data.location={}\nsync.local.server_dir=~/syncdir\n",
            fx.path("data").display()
        ),
    )?;

    let mut session = fx.open(&taskrc, None).await?;
    session.taskchampion.add_task("Synced home", &[]).await?;
    session.taskchampion.sync().await?;

    assert!(
        fx.path("home/syncdir").is_dir(),
        "server dir not under home"
    );
    Ok(())
}

#[tokio::test]
async fn incomplete_taskrc_sync_settings_warn_and_leave_sync_unconfigured() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "data.location={}\nsync.server.url=https://tw.example.com\n",
            fx.path("data").display()
        ),
    )?;

    let session = fx.open(&taskrc, None).await?;
    assert!(!session.taskchampion.is_sync_configured());

    let mut ui = AppUI::new(&session.config)?;
    ui.show_config_warnings(&session.warnings);
    let mut sync_handler = SyncHandler::new();
    sync_handler.initialize(&session.taskchampion)?;
    let mut terminal = Terminal::new(TestBackend::new(200, 40))?;
    terminal.draw(|f| ui.render_with_sync(f, &sync_handler))?;
    let screen: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        screen.contains("taskrc") && screen.contains("client_id"),
        "warning missing from screen:\n{screen}"
    );
    Ok(())
}
