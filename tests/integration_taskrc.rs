// Startup reads the user's taskrc: these tests open a session the way the
// binary does, with the taskrc, TASKDATA and home passed in explicitly.

use std::path::{Path, PathBuf};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazytask::app::{LaunchEnv, Session};
use lazytask::handlers::sync::SyncHandler;
use lazytask::taskchampion::{AwsCredentials, SyncSettings, TaskChampionIntegration};
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

    /// Opens a session with `taskrc_var` as `TASKRC`, `taskdata_var` as
    /// `TASKDATA`, and the lazytask config at `config`, or none.
    async fn open(
        &self,
        taskrc_var: &Path,
        taskdata_var: Option<&Path>,
        config: Option<&Path>,
    ) -> Result<Session> {
        self.open_with(self.env(taskrc_var, taskdata_var), config)
            .await
    }

    /// A launch environment with a home directory, no working directory,
    /// variables or package rc directories, and the given `TASKRC` and
    /// `TASKDATA`.
    fn env(&self, taskrc_var: &Path, taskdata_var: Option<&Path>) -> LaunchEnv {
        LaunchEnv {
            taskrc_var: Some(taskrc_var.into()),
            taskdata_var: taskdata_var.map(Into::into),
            home: Some(self.path("home")),
            ..LaunchEnv::default()
        }
    }

    async fn open_with(&self, env: LaunchEnv, config: Option<&Path>) -> Result<Session> {
        let config = config.map_or_else(|| self.path("no-config.toml"), Path::to_path_buf);
        Session::open(Some(config.to_str().unwrap()), env).await
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

    let mut session = fx.open(&taskrc, None, None).await?;
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
    other.configure_sync(SyncSettings::Local {
        server_dir: Some(server_dir),
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

    let mut session = fx.open(&taskrc, None, None).await?;

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

    let session = fx.open(&taskrc, None, None).await?;
    assert_eq!(session.taskchampion.data_dir(), &fx.path("from-taskrc"));
    drop(session);

    let session = fx
        .open(&taskrc, Some(&fx.path("from-taskdata")), None)
        .await?;
    assert_eq!(session.taskchampion.data_dir(), &fx.path("from-taskdata"));
    drop(session);

    let config = fx.write(
        "config.toml",
        &format!(
            "[taskwarrior]\ndata_location = \"{}\"\n",
            fx.path("from-config").display()
        ),
    )?;
    let session = fx
        .open(&taskrc, Some(&fx.path("from-taskdata")), Some(&config))
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

    let err = match fx.open(&taskrc, None, None).await {
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

    let mut session = fx.open(&taskrc, None, None).await?;
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

    let session = fx.open(&taskrc, None, None).await?;
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

#[tokio::test]
async fn relative_includes_search_cwd_then_taskrc_dir_then_package_dirs() -> Result<()> {
    let fx = Fixture::new()?;
    for dir in ["cwd", "rc", "pkg"] {
        std::fs::create_dir(fx.path(dir))?;
    }
    let data_line = |dir: &str| format!("data.location={}\n", fx.path(dir).display());
    fx.write("cwd/a.rc", &data_line("from-cwd"))?;
    fx.write("rc/a.rc", &data_line("from-rc-dir"))?;
    fx.write("rc/b.rc", "include theme.rc\n")?;
    fx.write("pkg/b.rc", &data_line("from-pkg-b"))?;
    fx.write(
        "pkg/theme.rc",
        &format!(
            "sync.local.server_dir={}\n",
            fx.path("pkg-server").display()
        ),
    )?;
    let taskrc = fx.write("rc/taskrc", "include a.rc\ninclude b.rc\n")?;

    let env = LaunchEnv {
        cwd: Some(fx.path("cwd")),
        rc_dirs: vec![fx.path("pkg")],
        ..fx.env(&taskrc, None)
    };
    let session = fx.open_with(env, None).await?;

    assert_eq!(session.taskchampion.data_dir(), &fx.path("from-cwd"));
    assert!(session.taskchampion.is_sync_configured());
    assert!(fx.path("pkg-server").is_dir(), "package theme.rc not read");
    Ok(())
}

#[tokio::test]
async fn variables_expand_in_include_paths_and_values() -> Result<()> {
    let fx = Fixture::new()?;
    std::fs::create_dir(fx.path("conf"))?;
    fx.write("conf/extra.rc", "data.location=$DATA_ROOT/tasks\n")?;
    let taskrc = fx.write("taskrc", "include $CONF_DIR/extra.rc\n")?;

    let env = LaunchEnv {
        vars: [
            ("CONF_DIR", fx.path("conf")),
            ("DATA_ROOT", fx.path("root")),
        ]
        .map(|(k, v)| (k.to_string(), v.display().to_string()))
        .into(),
        ..fx.env(&taskrc, None)
    };
    let session = fx.open_with(env, None).await?;

    assert_eq!(session.taskchampion.data_dir(), &fx.path("root/tasks"));
    Ok(())
}

const CLIENT_ID: &str = "0f0e0d0c-0b0a-4908-8706-050403020100";

/// Opens a session on a taskrc of `lines` and returns the sync target it
/// configured, or the startup warnings when it configured none.
async fn sync_target(lines: &str) -> Result<std::result::Result<SyncSettings, Vec<String>>> {
    let fx = Fixture::new()?;
    let taskrc = fx.write(
        "taskrc",
        &format!("data.location={}\n{lines}", fx.path("data").display()),
    )?;
    let session = fx.open(&taskrc, None, None).await?;
    Ok(session
        .taskchampion
        .sync_settings()
        .cloned()
        .ok_or(session.warnings))
}

#[tokio::test]
async fn sync_backend_precedence_is_local_then_aws_then_gcp_then_server() -> Result<()> {
    let server = format!(
        "sync.server.url=https://tw.example.com\nsync.server.client_id={CLIENT_ID}\n\
         sync.encryption_secret=s3cret\n"
    );
    let gcp = "sync.gcp.bucket=gcp-bucket\nsync.gcp.credential_path=/keys/gcp.json\n";
    let aws = "sync.aws.bucket=aws-bucket\nsync.aws.region=eu-west-1\nsync.aws.profile=tw\n";
    let local_dir = tempfile::tempdir()?;
    let local = format!("sync.local.server_dir={}\n", local_dir.path().display());

    assert_eq!(
        sync_target(&format!("{server}{gcp}{aws}{local}")).await?,
        Ok(SyncSettings::Local {
            server_dir: Some(local_dir.path().into())
        })
    );
    assert_eq!(
        sync_target(&format!("{server}{gcp}{aws}")).await?,
        Ok(SyncSettings::Aws {
            region: "eu-west-1".into(),
            bucket: "aws-bucket".into(),
            credentials: AwsCredentials::Profile("tw".into()),
            encryption_secret: "s3cret".into(),
        })
    );
    assert_eq!(
        sync_target(&format!("{server}{gcp}")).await?,
        Ok(SyncSettings::Gcp {
            bucket: "gcp-bucket".into(),
            credential_path: Some("/keys/gcp.json".into()),
            encryption_secret: "s3cret".into(),
        })
    );
    assert_eq!(
        sync_target(&server).await?,
        Ok(SyncSettings::Server {
            url: "https://tw.example.com".into(),
            client_id: CLIENT_ID.into(),
            encryption_secret: "s3cret".into(),
        })
    );
    Ok(())
}

#[tokio::test]
async fn aws_credentials_come_from_exactly_one_method() -> Result<()> {
    let aws = "sync.aws.bucket=b\nsync.aws.region=r\nsync.encryption_secret=s\n";

    assert_eq!(
        sync_target(&format!(
            "{aws}sync.aws.access_key_id=AKID\nsync.aws.secret_access_key=SAK\n"
        ))
        .await?,
        Ok(SyncSettings::Aws {
            region: "r".into(),
            bucket: "b".into(),
            credentials: AwsCredentials::AccessKey {
                access_key_id: "AKID".into(),
                secret_access_key: "SAK".into(),
            },
            encryption_secret: "s".into(),
        })
    );
    assert_eq!(
        sync_target(&format!("{aws}sync.aws.default_credentials=true\n")).await?,
        Ok(SyncSettings::Aws {
            region: "r".into(),
            bucket: "b".into(),
            credentials: AwsCredentials::Default,
            encryption_secret: "s".into(),
        })
    );
    for creds in [
        "",
        "sync.aws.profile=p\nsync.aws.default_credentials=true\n",
    ] {
        let warnings = sync_target(&format!("{aws}{creds}")).await?.unwrap_err();
        assert!(
            warnings.iter().any(|w| w.contains("exactly one")),
            "{creds:?}: {warnings:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn cloud_and_server_sync_need_their_required_keys() -> Result<()> {
    let cases = [
        (
            "sync.aws.bucket=b\nsync.aws.profile=p\nsync.encryption_secret=s\n",
            "sync.aws.region",
        ),
        (
            "sync.aws.bucket=b\nsync.aws.region=r\nsync.aws.profile=p\n",
            "sync.encryption_secret",
        ),
        ("sync.gcp.bucket=b\n", "sync.encryption_secret"),
        (
            "sync.server.url=https://tw.example.com\nsync.encryption_secret=s\n",
            "sync.server.client_id",
        ),
    ];
    for (lines, missing) in cases {
        let warnings = sync_target(lines).await?.unwrap_err();
        assert!(
            warnings.iter().any(|w| w.contains(missing)),
            "{lines:?} should name {missing}: {warnings:?}"
        );
    }
    Ok(())
}
