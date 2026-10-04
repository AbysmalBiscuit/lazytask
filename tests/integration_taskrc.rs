// Startup reads the user's taskrc: these tests open a session the way the
// binary does, with the taskrc, TASKDATA and home passed in explicitly.

use std::path::{Path, PathBuf};

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazytask::app::{App, LaunchEnv, Session};
use lazytask::handlers::sync::SyncHandler;
use lazytask::taskchampion::{AwsCredentials, SyncSettings, TaskChampionIntegration};
use lazytask::ui::app_ui::AppUI;
use ratatui::{backend::TestBackend, Terminal};
use tempfile::TempDir;
use tokio::sync::mpsc;

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

/// The app as the binary runs it on a session opened from a taskrc, fed key
/// presses through its input channel.
struct Running {
    app: App<TestBackend>,
    keys: mpsc::UnboundedSender<Event>,
}

impl Fixture {
    /// Launches the app on `taskrc`, with automatic sync off.
    async fn launch(&self, taskrc: &Path) -> Result<Running> {
        let config = self.write("config.toml", "[sync]\nauto_sync_interval = 0\n")?;
        let session = self
            .open(taskrc, Some(&self.path("data")), Some(&config))
            .await?;
        let (keys, input) = mpsc::unbounded_channel();
        let terminal = Terminal::new(TestBackend::new(220, 40))?;
        let app = App::with_terminal(terminal, session, input).await?;
        Ok(Running { app, keys })
    }
}

impl Running {
    fn press(&self, code: KeyCode) {
        self.keys
            .send(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
            .expect("app input closed");
    }

    fn type_text(&self, text: &str) {
        text.chars().for_each(|c| self.press(KeyCode::Char(c)));
    }

    /// Replaces the focused modal field's `old` text with `new`.
    fn retype(&self, old: &str, new: &str) {
        old.chars().for_each(|_| self.press(KeyCode::Backspace));
        self.type_text(new);
    }

    fn open_sync_modal(&self) {
        self.type_text("S");
    }

    /// Runs the event loop until the screen contains `needle`, returning the
    /// screen. Fails when a step fails, or with the last screen after five
    /// seconds.
    async fn wait_for(&mut self, needle: &str) -> Result<String> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let screen = self.screen();
            if screen.contains(needle) {
                return Ok(screen);
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, self.app.step()).await {
                Ok(stepped) => stepped?,
                Err(_) => anyhow::bail!("{needle:?} never appeared:\n{screen}"),
            }
        }
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

/// Fills in the empty sync modal with a server, client and `secret`, and
/// saves.
fn save_new_server(app: &Running, secret: &str) {
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
async fn saving_refuses_to_put_the_secret_in_a_file_others_can_read() -> Result<()> {
    let fx = Fixture::new()?;
    let contents = "color=on\n";
    let taskrc = fx.write("taskrc", contents)?;
    set_mode(&taskrc, 0o644)?;

    let mut app = fx.launch(&taskrc).await?;
    save_new_server(&app, "s3cret");
    let screen = app.wait_for("chmod 600").await?;

    assert!(
        screen.contains(&taskrc.display().to_string()),
        "refusal does not name the file:\n{screen}"
    );
    assert_eq!(std::fs::read_to_string(&taskrc)?, contents);
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

    app.press(KeyCode::Tab);
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
async fn saving_over_a_local_target_notes_that_taskwarrior_still_prefers_it() -> Result<()> {
    let fx = Fixture::new()?;
    let local = format!("sync.local.server_dir={}\n", fx.path("server").display());
    let taskrc = fx.write("taskrc", &local)?;
    #[cfg(unix)]
    set_mode(&taskrc, 0o600)?;

    let mut app = fx.launch(&taskrc).await?;
    save_new_server(&app, "s3cret");
    app.wait_for("Taskwarrior syncs to sync.local.server_dir until it is removed")
        .await?;

    assert_eq!(
        std::fs::read_to_string(&taskrc)?,
        format!("{local}{}", server_block(CLIENT_ID, "s3cret"))
    );
    Ok(())
}
