// Modal widget for configuring TaskChampion sync.

use anyhow::Result;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::handlers::input::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncConfigType {
    Server,
    Local,
    GCP,
    AWS,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub config_type: SyncConfigType,
    pub server_url: String,
    pub client_id: String,
    pub encryption_secret: String,
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
    server_url: String,
    client_id: String,
    encryption_secret: String,
}

impl SyncConfigWidget {
    pub fn new() -> Self {
        SyncConfigWidget {
            active: false,
            field: Field::ServerUrl,
            server_url: String::new(),
            client_id: String::new(),
            encryption_secret: String::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.field = Field::ServerUrl;
        self.server_url.clear();
        self.client_id.clear();
        self.encryption_secret.clear();
    }

    pub fn handle_input(&mut self, action: Action) -> Result<Option<SyncConfigResult>> {
        if !self.active {
            return Ok(None);
        }

        match action {
            Action::Back => return Ok(Some(SyncConfigResult::Cancel)),
            Action::Select => {
                let config = SyncConfig {
                    config_type: SyncConfigType::Server,
                    server_url: self.server_url.trim().to_string(),
                    client_id: self.client_id.trim().to_string(),
                    encryption_secret: self.encryption_secret.clone(),
                };
                return Ok(Some(SyncConfigResult::Save(config)));
            }
            Action::Tab | Action::MoveDown => self.field = self.field.next(),
            Action::MoveUp => self.field = self.field.prev(),
            Action::Backspace => {
                self.current_input_mut().pop();
            }
            Action::Character(c) => {
                self.current_input_mut().push(c);
            }
            Action::Space => {
                self.current_input_mut().push(' ');
            }
            _ => {}
        }
        Ok(None)
    }

    fn current_input_mut(&mut self) -> &mut String {
        match self.field {
            Field::ServerUrl => &mut self.server_url,
            Field::ClientId => &mut self.client_id,
            Field::EncryptionSecret => &mut self.encryption_secret,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.active {
            return;
        }

        let popup = centered_rect(60, 50, area);
        f.render_widget(Clear, popup);

        let block = Block::default()
            .title("Configure Sync")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
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

        self.render_field(f, chunks[0], Field::ServerUrl, &self.server_url, false);
        self.render_field(f, chunks[1], Field::ClientId, &self.client_id, false);
        self.render_field(
            f,
            chunks[2],
            Field::EncryptionSecret,
            &self.encryption_secret,
            true,
        );

        let hint = Paragraph::new(Line::from(vec![
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(" next  "),
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(Color::Red)),
            Span::raw(" cancel"),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(hint, chunks[3]);
    }

    fn render_field(&self, f: &mut Frame, area: Rect, field: Field, value: &str, mask: bool) {
        let active = matches!(
            (field, self.field),
            (Field::ServerUrl, Field::ServerUrl)
                | (Field::ClientId, Field::ClientId)
                | (Field::EncryptionSecret, Field::EncryptionSecret)
        );
        let border_color = if active { Color::Yellow } else { Color::Gray };
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
