use crossterm::event::KeyEvent;

use crate::utils::keybindings::{InputContext, Keymap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    Refresh,
    Help,
    AddTask,
    EditTask,
    DoneTask,
    DeleteTask,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Select,
    Back,
    Filter,
    ToggleCalendar,
    PrevMonth,
    NextMonth,
    Today,
    Reports,
    Sync,
    ForceSync,
    SyncConfig,
    Character(char),
    Backspace,
    None,
    Space,
    Tab,
}

pub struct InputHandler {
    keymap: Keymap,
}

impl InputHandler {
    pub fn new(config: &crate::config::Config) -> Self {
        let (keymap, _) = Keymap::from_config(&config.keybindings);
        InputHandler { keymap }
    }

    pub fn handle_key_event_with_context(&self, key: KeyEvent, context: InputContext) -> Action {
        self.keymap.action(context, key)
    }
}
