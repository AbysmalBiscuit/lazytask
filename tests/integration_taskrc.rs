// Startup reads the user's taskrc: these tests open a session the way the
// binary does, with the taskrc, TASKDATA and home passed in explicitly.

use std::path::{Path, PathBuf};

use anyhow::Result;
mod common;

use common::Driver;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazytask::app::{LaunchEnv, Session};
use lazytask::handlers::sync::SyncHandler;
use lazytask::taskchampion::{
    AwsCredentials, ServerSettings, SyncSettings, TaskChampionIntegration,
};
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

    /// `home_env` with the given `TASKRC` and `TASKDATA`.
    fn env(&self, taskrc_var: &Path, taskdata_var: Option<&Path>) -> LaunchEnv {
        LaunchEnv {
            taskrc_var: Some(taskrc_var.into()),
            taskdata_var: taskdata_var.map(Into::into),
            ..self.home_env()
        }
    }

    async fn open_with(&self, env: LaunchEnv, config: Option<&Path>) -> Result<Session> {
        let config = config.map_or_else(|| self.path("no-config.toml"), Path::to_path_buf);
        Session::open(Some(config.to_str().unwrap()), env).await
    }

    /// A launch environment with a home directory and no working directory,
    /// variables or package rc directories, so the taskrc is found the way
    /// Taskwarrior finds it without `TASKRC`.
    fn home_env(&self) -> LaunchEnv {
        LaunchEnv {
            home: Some(self.path("home")),
            ..LaunchEnv::default()
        }
    }

    /// `home_env` with `XDG_CONFIG_HOME` set to `rel`.
    fn xdg_env(&self, rel: &str) -> LaunchEnv {
        LaunchEnv {
            xdg_config_home_var: Some(self.path(rel).into()),
            ..self.home_env()
        }
    }

    /// Writes a taskrc at `rel`, creating its directories, whose
    /// `data.location` is `data/<name>`.
    fn taskrc_naming(&self, rel: &str, name: &str) -> Result<PathBuf> {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, self.data_location_line(name))?;
        Ok(path)
    }

    fn data_location_line(&self, name: &str) -> String {
        format!("data.location={}\n", self.path("data").join(name).display())
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
    let mut sync_handler = SyncHandler::new(None);
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
    let mut sync_handler = SyncHandler::new(None);
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

#[tokio::test]
async fn without_a_home_taskrc_the_xdg_taskrc_names_the_replica() -> Result<()> {
    let fx = Fixture::new()?;
    fx.taskrc_naming("home/.config/task/taskrc", "dot-config")?;
    fx.taskrc_naming("xdg/task/taskrc", "xdg")?;
    std::fs::create_dir(fx.path("empty-xdg"))?;

    for (env, expected) in [
        (fx.home_env(), fx.path("data/dot-config")),
        (fx.xdg_env("xdg"), fx.path("data/xdg")),
        // As in Taskwarrior, a set XDG_CONFIG_HOME is the only place looked.
        (fx.xdg_env("empty-xdg"), fx.path("home/.task")),
        // An empty variable is unset, as the XDG spec says.
        (
            LaunchEnv {
                xdg_config_home_var: Some("".into()),
                ..fx.home_env()
            },
            fx.path("data/dot-config"),
        ),
    ] {
        let session = fx.open_with(env, None).await?;
        assert_eq!(session.taskchampion.data_dir(), &expected);
    }
    Ok(())
}

#[tokio::test]
async fn taskrc_precedence_is_config_then_taskrc_var_then_home_then_xdg() -> Result<()> {
    let fx = Fixture::new()?;
    fx.taskrc_naming("xdg/task/taskrc", "xdg")?;
    let home = fx.taskrc_naming("home/.taskrc", "home")?;
    let var = fx.taskrc_naming("var.taskrc", "var")?;
    let named = fx.taskrc_naming("named.taskrc", "config")?;
    let config = fx.write(
        "config.toml",
        &format!("[taskwarrior]\ntaskrc_path = \"{}\"\n", named.display()),
    )?;
    let with_var = LaunchEnv {
        taskrc_var: Some(var.into()),
        ..fx.xdg_env("xdg")
    };

    let cases = [
        (with_var.clone(), Some(config.as_path()), "config"),
        (with_var, None, "var"),
        (fx.xdg_env("xdg"), None, "home"),
    ];
    for (env, config, expected) in cases {
        let session = fx.open_with(env, config).await?;
        assert_eq!(
            session.taskchampion.data_dir(),
            &fx.path("data").join(expected)
        );
    }

    std::fs::remove_file(home)?;
    let session = fx.open_with(fx.xdg_env("xdg"), None).await?;
    assert_eq!(session.taskchampion.data_dir(), &fx.path("data/xdg"));
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
    let server = server_block(CLIENT_ID, "s3cret");
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
        Ok(SyncSettings::Server(ServerSettings {
            url: "https://tw.example.com".into(),
            client_id: CLIENT_ID.into(),
            encryption_secret: "s3cret".into(),
        }))
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

impl Fixture {
    /// Launches the app on `taskrc`, with automatic sync off.
    async fn launch(&self, taskrc: &Path) -> Result<Driver> {
        self.launch_with(self.env(taskrc, Some(&self.path("data"))))
            .await
    }

    /// Launches the app in `env`, with automatic sync off.
    async fn launch_with(&self, env: LaunchEnv) -> Result<Driver> {
        let config = self.write("config.toml", "[sync]\nauto_sync_interval = 0\n")?;
        let session = self.open_with(env, Some(&config)).await?;
        Driver::new(session).await
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

/// A taskrc's sync server block for https://tw.example.com, one key per
/// line in the order a save appends them.
fn server_block(client_id: &str, secret: &str) -> String {
    format!(
        "sync.server.url=https://tw.example.com\nsync.server.client_id={client_id}\n\
         sync.encryption_secret={secret}\n"
    )
}

const OTHER_CLIENT_ID: &str = "11111111-2222-4333-8444-555555555555";

#[tokio::test]
async fn saving_the_sync_modal_changes_only_the_sync_lines() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "# Taskwarrior settings\n\
             weekstart=monday\n\
             \n\
             # self-hosted sync\n\
             sync.server.url=https://old.example.com  # home server\n\
             color=on\n\
             sync.server.client_id={CLIENT_ID}\n\
             report.next.columns=id,description"
        ),
    )?;
    #[cfg(unix)]
    set_mode(&taskrc, 0o600)?;

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    app.retype("https://old.example.com", "https://new.example.com");
    app.press(KeyCode::Tab);
    app.retype(CLIENT_ID, OTHER_CLIENT_ID);
    app.press(KeyCode::Tab);
    app.type_text("s3cret");
    app.press(KeyCode::Enter);
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!(
            "# Taskwarrior settings\n\
             weekstart=monday\n\
             \n\
             # self-hosted sync\n\
             sync.server.url=https://new.example.com  # home server\n\
             color=on\n\
             sync.server.client_id={OTHER_CLIENT_ID}\n\
             report.next.columns=id,description\n\
             sync.encryption_secret=s3cret\n"
        )
    );
    Ok(())
}

#[tokio::test]
async fn saving_keeps_the_indentation_of_a_changed_line() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "  sync.server.url=https://old.example.com\n\tsync.server.client_id={CLIENT_ID}\n\
             \x20 sync.encryption_secret=s3cret\n"
        ),
    )?;
    #[cfg(unix)]
    set_mode(&taskrc, 0o600)?;

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    app.retype("https://old.example.com", "https://new.example.com");
    app.press(KeyCode::Enter);
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!(
            "  sync.server.url=https://new.example.com\n\tsync.server.client_id={CLIENT_ID}\n\
             \x20 sync.encryption_secret=s3cret\n"
        )
    );
    Ok(())
}

#[tokio::test]
async fn saving_appends_with_the_taskrc_line_ending() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write("taskrc", "# windows taskrc\r\ncolor=on")?;
    #[cfg(unix)]
    set_mode(&taskrc, 0o600)?;

    let mut app = fx.launch(&taskrc).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!(
            "# windows taskrc\r\ncolor=on\r\nsync.server.url=https://tw.example.com\r\n\
             sync.server.client_id={CLIENT_ID}\r\nsync.encryption_secret=s3cret\r\n"
        )
    );
    Ok(())
}

#[tokio::test]
async fn sync_modal_opens_prefilled_with_the_taskrc_sync_server() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write("taskrc", &server_block(CLIENT_ID, "s3cret"))?;

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    let screen = app.wait_for("Configure Sync").await?;

    assert!(
        screen.contains("https://tw.example.com")
            && screen.contains(CLIENT_ID)
            && screen.contains("│****** ")
            && !screen.contains("s3cret"),
        "modal not prefilled:\n{screen}"
    );
    Ok(())
}

#[tokio::test]
async fn sync_modal_prefills_server_keys_that_are_incomplete_or_outranked() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "sync.local.server_dir={}\nsync.server.url=https://tw.example.com\n\
             sync.server.client_id={CLIENT_ID}\n",
            fx.path("server").display()
        ),
    )?;

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    let screen = app.wait_for("Configure Sync").await?;

    assert!(
        screen.contains("https://tw.example.com") && screen.contains(CLIENT_ID),
        "modal not prefilled:\n{screen}"
    );
    Ok(())
}

#[tokio::test]
async fn saving_updates_a_key_in_the_included_file_that_defines_it() -> Result<()> {
    let fx = Fixture::new()?;
    let included = fx.write("sync.rc", &server_block(CLIENT_ID, "s3cret"))?;
    #[cfg(unix)]
    set_mode(&included, 0o600)?;
    let main = "# main taskrc\ninclude sync.rc\ncolor=on\n";
    let taskrc = fx.write("taskrc", main)?;

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    app.press(KeyCode::Tab);
    app.retype(CLIENT_ID, OTHER_CLIENT_ID);
    app.press(KeyCode::Tab);
    app.retype("s3cret", "n3w-secret");
    app.press(KeyCode::Enter);
    app.wait_for("Sync settings saved").await?;

    assert_eq!(std::fs::read_to_string(&taskrc)?, main);
    assert_eq!(
        std::fs::read_to_string(&included)?,
        server_block(OTHER_CLIENT_ID, "n3w-secret")
    );
    Ok(())
}

#[tokio::test]
async fn saving_rewrites_a_deprecated_origin_line_as_the_server_url() -> Result<()> {
    let fx = Fixture::new()?;
    let included = fx.write(
        "sync.rc",
        &format!(
            "sync.server.origin=https://old.example.com\nsync.server.client_id={CLIENT_ID}\n\
             sync.encryption_secret=s3cret\n"
        ),
    )?;
    #[cfg(unix)]
    set_mode(&included, 0o600)?;
    let main = "include sync.rc\n";
    let taskrc = fx.write("taskrc", main)?;

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    app.retype("https://old.example.com", "https://tw.example.com");
    app.press(KeyCode::Enter);
    app.wait_for("Sync settings saved").await?;

    assert_eq!(std::fs::read_to_string(&taskrc)?, main);
    assert_eq!(
        std::fs::read_to_string(&included)?,
        server_block(CLIENT_ID, "s3cret")
    );
    Ok(())
}

/// Fills in the empty sync modal with a server, client and `secret`, and
/// saves.
fn save_new_server(app: &Driver, secret: &str) {
    app.open_sync_modal();
    app.type_text("https://tw.example.com");
    app.press(KeyCode::Tab);
    app.type_text(CLIENT_ID);
    app.press(KeyCode::Tab);
    app.type_text(secret);
    app.press(KeyCode::Enter);
}

#[cfg(unix)]
#[tokio::test]
async fn saving_the_secret_makes_a_file_others_can_read_private() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fixture::new()?;
    let taskrc = fx.write("taskrc", "color=on\n")?;
    set_mode(&taskrc, 0o644)?;

    let mut app = fx.launch(&taskrc).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::metadata(&taskrc)?.permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!("color=on\n{}", server_block(CLIENT_ID, "s3cret"))
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn saving_refuses_the_secret_when_the_file_cannot_be_made_private() -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let fx = Fixture::new()?;
    // /dev/null is readable by everyone and owned by root, so chmod fails for
    // anyone else. Its owner would change its mode, so the test skips then.
    let shared = Path::new("/dev/null");
    let own_uid = std::fs::metadata(fx.write("probe", "")?)?.uid();
    if std::fs::metadata(shared)?.uid() == own_uid {
        return Ok(());
    }

    let mut app = fx.launch(shared).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Run `chmod 600 /dev/null`").await?;
    Ok(())
}

#[tokio::test]
async fn saving_without_a_taskrc_creates_one_holding_only_the_sync_keys() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.path("taskrc");

    let mut app = fx.launch(&taskrc).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        server_block(CLIENT_ID, "s3cret")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&taskrc)?.permissions().mode() & 0o777,
            0o600
        );
    }
    Ok(())
}

#[tokio::test]
async fn saving_with_only_an_xdg_taskrc_writes_into_it() -> Result<()> {
    let fx = Fixture::new()?;
    let xdg_taskrc = fx.taskrc_naming("xdg/task/taskrc", "xdg")?;

    let mut app = fx.launch_with(fx.xdg_env("xdg")).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(&xdg_taskrc)?,
        format!(
            "{}{}",
            fx.data_location_line("xdg"),
            server_block(CLIENT_ID, "s3cret")
        )
    );
    assert!(!fx.path("home/.taskrc").exists());
    Ok(())
}

#[tokio::test]
async fn without_a_home_or_xdg_taskrc_home_holds_the_replica_and_saved_taskrc() -> Result<()> {
    let fx = Fixture::new()?;
    std::fs::create_dir(fx.path("xdg"))?;

    for env in [fx.home_env(), fx.xdg_env("xdg")] {
        let session = fx.open_with(env, None).await?;
        assert_eq!(session.taskchampion.data_dir(), &fx.path("home/.task"));
    }

    let mut app = fx.launch_with(fx.xdg_env("xdg")).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(fx.path("home/.taskrc"))?,
        server_block(CLIENT_ID, "s3cret")
    );
    assert!(!fx.path("xdg/task/taskrc").exists());
    Ok(())
}

#[tokio::test]
async fn an_empty_server_url_is_refused_and_the_modal_keeps_its_fields() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.path("taskrc");
    let mut app = fx.launch(&taskrc).await?;

    app.open_sync_modal();
    app.press(KeyCode::Tab);
    app.type_text(CLIENT_ID);
    app.press(KeyCode::Tab);
    app.type_text("s3cret");
    app.press(KeyCode::Enter);
    app.wait_for("Server URL is required").await?;
    assert!(!taskrc.exists(), "a refused save wrote the taskrc");

    app.press(KeyCode::Down);
    app.press(KeyCode::Down);
    app.type_text("https://tw.example.com");
    app.press(KeyCode::Enter);
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        server_block(CLIENT_ID, "s3cret")
    );
    Ok(())
}

#[tokio::test]
async fn saving_refuses_a_secret_the_taskrc_reads_as_a_comment() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.path("taskrc");
    let mut app = fx.launch(&taskrc).await?;

    save_new_server(&app, "has#hash");
    app.wait_for("sync.encryption_secret cannot be saved")
        .await?;

    assert!(!taskrc.exists(), "a refused save wrote the taskrc");
    Ok(())
}

#[tokio::test]
async fn saving_under_a_cloud_target_notes_that_taskwarrior_still_prefers_it() -> Result<()> {
    let fx = Fixture::new()?;
    let gcp = "sync.gcp.bucket=tasks\n";
    let taskrc = fx.write("taskrc", gcp)?;
    #[cfg(unix)]
    set_mode(&taskrc, 0o600)?;

    let mut app = fx.launch(&taskrc).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Taskwarrior syncs to sync.gcp.bucket until it is removed")
        .await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!("{gcp}{}", server_block(CLIENT_ID, "s3cret"))
    );
    Ok(())
}

#[tokio::test]
async fn saving_a_local_server_dir_makes_it_the_sync_target() -> Result<()> {
    let fx = Fixture::new()?;
    let taskrc = fx.path("taskrc");
    let server_dir = fx.path("server");

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    app.press(KeyCode::Up);
    app.type_text(&server_dir.display().to_string());
    app.press(KeyCode::Enter);
    app.wait_for("Sync settings saved").await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!("sync.local.server_dir={}\n", server_dir.display())
    );
    assert_eq!(
        app.app.taskchampion.sync_settings(),
        Some(&SyncSettings::Local {
            server_dir: Some(server_dir)
        })
    );
    Ok(())
}

#[tokio::test]
async fn clearing_the_local_server_dir_hands_sync_to_the_server() -> Result<()> {
    let fx = Fixture::new()?;
    let server_dir = fx.path("server").display().to_string();
    let taskrc = fx.write(
        "taskrc",
        &format!(
            "sync.local.server_dir={server_dir}\n{}",
            server_block(CLIENT_ID, "s3cret")
        ),
    )?;
    #[cfg(unix)]
    set_mode(&taskrc, 0o600)?;

    let mut app = fx.launch(&taskrc).await?;
    app.open_sync_modal();
    app.press(KeyCode::Up);
    app.retype(&server_dir, "");
    app.press(KeyCode::Enter);
    let screen = app.wait_for("Sync settings saved").await?;

    assert!(!screen.contains("Taskwarrior syncs to"), "{screen}");
    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!(
            "sync.local.server_dir=\n{}",
            server_block(CLIENT_ID, "s3cret")
        )
    );
    assert!(matches!(
        app.app.taskchampion.sync_settings(),
        Some(SyncSettings::Server(_))
    ));
    Ok(())
}
