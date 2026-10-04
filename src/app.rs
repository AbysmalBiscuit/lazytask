use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::time::Duration;

use crate::config::{Config, LoadedConfig};
use crate::handlers::input::Action;
use crate::handlers::sync::SyncHandler;
use crate::taskchampion::TaskChampionIntegration;
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

impl App {
    pub async fn new(config_path: Option<&str>, _verbose: bool) -> Result<Self> {
        let LoadedConfig {
            config,
            unknown_keys,
        } = Config::load(config_path)?;
        let mut ui = AppUI::new(&config)?;
        ui.show_config_warnings(&unknown_keys);
        let data_dir = config
            .taskwarrior
            .resolve_data_location(std::env::var_os("TASKDATA"), dirs::home_dir().as_deref())?;
        let taskchampion = TaskChampionIntegration::new(data_dir).await?;
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
