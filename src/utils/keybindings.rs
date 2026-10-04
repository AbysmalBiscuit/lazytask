//! The keymap: which key triggers which action, built from the built-in
//! defaults and the `[keybindings]` config sections.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use enum_dispatch::enum_dispatch;
use strum::{EnumDiscriminants, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use crate::config::KeyBindingsConfig;
use crate::handlers::input::Action;

/// What every bindable action knows about itself.
#[enum_dispatch]
pub trait Bindable {
    /// The command a key bound to this action sends.
    fn action(&self) -> Action;
    /// The default key string, if the action has one.
    fn default_key(&self) -> Option<&'static str>;
    /// The help-overlay text.
    fn description(&self) -> &'static str;
    /// The name used in config, such as `quit`.
    fn name(&self) -> &'static str;
}

/// Actions in `[keybindings.global]`, active in every view outside a form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum GlobalAction {
    Quit,
    ForceQuit,
    Help,
    Refresh,
    Back,
    Reports,
    Sync,
    ForceSync,
    SyncConfig,
}

/// Actions in `[keybindings.task_list]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum TaskListAction {
    MoveUp,
    MoveDown,
    AddTask,
    EditTask,
    DoneTask,
    DeleteTask,
    Filter,
}

/// Actions in `[keybindings.reports]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ReportsAction {
    ToggleCalendar,
    PrevDay,
    NextDay,
    PrevWeek,
    NextWeek,
    PrevMonth,
    NextMonth,
    Today,
}

/// Actions in `[keybindings.form]`, active while a form or the filter
/// panel has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum FormAction {
    NextField,
    PrevField,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Toggle,
    Erase,
    Confirm,
    Cancel,
}

impl Bindable for GlobalAction {
    fn action(&self) -> Action {
        match self {
            GlobalAction::Quit | GlobalAction::ForceQuit => Action::Quit,
            GlobalAction::Help => Action::Help,
            GlobalAction::Refresh => Action::Refresh,
            GlobalAction::Back => Action::Back,
            GlobalAction::Reports => Action::Reports,
            GlobalAction::Sync => Action::Sync,
            GlobalAction::ForceSync => Action::ForceSync,
            GlobalAction::SyncConfig => Action::SyncConfig,
        }
    }

    fn default_key(&self) -> Option<&'static str> {
        match self {
            GlobalAction::Quit => Some("q"),
            GlobalAction::ForceQuit => Some("Ctrl+c"),
            GlobalAction::Help => Some("F1"),
            GlobalAction::Refresh => Some("F5"),
            GlobalAction::Back => Some("Esc"),
            GlobalAction::Reports => Some("r"),
            GlobalAction::Sync => Some("s"),
            GlobalAction::ForceSync => None,
            GlobalAction::SyncConfig => Some("S"),
        }
    }

    fn description(&self) -> &'static str {
        match self {
            GlobalAction::Quit | GlobalAction::ForceQuit => "Quit",
            GlobalAction::Help => "Show this help",
            GlobalAction::Refresh => "Reload tasks from replica",
            GlobalAction::Back => "Back to the task list",
            GlobalAction::Reports => "Open Reports view",
            GlobalAction::Sync => "Sync (needs sync config)",
            GlobalAction::ForceSync => "Force a full sync",
            GlobalAction::SyncConfig => "Open Sync Config modal",
        }
    }

    fn name(&self) -> &'static str {
        self.into()
    }
}

impl Bindable for TaskListAction {
    fn action(&self) -> Action {
        match self {
            TaskListAction::MoveUp => Action::MoveUp,
            TaskListAction::MoveDown => Action::MoveDown,
            TaskListAction::AddTask => Action::AddTask,
            TaskListAction::EditTask => Action::EditTask,
            TaskListAction::DoneTask => Action::DoneTask,
            TaskListAction::DeleteTask => Action::DeleteTask,
            TaskListAction::Filter => Action::Filter,
        }
    }

    fn default_key(&self) -> Option<&'static str> {
        Some(match self {
            TaskListAction::MoveUp => "Up",
            TaskListAction::MoveDown => "Down",
            TaskListAction::AddTask => "a",
            TaskListAction::EditTask => "e",
            TaskListAction::DoneTask => "d",
            TaskListAction::DeleteTask => "Delete",
            TaskListAction::Filter => "/",
        })
    }

    fn description(&self) -> &'static str {
        match self {
            TaskListAction::MoveUp => "Select previous task",
            TaskListAction::MoveDown => "Select next task",
            TaskListAction::AddTask => "Add new task",
            TaskListAction::EditTask => "Edit selected task",
            TaskListAction::DoneTask => "Mark task done",
            TaskListAction::DeleteTask => "Soft-delete task",
            TaskListAction::Filter => "Open filter panel",
        }
    }

    fn name(&self) -> &'static str {
        self.into()
    }
}

impl Bindable for ReportsAction {
    fn action(&self) -> Action {
        match self {
            ReportsAction::ToggleCalendar => Action::ToggleCalendar,
            ReportsAction::PrevDay => Action::MoveLeft,
            ReportsAction::NextDay => Action::MoveRight,
            ReportsAction::PrevWeek => Action::MoveUp,
            ReportsAction::NextWeek => Action::MoveDown,
            ReportsAction::PrevMonth => Action::PrevMonth,
            ReportsAction::NextMonth => Action::NextMonth,
            ReportsAction::Today => Action::Today,
        }
    }

    fn default_key(&self) -> Option<&'static str> {
        Some(match self {
            ReportsAction::ToggleCalendar => "c",
            ReportsAction::PrevDay => "Left",
            ReportsAction::NextDay => "Right",
            ReportsAction::PrevWeek => "Up",
            ReportsAction::NextWeek => "Down",
            ReportsAction::PrevMonth => "<",
            ReportsAction::NextMonth => ">",
            ReportsAction::Today => "t",
        })
    }

    fn description(&self) -> &'static str {
        match self {
            ReportsAction::ToggleCalendar => "Toggle Calendar / Dashboard",
            ReportsAction::PrevDay => "Calendar: previous day",
            ReportsAction::NextDay => "Calendar: next day",
            ReportsAction::PrevWeek => "Calendar: previous week",
            ReportsAction::NextWeek => "Calendar: next week",
            ReportsAction::PrevMonth => "Calendar: previous month",
            ReportsAction::NextMonth => "Calendar: next month",
            ReportsAction::Today => "Calendar: jump to today",
        }
    }

    fn name(&self) -> &'static str {
        self.into()
    }
}

impl Bindable for FormAction {
    fn action(&self) -> Action {
        match self {
            FormAction::NextField => Action::NextField,
            FormAction::PrevField | FormAction::MoveUp => Action::MoveUp,
            FormAction::MoveDown => Action::MoveDown,
            FormAction::MoveLeft => Action::MoveLeft,
            FormAction::MoveRight => Action::MoveRight,
            FormAction::Toggle => Action::Toggle,
            FormAction::Erase => Action::Erase,
            FormAction::Confirm => Action::Select,
            FormAction::Cancel => Action::Back,
        }
    }

    fn default_key(&self) -> Option<&'static str> {
        Some(match self {
            FormAction::NextField => "Tab",
            FormAction::PrevField => "Shift+Tab",
            FormAction::MoveUp => "Up",
            FormAction::MoveDown => "Down",
            FormAction::MoveLeft => "Left",
            FormAction::MoveRight => "Right",
            FormAction::Toggle => "Space",
            FormAction::Erase => "Backspace",
            FormAction::Confirm => "Enter",
            FormAction::Cancel => "Esc",
        })
    }

    fn description(&self) -> &'static str {
        match self {
            FormAction::NextField => "Next field / filter section",
            FormAction::PrevField => "Previous field",
            FormAction::MoveUp => "Previous field / filter item",
            FormAction::MoveDown => "Next field / filter item",
            FormAction::MoveLeft => "Move cursor left",
            FormAction::MoveRight => "Move cursor right",
            FormAction::Toggle => "Toggle filter item",
            FormAction::Erase => "Erase a character",
            FormAction::Confirm => "Commit field, then save",
            FormAction::Cancel => "Cancel / close",
        }
    }

    fn name(&self) -> &'static str {
        self.into()
    }
}

/// Any bindable action, tagged with its config section.
#[enum_dispatch(Bindable)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(Section),
    doc = "A `[keybindings.<section>]` config table.",
    derive(Hash, EnumIter, IntoStaticStr),
    strum(serialize_all = "snake_case")
)]
pub enum Binding {
    Global(GlobalAction),
    TaskList(TaskListAction),
    Reports(ReportsAction),
    Form(FormAction),
}

impl Binding {
    /// Every action, in the order the help overlay lists them.
    pub fn all() -> impl Iterator<Item = Binding> {
        GlobalAction::iter()
            .map(Binding::from)
            .chain(TaskListAction::iter().map(Binding::from))
            .chain(ReportsAction::iter().map(Binding::from))
            .chain(FormAction::iter().map(Binding::from))
    }

    /// The action called `name` in `section`.
    fn parse(section: Section, name: &str) -> Option<Binding> {
        match section {
            Section::Global => name.parse::<GlobalAction>().ok().map(Binding::from),
            Section::TaskList => name.parse::<TaskListAction>().ok().map(Binding::from),
            Section::Reports => name.parse::<ReportsAction>().ok().map(Binding::from),
            Section::Form => name.parse::<FormAction>().ok().map(Binding::from),
        }
    }

    pub fn section(&self) -> Section {
        self.into()
    }

    /// The dotted config path, such as `keybindings.global.quit`.
    pub fn path(&self) -> String {
        format!("keybindings.{}.{}", self.section().name(), self.name())
    }
}

impl Section {
    pub fn name(self) -> &'static str {
        self.into()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
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
    bindings: Vec<(Binding, Key)>,
}

impl Keymap {
    /// Builds the keymap from the config, with a warning for each config
    /// entry it could not use. A configured key wins over another action's
    /// default on that key; an entry that cannot be used leaves its action
    /// on its default.
    pub fn from_config(config: &KeyBindingsConfig) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut configured: Vec<(Binding, Key)> = Vec::new();

        for section in Section::iter() {
            let mut entries: Vec<_> = section.config(config).iter().collect();
            entries.sort();
            for (name, value) in entries {
                let path = format!("keybindings.{}.{name}", section.name());
                let Some(binding) = Binding::parse(section, name) else {
                    warnings.push(format!("{path}: unknown action"));
                    continue;
                };
                let Ok(key) = value.parse::<Key>() else {
                    warnings.push(format!("{path}: cannot parse key {value:?}"));
                    continue;
                };
                if let Some((taken_by, _)) = configured
                    .iter()
                    .find(|(other, k)| other.section() == section && *k == key)
                {
                    warnings.push(format!(
                        "{path}: {value:?} is already bound to {}",
                        taken_by.name()
                    ));
                    continue;
                }
                configured.push((binding, key));
            }
        }

        let bindings = Binding::all()
            .filter_map(|binding| {
                if let Some(&(_, key)) = configured.iter().find(|(b, _)| *b == binding) {
                    return Some((binding, key));
                }
                let key = binding
                    .default_key()?
                    .parse::<Key>()
                    .expect("default keys parse");
                let taken = configured
                    .iter()
                    .any(|(other, k)| other.section() == binding.section() && *k == key);
                (!taken).then_some((binding, key))
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
                .find(|(binding, k)| binding.section() == section && *k == key)
                .map(|(binding, _)| binding.action())
        });
        match (bound, event.code) {
            (Some(action), _) => action,
            (None, KeyCode::Char(c)) if context == InputContext::Form => Action::Character(c),
            (None, _) => Action::None,
        }
    }

    /// The key bound to `binding`, if any.
    pub fn key(&self, binding: impl Into<Binding>) -> Option<Key> {
        let binding = binding.into();
        self.bindings
            .iter()
            .find(|(b, _)| *b == binding)
            .map(|&(_, key)| key)
    }

    /// The bindings in `section`, in help-overlay order.
    pub fn bindings(&self, section: Section) -> impl Iterator<Item = (Binding, Key)> + '_ {
        self.bindings
            .iter()
            .filter(move |(binding, _)| binding.section() == section)
            .copied()
    }
}
