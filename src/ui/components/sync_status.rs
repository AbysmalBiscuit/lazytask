// Sync status widget with progress indicators

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

use crate::handlers::sync::{SyncPhase, SyncStatus};

pub struct SyncStatusWidget;

impl SyncStatusWidget {
    pub fn new() -> Self {
        SyncStatusWidget
    }

    pub fn render(&self, f: &mut Frame, area: Rect, sync_status: Option<&SyncStatus>) {
        match sync_status {
            Some(status) => self.render_sync_status(f, area, status),
            None => self.render_no_sync(f, area),
        }
    }

    pub fn render_sync_overlay(&self, f: &mut Frame, area: Rect, sync_status: &SyncStatus) {
        if !sync_status.is_syncing {
            return;
        }

        // Create centered overlay for sync progress
        let popup_area = Self::centered_rect(60, 40, area);

        // Clear background
        f.render_widget(ratatui::widgets::Clear, popup_area);

        // Main sync dialog
        let block = Block::default()
            .title("Synchronizing with Taskserver")
            .title_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));
        f.render_widget(block, popup_area);

        let inner = popup_area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 2,
        });

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Status and phase
                Constraint::Length(2), // Progress bar
                Constraint::Length(4), // Statistics
                Constraint::Min(1),    // Instructions
            ])
            .split(inner);

        // Phase and status
        let phase_text = match sync_status.progress.phase {
            SyncPhase::Connecting => "🔗 Connecting to server...",
            SyncPhase::Uploading => "📤 Uploading local changes...",
            SyncPhase::Downloading => "📥 Downloading remote changes...",
            SyncPhase::ResolvingConflicts => "⚠️ Resolving conflicts...",
            SyncPhase::Finalizing => "✨ Finalizing sync...",
            SyncPhase::Complete => "✅ Sync complete!",
            SyncPhase::Error => "❌ Sync failed",
            SyncPhase::Idle => "💤 Ready to sync",
        };

        let status_paragraph = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Phase: ", Style::default().fg(Color::Yellow)),
                Span::raw(phase_text),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Yellow)),
                Span::raw(&sync_status.progress.message),
            ]),
        ]);
        f.render_widget(status_paragraph, chunks[0]);

        // Progress bar
        let progress_color = match sync_status.progress.phase {
            SyncPhase::Error => Color::Red,
            SyncPhase::Complete => Color::Green,
            _ => Color::Blue,
        };

        let progress_bar = Gauge::default()
            .block(Block::default().borders(Borders::ALL))
            .gauge_style(Style::default().fg(progress_color))
            .percent(sync_status.progress.progress_percent as u16)
            .label(format!("{:.0}%", sync_status.progress.progress_percent));
        f.render_widget(progress_bar, chunks[1]);

        // Statistics
        let stats_text = vec![
            Line::from(vec![
                Span::styled("Uploaded: ", Style::default().fg(Color::Green)),
                Span::raw(format!("{} tasks", sync_status.progress.tasks_uploaded)),
            ]),
            Line::from(vec![
                Span::styled("Downloaded: ", Style::default().fg(Color::Blue)),
                Span::raw(format!("{} tasks", sync_status.progress.tasks_downloaded)),
            ]),
            Line::from(vec![
                Span::styled("Conflicts: ", Style::default().fg(Color::Red)),
                Span::raw(format!("{}", sync_status.progress.conflicts_detected)),
            ]),
        ];
        let stats = Paragraph::new(stats_text);
        f.render_widget(stats, chunks[2]);

        // Instructions
        let instruction_text = if sync_status.progress.phase == SyncPhase::Error {
            "Press Esc to close | Press R to retry sync"
        } else if sync_status.progress.phase == SyncPhase::Complete {
            "Press Esc to close | Sync completed successfully"
        } else {
            "Press Esc to cancel sync"
        };

        let instructions = Paragraph::new(instruction_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        f.render_widget(instructions, chunks[3]);
    }

    fn render_sync_status(&self, f: &mut Frame, area: Rect, status: &SyncStatus) {
        let last_sync_text = if let Some(last_sync) = status.last_sync {
            let elapsed = last_sync.elapsed();
            if elapsed.as_secs() < 60 {
                format!("{}s ago", elapsed.as_secs())
            } else if elapsed.as_secs() < 3600 {
                format!("{}m ago", elapsed.as_secs() / 60)
            } else {
                format!("{}h ago", elapsed.as_secs() / 3600)
            }
        } else {
            "Never".to_string()
        };

        let mut sync_text = vec![
            Line::from(vec![
                Span::styled("Server: ", Style::default().fg(Color::Yellow)),
                if status.server_configured {
                    Span::styled("✅ Configured", Style::default().fg(Color::Green))
                } else {
                    Span::styled("❌ Not configured", Style::default().fg(Color::Red))
                },
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Yellow)),
                if status.is_syncing {
                    Span::styled("🔄 Syncing...", Style::default().fg(Color::Blue))
                } else if status.sync_error.is_some() {
                    Span::styled("❌ Error", Style::default().fg(Color::Red))
                } else {
                    Span::styled("✅ Ready", Style::default().fg(Color::Green))
                },
            ]),
            Line::from(vec![
                Span::styled("Last Sync: ", Style::default().fg(Color::Yellow)),
                Span::raw(last_sync_text),
            ]),
        ];

        if let Some(ref error) = status.sync_error {
            let error_text = crate::utils::formatting::truncate_chars(error, 40);
            sync_text.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(Color::Red)),
                Span::styled(error_text, Style::default().fg(Color::Red)),
            ]));
        }

        let sync_panel = Paragraph::new(sync_text).block(
            Block::default()
                .title("Sync Status")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

        f.render_widget(sync_panel, area);
    }

    fn render_no_sync(&self, f: &mut Frame, area: Rect) {
        let no_sync = Paragraph::new("Sync not initialized")
            .block(
                Block::default()
                    .title("Sync Status")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray)),
            )
            .style(Style::default().fg(Color::Gray));

        f.render_widget(no_sync, area);
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
}
