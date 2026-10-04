// Comprehensive task detail view component

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::data::models::{Priority, Task, TaskStatus};
use crate::ui::theme::Theme;

pub struct TaskDetailWidget;

impl TaskDetailWidget {
    pub fn new() -> Self {
        TaskDetailWidget
    }

    pub fn render(&self, f: &mut Frame, area: Rect, task: Option<&Task>, theme: &Theme) {
        if let Some(task) = task {
            self.render_task_details(f, area, task, theme);
        } else {
            let placeholder = Paragraph::new("Select a task to view details")
                .block(Block::default().title("Task Details").borders(Borders::ALL))
                .style(Style::default().fg(theme.muted));
            f.render_widget(placeholder, area);
        }
    }

    fn render_task_details(&self, f: &mut Frame, area: Rect, task: &Task, theme: &Theme) {
        // Split the area into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(20), // Main details section
                Constraint::Min(5),  // Modification history
            ])
            .split(area);

        // Render main details
        self.render_main_details(f, chunks[0], task, theme);

        // Render modification history
        self.render_modification_history(f, chunks[1], task, theme);
    }

    fn render_main_details(&self, f: &mut Frame, area: Rect, task: &Task, theme: &Theme) {
        let mut lines = Vec::new();

        // Header
        lines.push(Line::from(vec![
            Span::styled(
                "Name",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("          "),
            Span::styled(
                "Value",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        // ID
        lines.push(Line::from(vec![
            Span::styled("ID            ", Style::default().fg(theme.primary)),
            Span::styled(
                task.id
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "".to_string()),
                Style::default().fg(theme.foreground),
            ),
        ]));

        // Description
        lines.push(Line::from(vec![
            Span::styled("Description   ", Style::default().fg(theme.primary)),
            Span::styled(
                &task.description,
                Style::default()
                    .fg(theme.foreground)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Status
        let status_color = theme.status(task.status);
        lines.push(Line::from(vec![
            Span::styled("Status        ", Style::default().fg(theme.primary)),
            Span::styled(
                task.status.label(),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Project
        if let Some(ref project) = task.project {
            lines.push(Line::from(vec![
                Span::styled("Project       ", Style::default().fg(theme.primary)),
                Span::styled(
                    project,
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // Priority
        if let Some(ref priority) = task.priority {
            let (priority_str, priority_color) = match priority {
                Priority::High => ("High", theme.priority_high),
                Priority::Medium => ("Medium", theme.priority_medium),
                Priority::Low => ("Low", theme.priority_low),
            };
            lines.push(Line::from(vec![
                Span::styled("Priority      ", Style::default().fg(theme.primary)),
                Span::styled(
                    priority_str,
                    Style::default()
                        .fg(priority_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // Due date
        if let Some(due) = task.due {
            let due_color = if task.is_overdue() {
                theme.error
            } else {
                theme.warning
            };
            lines.push(Line::from(vec![
                Span::styled("Due           ", Style::default().fg(theme.primary)),
                Span::styled(
                    due.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default().fg(due_color).add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // Get current time for relative calculations
        let now = Utc::now();

        // Start date (when task is started)
        if let Some(start) = task.start {
            let start_duration = now - start;
            let start_relative = self.format_relative_time(start_duration);
            lines.push(Line::from(vec![
                Span::styled("Start         ", Style::default().fg(theme.primary)),
                Span::styled(
                    format!("{} ({})", start.format("%Y-%m-%d %H:%M:%S"), start_relative),
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // Created (formerly Entered)
        let entry_duration = now - task.entry;
        let entry_relative = self.format_relative_time(entry_duration);
        lines.push(Line::from(vec![
            Span::styled("Created       ", Style::default().fg(theme.primary)),
            Span::styled(
                format!(
                    "{} ({})",
                    task.entry.format("%Y-%m-%d %H:%M:%S"),
                    entry_relative
                ),
                Style::default().fg(theme.muted),
            ),
        ]));

        // Last modified
        if let Some(modified) = task.modified {
            let mod_duration = now - modified;
            let mod_relative = self.format_relative_time(mod_duration);
            lines.push(Line::from(vec![
                Span::styled("Last modified ", Style::default().fg(theme.primary)),
                Span::styled(
                    format!(
                        "{} ({})",
                        modified.format("%Y-%m-%d %H:%M:%S"),
                        mod_relative
                    ),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }

        // Tags
        if !task.tags.is_empty() {
            let tags_str = task.tags.join(" ");
            lines.push(Line::from(vec![
                Span::styled("Tags          ", Style::default().fg(theme.primary)),
                Span::styled(
                    tags_str,
                    Style::default()
                        .fg(theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // UUID
        lines.push(Line::from(vec![
            Span::styled("UUID          ", Style::default().fg(theme.primary)),
            Span::styled(&task.uuid, Style::default().fg(theme.muted)),
        ]));

        // Urgency
        let urgency_color = if task.urgency >= 10.0 {
            theme.error
        } else if task.urgency >= 5.0 {
            theme.warning
        } else {
            theme.success
        };
        lines.push(Line::from(vec![
            Span::styled("Urgency       ", Style::default().fg(theme.primary)),
            Span::styled(
                format!("{:.1}", task.urgency),
                Style::default()
                    .fg(urgency_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        let detail = Paragraph::new(lines)
            .block(Block::default().title("Task Details").borders(Borders::ALL))
            .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(detail, area);
    }

    fn render_modification_history(&self, f: &mut Frame, area: Rect, task: &Task, theme: &Theme) {
        let mut header = Vec::new();
        header.push(Line::from(vec![
            Span::styled(
                "Date",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "                Modification",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Collect modifications with latest first
        let mut modifications = Vec::new();

        // Use modified date if available, otherwise use entry date
        let display_date = if let Some(modified) = task.modified {
            modified.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            task.entry.format("%Y-%m-%d %H:%M:%S").to_string()
        };

        // Show latest modifications first (most recent changes)

        // Due date changes (show with modified date if changed, or entry date if set on creation)
        if let Some(due) = task.due {
            let due_display_date = if let Some(modified) = task.modified {
                modified.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                task.entry.format("%Y-%m-%d %H:%M:%S").to_string()
            };
            modifications.push(Line::from(vec![
                Span::styled(due_display_date, Style::default().fg(theme.muted)),
                Span::styled(" Due set to '", Style::default().fg(theme.muted)),
                Span::styled(
                    due.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("'.", Style::default().fg(theme.muted)),
            ]));
        }

        // Start date (when task is started - IMPORTANT!)
        if let Some(start) = task.start {
            modifications.push(Line::from(vec![
                Span::styled(
                    start.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(" Start set to '", Style::default().fg(theme.muted)),
                Span::styled(
                    start.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("'.", Style::default().fg(theme.muted)),
            ]));
        }

        // Tags (typically added during modifications)
        for tag in &task.tags {
            modifications.push(Line::from(vec![
                Span::styled(display_date.clone(), Style::default().fg(theme.muted)),
                Span::styled(" Tag '", Style::default().fg(theme.muted)),
                Span::styled(
                    tag,
                    Style::default()
                        .fg(theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("' added.", Style::default().fg(theme.muted)),
            ]));
        }

        // Priority
        if let Some(ref priority) = task.priority {
            modifications.push(Line::from(vec![
                Span::styled(display_date.clone(), Style::default().fg(theme.muted)),
                Span::styled(" Priority set to '", Style::default().fg(theme.muted)),
                Span::styled(
                    match priority {
                        Priority::High => "High",
                        Priority::Medium => "Medium",
                        Priority::Low => "Low",
                    },
                    Style::default()
                        .fg(match priority {
                            Priority::High => theme.priority_high,
                            Priority::Medium => theme.priority_medium,
                            Priority::Low => theme.priority_low,
                        })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("'.", Style::default().fg(theme.muted)),
            ]));
        }

        // Status
        modifications.push(Line::from(vec![
            Span::styled(display_date.clone(), Style::default().fg(theme.muted)),
            Span::styled(" Status set to '", Style::default().fg(theme.muted)),
            Span::styled(
                match task.status {
                    TaskStatus::Pending => "pending",
                    TaskStatus::Completed => "completed",
                    TaskStatus::Deleted => "deleted",
                    TaskStatus::Waiting => "waiting",
                    TaskStatus::Recurring => "recurring",
                },
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("'.", Style::default().fg(theme.muted)),
        ]));

        // Project
        if let Some(ref project) = task.project {
            modifications.push(Line::from(vec![
                Span::styled(display_date.clone(), Style::default().fg(theme.muted)),
                Span::styled(" Project set to '", Style::default().fg(theme.muted)),
                Span::styled(
                    project,
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("'.", Style::default().fg(theme.muted)),
            ]));
        }

        // Description and entry (oldest - shown last)
        modifications.push(Line::from(vec![
            Span::styled(
                task.entry.format("%Y-%m-%d %H:%M:%S").to_string(),
                Style::default().fg(theme.muted),
            ),
            Span::styled(" Description set to '", Style::default().fg(theme.muted)),
            Span::styled(
                &task.description,
                Style::default()
                    .fg(theme.foreground)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("'.", Style::default().fg(theme.muted)),
        ]));
        modifications.push(Line::from(vec![
            Span::styled(
                "                    Entry set to '",
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                task.entry.format("%Y-%m-%d %H:%M:%S").to_string(),
                Style::default().fg(theme.foreground),
            ),
            Span::styled("'.", Style::default().fg(theme.muted)),
        ]));

        // Combine header and modifications
        let mut lines = header;
        lines.extend(modifications);

        let history_block = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL))
            .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(history_block, area);
    }

    fn format_relative_time(&self, duration: chrono::Duration) -> String {
        if duration.num_minutes() < 60 {
            format!("{}min", duration.num_minutes().max(1))
        } else if duration.num_hours() < 24 {
            format!("{}h", duration.num_hours())
        } else if duration.num_days() < 30 {
            format!("{}d", duration.num_days())
        } else if duration.num_days() < 365 {
            let weeks = duration.num_days() / 7;
            if weeks < 10 {
                format!("{}w", weeks)
            } else {
                format!("{}mo", duration.num_days() / 30)
            }
        } else {
            format!("{}y", duration.num_days() / 365)
        }
    }
}
