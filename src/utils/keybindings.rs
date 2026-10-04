//! The keymap: which key triggers which action, built from the built-in
//! defaults and the `[keybindings]` config sections.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::KeyBindingsConfig;
use crate::handlers::input::Action;

/// A `[keybindings.<section>]` config table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Global,
    TaskList,
    Reports,
    Form,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::Global,
        Section::TaskList,
        Section::Reports,
        Section::Form,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Section::Global => "global",
            Section::TaskList => "task_list",
            Section::Reports => "reports",
            Section::Form => "form",
        }
    }

    fn config(self, config: &KeyBindingsConfig) -> &HashMap<String, String> {
        match self {
            Section::Global => &config.global,
            Section::TaskList => &config.task_list,
            Section::Reports => &config.reports,
            Section::Form => &config.form,
        }
    }
}

/// Where keyboard input is going, which decides the sections searched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    TaskList,
    Reports,
    /// A view with no keys of its own, such as the help overlay.
    Other,
    /// A text-entry form: the task form, the filter panel or sync setup.
    Form,
}

impl InputContext {
    /// The sections searched, most specific first, so a view's own binding
    /// shadows a global one on the same key.
    fn sections(self) -> &'static [Section] {
        match self {
            InputContext::TaskList => &[Section::TaskList, Section::Global],
            InputContext::Reports => &[Section::Reports, Section::Global],
            InputContext::Other => &[Section::Global],
            InputContext::Form => &[Section::Form],
        }
    }
}

/// A bindable action: its config name, what it does and its default key.
#[derive(Debug)]
pub struct ActionSpec {
    pub section: Section,
    pub name: &'static str,
    pub action: Action,
    pub default: Option<&'static str>,
    pub description: &'static str,
}

const fn spec(
    section: Section,
    name: &'static str,
    action: Action,
    default: Option<&'static str>,
    description: &'static str,
) -> ActionSpec {
    ActionSpec {
        section,
        name,
        action,
        default,
        description,
    }
}

use Section::{Form, Global, Reports, TaskList};

/// Every action a key can trigger, in the order the help overlay lists them.
#[rustfmt::skip]
pub static ACTIONS: &[ActionSpec] = &[
    spec(Global, "quit", Action::Quit, Some("q"), "Quit"),
    spec(Global, "force_quit", Action::Quit, Some("Ctrl+c"), "Quit"),
    spec(Global, "help", Action::Help, Some("F1"), "Show this help"),
    spec(Global, "refresh", Action::Refresh, Some("F5"), "Reload tasks from replica"),
    spec(Global, "back", Action::Back, Some("Esc"), "Back to the task list"),
    spec(Global, "reports", Action::Reports, Some("r"), "Open Reports view"),
    spec(Global, "sync", Action::Sync, Some("s"), "Sync (needs sync config)"),
    spec(Global, "force_sync", Action::ForceSync, None, "Force a full sync"),
    spec(Global, "sync_config", Action::SyncConfig, Some("S"), "Open Sync Config modal"),
    spec(TaskList, "move_up", Action::MoveUp, Some("Up"), "Select previous task"),
    spec(TaskList, "move_down", Action::MoveDown, Some("Down"), "Select next task"),
    spec(TaskList, "add_task", Action::AddTask, Some("a"), "Add new task"),
    spec(TaskList, "edit_task", Action::EditTask, Some("e"), "Edit selected task"),
    spec(TaskList, "done_task", Action::DoneTask, Some("d"), "Mark task done"),
    spec(TaskList, "delete_task", Action::DeleteTask, Some("Delete"), "Soft-delete task"),
    spec(TaskList, "filter", Action::Filter, Some("/"), "Open filter panel"),
    spec(Reports, "toggle_calendar", Action::ToggleCalendar, Some("c"), "Toggle Calendar / Dashboard"),
    spec(Reports, "prev_day", Action::MoveLeft, Some("Left"), "Calendar: previous day"),
    spec(Reports, "next_day", Action::MoveRight, Some("Right"), "Calendar: next day"),
    spec(Reports, "prev_week", Action::MoveUp, Some("Up"), "Calendar: previous week"),
    spec(Reports, "next_week", Action::MoveDown, Some("Down"), "Calendar: next week"),
    spec(Reports, "prev_month", Action::PrevMonth, Some("<"), "Calendar: previous month"),
    spec(Reports, "next_month", Action::NextMonth, Some(">"), "Calendar: next month"),
    spec(Reports, "today", Action::Today, Some("t"), "Calendar: jump to today"),
    spec(Form, "next_field", Action::Tab, Some("Tab"), "Next field / filter section"),
    spec(Form, "prev_field", Action::MoveUp, Some("Shift+Tab"), "Previous field"),
    spec(Form, "move_up", Action::MoveUp, Some("Up"), "Previous field / filter item"),
    spec(Form, "move_down", Action::MoveDown, Some("Down"), "Next field / filter item"),
    spec(Form, "move_left", Action::MoveLeft, Some("Left"), "Move cursor left"),
    spec(Form, "move_right", Action::MoveRight, Some("Right"), "Move cursor right"),
    spec(Form, "toggle", Action::Space, Some("Space"), "Toggle filter item"),
    spec(Form, "erase", Action::Backspace, Some("Backspace"), "Erase a character"),
    spec(Form, "confirm", Action::Select, Some("Enter"), "Commit field, then save"),
    spec(Form, "cancel", Action::Back, Some("Esc"), "Cancel / close"),
];

/// A key with its modifiers, written in config as `Ctrl+s`, `Shift+Tab`,
/// `F1` or `q`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Key {
    /// Folds Shift into the key itself, the way terminals report it: a
    /// shifted letter arrives as its capital and Shift+Tab as BackTab.
    fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        match code {
            KeyCode::Char(c) if modifiers.contains(KeyModifiers::SHIFT) => Key {
                code: KeyCode::Char(c.to_ascii_uppercase()),
                modifiers: modifiers - KeyModifiers::SHIFT,
            },
            KeyCode::Tab if modifiers.contains(KeyModifiers::SHIFT) => Key {
                code: KeyCode::BackTab,
                modifiers: modifiers - KeyModifiers::SHIFT,
            },
            KeyCode::BackTab => Key {
                code,
                modifiers: modifiers - KeyModifiers::SHIFT,
            },
            _ => Key { code, modifiers },
        }
    }
}

impl From<KeyEvent> for Key {
    fn from(event: KeyEvent) -> Self {
        Key::new(event.code, event.modifiers)
    }
}

const NAMED_KEYS: &[(&str, KeyCode)] = &[
    ("Enter", KeyCode::Enter),
    ("Esc", KeyCode::Esc),
    ("Tab", KeyCode::Tab),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PageUp", KeyCode::PageUp),
    ("PageDown", KeyCode::PageDown),
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("Space", KeyCode::Char(' ')),
];

impl FromStr for Key {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        // A trailing `+` after a separator is the `+` key itself: `Ctrl++`.
        let (modifier_names, key_name) = match s.rsplit_once('+') {
            None => ("", s),
            Some(("", "")) => ("", "+"),
            Some((prefix, "")) => (prefix.strip_suffix('+').ok_or(())?, "+"),
            Some(split) => split,
        };

        let mut modifiers = KeyModifiers::NONE;
        if !modifier_names.is_empty() {
            for name in modifier_names.split('+') {
                modifiers |= match name.to_ascii_lowercase().as_str() {
                    "ctrl" => KeyModifiers::CONTROL,
                    "alt" => KeyModifiers::ALT,
                    "shift" => KeyModifiers::SHIFT,
                    _ => return Err(()),
                };
            }
        }

        let mut chars = key_name.chars();
        let code = match (chars.next(), chars.next()) {
            (Some(c), None) => KeyCode::Char(c),
            _ => NAMED_KEYS
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key_name))
                .map(|&(_, code)| code)
                .or_else(|| {
                    let n = key_name.strip_prefix(['F', 'f'])?.parse().ok()?;
                    (1..=12).contains(&n).then_some(KeyCode::F(n))
                })
                .ok_or(())?,
        };
        Ok(Key::new(code, modifiers))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            f.write_str("Ctrl+")?;
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            f.write_str("Alt+")?;
        }
        match self.code {
            KeyCode::BackTab => f.write_str("Shift+Tab"),
            KeyCode::F(n) => write!(f, "F{n}"),
            code => match NAMED_KEYS.iter().find(|&&(_, named)| named == code) {
                Some((name, _)) => f.write_str(name),
                None => match code {
                    KeyCode::Char(c) => write!(f, "{c}"),
                    other => write!(f, "{other:?}"),
                },
            },
        }
    }
}

/// The keys in effect: each action's configured key, or its default.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(&'static ActionSpec, Key)>,
}

impl Keymap {
    /// Builds the keymap from the config, with a warning for each config
    /// entry it could not use. A configured key wins over another action's
    /// default on that key; an entry that cannot be used leaves its action
    /// on its default.
    pub fn from_config(config: &KeyBindingsConfig) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut configured: Vec<(&'static ActionSpec, Key)> = Vec::new();

        for section in Section::ALL {
            let mut entries: Vec<_> = section.config(config).iter().collect();
            entries.sort();
            for (name, value) in entries {
                let path = format!("keybindings.{}.{name}", section.name());
                let Some(spec) = ACTIONS
                    .iter()
                    .find(|spec| spec.section == section && spec.name == name)
                else {
                    warnings.push(format!("{path}: unknown action"));
                    continue;
                };
                let Ok(key) = value.parse::<Key>() else {
                    warnings.push(format!("{path}: cannot parse key {value:?}"));
                    continue;
                };
                if let Some((taken_by, _)) = configured
                    .iter()
                    .find(|(other, k)| other.section == section && *k == key)
                {
                    warnings.push(format!(
                        "{path}: {value:?} is already bound to {}",
                        taken_by.name
                    ));
                    continue;
                }
                configured.push((spec, key));
            }
        }

        let bindings = ACTIONS
            .iter()
            .filter_map(|spec| {
                if let Some(&(_, key)) = configured
                    .iter()
                    .find(|(s, _)| s.section == spec.section && s.name == spec.name)
                {
                    return Some((spec, key));
                }
                let key = spec.default?.parse::<Key>().expect("default keys parse");
                let taken = configured
                    .iter()
                    .any(|(other, k)| other.section == spec.section && *k == key);
                (!taken).then_some((spec, key))
            })
            .collect();

        (Keymap { bindings }, warnings)
    }

    /// The action `event` triggers in `context`. In a form, a printable key
    /// that is not bound types itself.
    pub fn action(&self, context: InputContext, event: KeyEvent) -> Action {
        let key = Key::from(event);
        let bound = context.sections().iter().find_map(|&section| {
            self.bindings
                .iter()
                .find(|(spec, k)| spec.section == section && *k == key)
                .map(|(spec, _)| spec.action.clone())
        });
        match (bound, event.code) {
            (Some(action), _) => action,
            (None, KeyCode::Char(c)) if context == InputContext::Form => Action::Character(c),
            (None, _) => Action::None,
        }
    }

    /// The key bound to the action `name` in `section`, if any.
    pub fn key(&self, section: Section, name: &str) -> Option<Key> {
        self.bindings
            .iter()
            .find(|(spec, _)| spec.section == section && spec.name == name)
            .map(|&(_, key)| key)
    }

    /// The bindings in `section`, in help-overlay order.
    pub fn bindings(
        &self,
        section: Section,
    ) -> impl Iterator<Item = (&'static ActionSpec, Key)> + '_ {
        self.bindings
            .iter()
            .filter(move |(spec, _)| spec.section == section)
            .copied()
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap::from_config(&KeyBindingsConfig::default()).0
    }
}
