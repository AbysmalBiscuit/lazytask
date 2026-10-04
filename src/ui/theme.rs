use ratatui::style::{Color, Modifier, Style};

use crate::config::ThemeConfig;

/// The only built-in palette, and the one an unknown `theme.name` falls
/// back to.
pub const DEFAULT_THEME_NAME: &str = "catppuccin-mocha";

/// The color of each role the UI draws with. Views and components take
/// their colors from here, never from literal colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    /// De-emphasized text and inactive borders.
    pub muted: Color,
    /// Titles, labels and borders.
    pub primary: Color,
    /// Tags and the second tier of emphasis.
    pub secondary: Color,
    /// Key hints and the focused element.
    pub accent: Color,
    pub info: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    /// Background of the selected row.
    pub selection: Color,
    pub priority_high: Color,
    pub priority_medium: Color,
    pub priority_low: Color,
}

impl Theme {
    pub const CATPPUCCIN_MOCHA: Theme = Theme {
        background: Color::Rgb(0x1e, 0x1e, 0x2e),
        foreground: Color::Rgb(0xcd, 0xd6, 0xf4),
        muted: Color::Rgb(0x7f, 0x84, 0x9c),
        primary: Color::Rgb(0x89, 0xb4, 0xfa),
        secondary: Color::Rgb(0xcb, 0xa6, 0xf7),
        accent: Color::Rgb(0xfa, 0xb3, 0x87),
        info: Color::Rgb(0x89, 0xdc, 0xeb),
        success: Color::Rgb(0xa6, 0xe3, 0xa1),
        warning: Color::Rgb(0xf9, 0xe2, 0xaf),
        error: Color::Rgb(0xf3, 0x8b, 0xa8),
        selection: Color::Rgb(0x45, 0x47, 0x5a),
        priority_high: Color::Rgb(0xf3, 0x8b, 0xa8),
        priority_medium: Color::Rgb(0xf9, 0xe2, 0xaf),
        priority_low: Color::Rgb(0xa6, 0xe3, 0xa1),
    };

    /// Every role left to the terminal's default color, for `NO_COLOR`.
    pub const NO_COLOR: Theme = Theme {
        background: Color::Reset,
        foreground: Color::Reset,
        muted: Color::Reset,
        primary: Color::Reset,
        secondary: Color::Reset,
        accent: Color::Reset,
        info: Color::Reset,
        success: Color::Reset,
        warning: Color::Reset,
        error: Color::Reset,
        selection: Color::Reset,
        priority_high: Color::Reset,
        priority_medium: Color::Reset,
        priority_low: Color::Reset,
    };

    /// The theme `config` names, with its `colors` overrides applied, plus a
    /// warning for each part of `config` that could not be used. An unknown
    /// name uses the default palette; an unknown role or unparseable color
    /// is skipped, so that role keeps its palette color.
    pub fn from_config(config: &ThemeConfig) -> (Theme, Vec<String>) {
        let mut theme = Theme::CATPPUCCIN_MOCHA;
        let mut warnings = Vec::new();
        if config.name != DEFAULT_THEME_NAME {
            warnings.push(format!(
                "Unknown theme.name \"{}\", using {DEFAULT_THEME_NAME}",
                config.name
            ));
        }

        let mut overrides: Vec<_> = config.colors.iter().collect();
        overrides.sort();
        let mut unknown_roles = Vec::new();
        for (role, value) in overrides {
            let Some(slot) = theme.role_mut(role) else {
                unknown_roles.push(role.as_str());
                continue;
            };
            match value.parse() {
                Ok(color) => *slot = color,
                Err(_) => warnings.push(format!(
                    "Invalid color \"{value}\" for theme.colors.{role}, using the default"
                )),
            }
        }
        if !unknown_roles.is_empty() {
            warnings.push(format!(
                "Unknown theme.colors skipped: {}",
                unknown_roles.join(", ")
            ));
        }

        if config.no_color {
            theme = Theme::NO_COLOR;
        }
        (theme, warnings)
    }

    fn role_mut(&mut self, role: &str) -> Option<&mut Color> {
        Some(match role {
            "background" => &mut self.background,
            "foreground" => &mut self.foreground,
            "muted" => &mut self.muted,
            "primary" => &mut self.primary,
            "secondary" => &mut self.secondary,
            "accent" => &mut self.accent,
            "info" => &mut self.info,
            "success" => &mut self.success,
            "warning" => &mut self.warning,
            "error" => &mut self.error,
            "selection" => &mut self.selection,
            "priority_high" => &mut self.priority_high,
            "priority_medium" => &mut self.priority_medium,
            "priority_low" => &mut self.priority_low,
            _ => return None,
        })
    }

    /// Plain text on the theme's background.
    pub fn base(&self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }

    /// The selected row. Without a selection color, the row shows reversed
    /// so it stays visible.
    pub fn selected(&self) -> Style {
        match self.selection {
            Color::Reset => Style::default().add_modifier(Modifier::REVERSED),
            selection => Style::default().bg(selection).fg(self.foreground),
        }
    }
}
