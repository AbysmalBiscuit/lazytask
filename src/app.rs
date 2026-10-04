use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::ffi::OsString;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use crate::config::{Config, LoadedConfig};
use crate::handlers::input::Action;
use crate::handlers::sync::SyncHandler;
use crate::taskchampion::TaskChampionIntegration;
use crate::taskrc::Taskrc;
use crate::ui::app_ui::AppUI;

pub type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

pub struct App {
    pub config: Config,
    pub terminal: AppTerminal,
    pub ui: AppUI,
    pub taskchampion: TaskChampionIntegration,
    pub sync_handler: SyncHandler,
    pub should_quit: bool,
}

/// The parts of the process environment startup reads.
pub struct LaunchEnv {
    /// `TASKRC`
    pub taskrc_var: Option<OsString>,
    /// `TASKDATA`
    pub taskdata_var: Option<OsString>,
    pub home: Option<PathBuf>,
}

impl LaunchEnv {
    pub fn from_process() -> Self {
        LaunchEnv {
            taskrc_var: std::env::var_os("TASKRC"),
            taskdata_var: std::env::var_os("TASKDATA"),
            home: dirs::home_dir(),
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
            config,
            unknown_keys,
        } = Config::load(config_path)?;
        let home = env.home.as_deref();
        let taskrc = match config
            .taskwarrior
            .resolve_taskrc_path(env.taskrc_var, home)?
        {
            Some(path) => Taskrc::load(&path, home)?,
            None => Taskrc::default(),
        };
        let data_dir = config
            .taskwarrior
            .resolve_data_location(env.taskdata_var, &taskrc, home)?;
        let mut taskchampion = TaskChampionIntegration::new(data_dir).await?;

        let mut warnings = Vec::new();
        if !unknown_keys.is_empty() {
            warnings.push(format!("Unknown config keys: {}", unknown_keys.join(", ")));
        }
        let sync = taskrc
            .sync_settings(home)
            .and_then(|settings| match settings {
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

impl App {
    pub async fn new(config_path: Option<&str>, _verbose: bool) -> Result<Self> {
        let Session {
            config,
            warnings,
            taskchampion,
        } = Session::open(config_path, LaunchEnv::from_process()).await?;
        let mut ui = AppUI::new(&config)?;
        ui.show_config_warnings(&warnings);
        let sync_handler = SyncHandler::new();

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;

        Ok(App {
            config,
            terminal,
            ui,
            taskchampion,
            sync_handler,
            should_quit: false,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        self.sync_handler.initialize(&self.taskchampion)?;
        self.ui.load_tasks(&mut self.taskchampion).await?;

        let mut needs_redraw = true;

        loop {
            if needs_redraw {
                self.terminal
                    .draw(|f| self.ui.render_with_sync(f, &self.sync_handler))?;
                needs_redraw = false;
            }

            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(key) => {
                        let action = self.ui.action(key);
                        match action {
                            Action::Quit => {
                                self.should_quit = true;
                            }
                            _ => {
                                self.ui
                                    .handle_action(
                                        action,
                                        &mut self.taskchampion,
                                        &mut self.sync_handler,
                                    )
                                    .await?;
                                needs_redraw = true;
                            }
                        }
                    }
                    Event::Resize(_, _) => {
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}
