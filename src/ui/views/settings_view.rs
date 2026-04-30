// Configuration UI view

use ratatui::{layout::Rect, Frame};

pub struct SettingsView;

impl SettingsView {
    pub fn new() -> Self {
        SettingsView
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        use ratatui::{
            style::{Color, Style},
            text::{Line, Span},
            widgets::{Block, Borders, Paragraph},
        };

        let block = Block::default()
            .title("Settings")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));

        let content = vec![
            Line::from("Settings panel - Coming soon"),
            Line::from(""),
            Line::from(vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw(" to return"),
            ]),
        ];

        let paragraph = Paragraph::new(content).block(block);
        f.render_widget(paragraph, area);
    }
}
