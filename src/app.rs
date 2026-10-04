use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, Interval, MissedTickBehavior};

use crate::config::{Config, LoadedConfig};
use crate::handlers::input::Action;
use crate::handlers::sync::SyncHandler;
use crate::handlers::watcher::ReplicaWatcher;
use crate::taskchampion::TaskChampionIntegration;
use crate::taskrc::Taskrc;
use crate::ui::app_ui::AppUI;

pub type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

pub struct App<B: Backend> {
    pub config: Config,
    pub terminal: Terminal<B>,
    pub ui: AppUI,
    pub taskchampion: TaskChampionIntegration,
    pub sync_handler: SyncHandler,
    pub should_quit: bool,
    input: mpsc::UnboundedReceiver<Event>,
    replica_watcher: Option<ReplicaWatcher>,
    auto_sync: Option<Interval>,
    needs_redraw: bool,
    // Declared last so the terminal is restored after everything else drops.
    _terminal_guard: Option<TerminalGuard>,
}

/// Where packaged Taskwarrior installs keep the rc files, such as themes,
/// that a taskrc can include by bare name. Taskwarrior searches only the
/// directory it was built with, so this covers the common builds.
const PACKAGE_RC_DIRS: [&str; 4] = [
    "/usr/share/taskwarrior",
    "/usr/share/doc/task/rc",
    "/usr/local/share/doc/task/rc",
    "/opt/homebrew/share/doc/task/rc",
];

/// The parts of the process environment startup reads.
#[derive(Debug, Clone, Default)]
pub struct LaunchEnv {
    /// `TASKRC`
    pub taskrc_var: Option<OsString>,
    /// `TASKDATA`
    pub taskdata_var: Option<OsString>,
    pub home: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    /// Environment variables, which taskrc paths and values may reference
    /// as `$NAME`.
    pub vars: HashMap<String, String>,
    /// Package directories searched last for a relative taskrc include.
    pub rc_dirs: Vec<PathBuf>,
}

impl LaunchEnv {
    pub fn from_process() -> Self {
        LaunchEnv {
            taskrc_var: std::env::var_os("TASKRC"),
            taskdata_var: std::env::var_os("TASKDATA"),
            home: dirs::home_dir(),
            cwd: std::env::current_dir().ok(),
            vars: std::env::vars_os()
                .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
                .collect(),
            rc_dirs: PACKAGE_RC_DIRS.map(PathBuf::from).to_vec(),
        }
    }
}

/// Everything startup loads before the terminal is taken over: the config,
/// the user's taskrc, and the replica they point at, with sync configured
/// from the taskrc when it names a usable target.
pub struct Session {
    pub config: Config,
    /// Problems that do not stop startup, for the UI to show.
    pub warnings: Vec<String>,
    pub taskchampion: TaskChampionIntegration,
}

impl Session {
    pub async fn open(config_path: Option<&str>, env: LaunchEnv) -> Result<Self> {
        let LoadedConfig {
            mut config,
            unknown_keys,
        } = Config::load(config_path)?;
        // https://no-color.org: set and not empty.
        config.theme.no_color = env.vars.get("NO_COLOR").is_some_and(|v| !v.is_empty());
        let home = env.home.as_deref();
        let taskrc = match config
            .taskwarrior
            .resolve_taskrc_path(env.taskrc_var.clone(), home)?
        {
            Some(path) => Taskrc::load(&path, &env)?,
            None => Taskrc::default(),
        };
        let data_dir =
            config
                .taskwarrior
                .resolve_data_location(env.taskdata_var.clone(), &taskrc, home)?;
        let mut taskchampion = TaskChampionIntegration::new(data_dir).await?;

        let mut warnings = Vec::new();
        if !unknown_keys.is_empty() {
            warnings.push(format!("Unknown config keys: {}", unknown_keys.join(", ")));
        }
        let sync = taskrc.sync_settings().and_then(|settings| match settings {
            Some(settings) => taskchampion.configure_sync(settings),
            None => Ok(()),
        });
        if let Err(err) = sync {
            warnings.push(format!("Sync settings in taskrc ignored: {err:#}"));
        }

        Ok(Session {
            config,
            warnings,
            taskchampion,
        })
    }
}

impl App<CrosstermBackend<Stdout>> {
    /// Loads the config, taskrc and replica, then takes over the terminal.
    pub async fn new(config_path: Option<&str>, _verbose: bool) -> Result<Self> {
        let session = Session::open(config_path, LaunchEnv::from_process()).await?;
        // Unusable keybindings fail here, before the terminal is taken over.
        let ui = AppUI::new(&session.config)?;
        let guard = TerminalGuard::enter()?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

        let (tx, input) = mpsc::unbounded_channel();
        // crossterm's reader blocks, so it gets its own thread. It ends with
        // the process.
        std::thread::spawn(move || {
            while let Ok(event) = event::read() {
                if tx.send(event).is_err() {
                    break;
                }
            }
        });

        let mut app = Self::assemble(terminal, session, ui, input).await?;
        app._terminal_guard = Some(guard);
        Ok(app)
    }
}

impl<B: Backend> App<B>
where
    B::Error: Send + Sync + 'static,
{
    /// Builds the app for `session` on any ratatui backend, reading terminal
    /// events from `input`, and draws the first frame.
    pub async fn with_terminal(
        terminal: Terminal<B>,
        session: Session,
        input: mpsc::UnboundedReceiver<Event>,
    ) -> Result<Self> {
        let ui = AppUI::new(&session.config)?;
        Self::assemble(terminal, session, ui, input).await
    }

    async fn assemble(
        terminal: Terminal<B>,
        session: Session,
        mut ui: AppUI,
        input: mpsc::UnboundedReceiver<Event>,
    ) -> Result<Self> {
        let Session {
            config,
            mut warnings,
            mut taskchampion,
        } = session;
        let mut sync_handler = SyncHandler::new();
        sync_handler.initialize(&taskchampion)?;
        ui.load_tasks(&mut taskchampion).await?;

        let replica_watcher = match ReplicaWatcher::new(taskchampion.data_dir()) {
            Ok(watcher) => Some(watcher),
            Err(e) => {
                warnings.push(format!("Not watching the task data, reload with F5: {e}"));
                None
            }
        };
        ui.show_config_warnings(&warnings);
        let auto_sync = match config.sync.auto_sync_interval {
            0 => None,
            secs => {
                let period = Duration::from_secs(secs);
                let mut interval = tokio::time::interval_at(Instant::now() + period, period);
                interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
                Some(interval)
            }
        };
        let mut app = App {
            config,
            terminal,
            ui,
            taskchampion,
            sync_handler,
            should_quit: false,
            input,
            replica_watcher,
            auto_sync,
            needs_redraw: true,
            _terminal_guard: None,
        };
        app.draw()?;
        Ok(app)
    }

    pub async fn run(&mut self) -> Result<()> {
        while !self.should_quit {
            self.step().await?;
        }
        Ok(())
    }

    /// Waits for the next terminal event, replica change or auto-sync tick,
    /// handles it, and redraws if needed.
    pub async fn step(&mut self) -> Result<()> {
        tokio::select! {
            event = self.input.recv() => match event {
                Some(event) => self.handle_event(event).await?,
                None => self.should_quit = true,
            },
            () = replica_changed(&mut self.replica_watcher) => {
                self.ui.reload_tasks(&mut self.taskchampion).await;
                self.needs_redraw = true;
            }
            () = auto_sync_due(&mut self.auto_sync) => {
                self.ui
                    .auto_sync(&mut self.taskchampion, &mut self.sync_handler)
                    .await;
                self.needs_redraw = true;
            }
        }
        self.draw()
    }

    async fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => {
                let action = self.ui.action(key);
                if matches!(action, Action::Quit) {
                    self.should_quit = true;
                    return Ok(());
                }
                let is_sync = matches!(action, Action::Sync | Action::ForceSync);
                self.ui
                    .handle_action(action, &mut self.taskchampion, &mut self.sync_handler)
                    .await?;
                if let (true, Some(interval)) = (is_sync, self.auto_sync.as_mut()) {
                    interval.reset();
                }
                self.needs_redraw = true;
            }
            Event::Resize(_, _) => self.needs_redraw = true,
            _ => {}
        }
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        if self.needs_redraw {
            self.terminal
                .draw(|f| self.ui.render_with_sync(f, &self.sync_handler))?;
            self.needs_redraw = false;
        }
        Ok(())
    }
}

async fn replica_changed(watcher: &mut Option<ReplicaWatcher>) {
    match watcher {
        Some(watcher) => watcher.changed().await,
        None => std::future::pending().await,
    }
}

async fn auto_sync_due(interval: &mut Option<Interval>) {
    match interval {
        Some(interval) => {
            interval.tick().await;
        }
        None => std::future::pending().await,
    }
}

/// Raw mode and the alternate screen, restored on drop.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let guard = TerminalGuard;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        );
    }
}
