// Modal widget for configuring TaskChampion sync.

use anyhow::Result;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::handlers::input::Action;
use crate::taskchampion::ServerSettings;
use crate::ui::theme::Theme;

/// What the modal edits.
#[derive(Debug, Clone, Default)]
pub struct SyncConfig {
    pub server: ServerSettings,
}

#[derive(Debug, Clone)]
pub enum SyncConfigResult {
    Save(SyncConfig),
    Cancel,
}

#[derive(Copy, Clone)]
enum Field {
    ServerUrl,
    ClientId,
    EncryptionSecret,
}

impl Field {
    fn next(self) -> Self {
        match self {
            Field::ServerUrl => Field::ClientId,
            Field::ClientId => Field::EncryptionSecret,
            Field::EncryptionSecret => Field::ServerUrl,
        }
    }

    fn prev(self) -> Self {
        match self {
            Field::ServerUrl => Field::EncryptionSecret,
            Field::ClientId => Field::ServerUrl,
            Field::EncryptionSecret => Field::ClientId,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Field::ServerUrl => "Server URL",
            Field::ClientId => "Client ID (UUID)",
            Field::EncryptionSecret => "Encryption Secret",
        }
    }
}

pub struct SyncConfigWidget {
    active: bool,
    field: Field,
    values: SyncConfig,
}

impl SyncConfigWidget {
    pub fn new() -> Self {
        SyncConfigWidget {
            active: false,
            field: Field::ServerUrl,
            values: SyncConfig::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Opens the modal with its fields filled in from `saved`.
    pub fn activate(&mut self, saved: SyncConfig) {
        self.active = true;
        self.values = saved;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.field = Field::ServerUrl;
        self.values = SyncConfig::default();
    }

    pub fn handle_input(&mut self, action: Action) -> Result<Option<SyncConfigResult>> {
        if !self.active {
            return Ok(None);
        }

        match action {
            Action::Back => return Ok(Some(SyncConfigResult::Cancel)),
            Action::Select => {
                let server = &self.values.server;
                let config = SyncConfig {
                    server: ServerSettings {
                        url: server.url.trim().to_string(),
                        client_id: server.client_id.trim().to_string(),
                        encryption_secret: server.encryption_secret.clone(),
                    },
                };
                return Ok(Some(SyncConfigResult::Save(config)));
            }
            Action::NextField | Action::MoveDown => self.field = self.field.next(),
            Action::MoveUp => self.field = self.field.prev(),
            Action::Erase => {
                self.current_input_mut().pop();
            }
            Action::Character(c) => {
                self.current_input_mut().push(c);
            }
            Action::Toggle => {
                self.current_input_mut().push(' ');
            }
            _ => {}
        }
        Ok(None)
    }

    fn current_input_mut(&mut self) -> &mut String {
        match self.field {
            Field::ServerUrl => &mut self.values.server.url,
            Field::ClientId => &mut self.values.server.client_id,
            Field::EncryptionSecret => &mut self.values.server.encryption_secret,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        if !self.active {
            return;
        }

        let popup = centered_rect(60, 50, area);
        f.render_widget(Clear, popup);

        let block = Block::default()
            .title("Configure Sync")
            .title_style(
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.primary))
            .style(theme.base());
        f.render_widget(block, popup);

        let inner = popup.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 2,
        });

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .split(inner);

        let server = &self.values.server;
        self.render_field(f, chunks[0], Field::ServerUrl, &server.url, false, theme);
        self.render_field(f, chunks[1], Field::ClientId, &server.client_id, false, theme);
        self.render_field(
            f,
            chunks[2],
            Field::EncryptionSecret,
            &server.encryption_secret,
            true,
            theme,
        );

        let hint = Paragraph::new(Line::from(vec![
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::raw(" next  "),
            Span::styled("Enter", Style::default().fg(theme.success)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(theme.error)),
            Span::raw(" cancel"),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(hint, chunks[3]);
    }

    fn render_field(
        &self,
        f: &mut Frame,
        area: Rect,
        field: Field,
        value: &str,
        mask: bool,
        theme: &Theme,
    ) {
        let active = matches!(
            (field, self.field),
            (Field::ServerUrl, Field::ServerUrl)
                | (Field::ClientId, Field::ClientId)
                | (Field::EncryptionSecret, Field::EncryptionSecret)
        );
        let border_color = if active { theme.accent } else { theme.muted };
        let display: String = if mask {
            "*".repeat(value.chars().count())
        } else {
            value.to_string()
        };
        let para = Paragraph::new(display).block(
            Block::default()
                .title(field.label())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );
        f.render_widget(para, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
