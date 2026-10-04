// Task display widget with clean, template-like table configuration and intelligent color coding

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};

use crate::data::models::Task;

/// A task list column, named in config by its `ui.task_list_columns` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Id,
    Uuid,
    Project,
    Priority,
    Due,
    Description,
    Tags,
    Urgency,
    Entry,
    Modified,
    Status,
}

impl Column {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "id" => Column::Id,
            "uuid" => Column::Uuid,
            "project" => Column::Project,
            "priority" => Column::Priority,
            "due" => Column::Due,
            "description" => Column::Description,
            "tags" => Column::Tags,
            "urgency" => Column::Urgency,
            "entry" => Column::Entry,
            "modified" => Column::Modified,
            "status" => Column::Status,
            _ => return None,
        })
    }

    fn header(self) -> &'static str {
        match self {
            Column::Id => "ID",
            Column::Uuid => "UUID",
            Column::Project => "Project",
            Column::Priority => "Priority",
            Column::Due => "Due",
            Column::Description => "Description",
            Column::Tags => "Tags",
            Column::Urgency => "Urgency",
            Column::Entry => "Entry",
            Column::Modified => "Modified",
            Column::Status => "Status",
        }
    }

    /// Cell width in characters, or `None` for the description, which
    /// takes the space the other columns leave.
    fn fixed_width(self) -> Option<u16> {
        match self {
            Column::Id => Some(4),
            Column::Uuid => Some(8),
            Column::Project => Some(14),
            Column::Priority => Some(8),
            Column::Due => Some(6),
            Column::Description => None,
            Column::Tags => Some(16),
            Column::Urgency => Some(7),
            Column::Entry | Column::Modified => Some(10),
            Column::Status => Some(9),
        }
    }

    fn width(self) -> Constraint {
        self.fixed_width()
            .map_or(Constraint::Min(20), Constraint::Length)
    }
}

pub struct TaskListWidget {
    pub state: TableState,
    tasks: Vec<Task>,
    columns: Vec<Column>,
}

impl TaskListWidget {
    pub fn new(columns: Vec<Column>) -> Self {
        TaskListWidget {
            state: TableState::default(),
            tasks: Vec::new(),
            columns,
        }
    }

    pub fn set_tasks(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks;
        if !self.tasks.is_empty() {
            self.state.select(Some(0));
        }
    }

    pub fn set_tasks_with_preserved_selection(
        &mut self,
        tasks: Vec<Task>,
        preserve_uuid: Option<&str>,
    ) {
        self.tasks = tasks;

        if self.tasks.is_empty() {
            self.state.select(None);
            return;
        }

        // If we have a UUID to preserve, try to find and select that task
        if let Some(uuid) = preserve_uuid {
            for (index, task) in self.tasks.iter().enumerate() {
                if task.uuid == uuid {
                    self.state.select(Some(index));
                    return;
                }
            }
        }

        // Fallback to first task if UUID not found or not provided
        self.state.select(Some(0));
    }

    pub fn selected_task_uuid(&self) -> Option<String> {
        self.selected_task().map(|task| task.uuid.clone())
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.tasks.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.tasks.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn selected_task(&self) -> Option<&Task> {
        if let Some(index) = self.state.selected() {
            self.tasks.get(index)
        } else {
            None
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let formatter = TaskTableFormatter::new();

        let header_cells = self
            .columns
            .iter()
            .map(|column| {
                Cell::from(column.header()).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect::<Vec<_>>();

        let header = Row::new(header_cells)
            .style(Style::default().bg(Color::DarkGray))
            .height(1);

        // Create data rows with intelligent color coding
        let rows: Vec<Row> = self
            .tasks
            .iter()
            .map(|task| formatter.format_task_row(task, &self.columns))
            .collect();

        let column_widths: Vec<Constraint> = self.columns.iter().map(|c| c.width()).collect();
        let task_count = self.tasks.len();
        let title = format!(" Tasks ({}) ", task_count);

        let table = Table::new(rows, &column_widths)
            .header(header)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .column_spacing(2) // Clean spacing between columns
            .style(Style::default().fg(Color::White))
            .row_highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            );

        f.render_stateful_widget(table, area, &mut self.state);
    }
}

// Clean, template-like table configuration with intelligent color coding
struct TaskTableFormatter;

impl TaskTableFormatter {
    fn new() -> Self {
        TaskTableFormatter
    }

    // Format a complete task row with intelligent row-level color coding
    fn format_task_row(&self, task: &Task, columns: &[Column]) -> Row<'static> {
        // Determine the most important styling factor for the entire row
        let row_style = self.get_row_style(task);

        let cells = columns
            .iter()
            .map(|&column| Cell::from(self.format_cell(task, column)));
        Row::new(cells).height(1).style(row_style)
    }

    fn format_cell(&self, task: &Task, column: Column) -> String {
        let text = match column {
            Column::Id => self.format_id(task.id),
            Column::Uuid => task.uuid.split('-').next().unwrap_or_default().to_string(),
            Column::Project => task.project.clone().unwrap_or_default(),
            Column::Priority => self.format_priority_full(&task.priority),
            Column::Due => self.format_due(task.due),
            Column::Description => task.description.clone(),
            Column::Tags => task.tags.join(" "),
            Column::Urgency => format!("{:.1}", task.urgency),
            Column::Entry => task.entry.format("%Y-%m-%d").to_string(),
            Column::Modified => task
                .modified
                .map(|m| m.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            Column::Status => task.status.label().to_string(),
        };
        match column.fixed_width() {
            Some(width) => crate::utils::formatting::truncate_chars(&text, width.into()),
            None => text,
        }
    }

    // ===== INTELLIGENT ROW-LEVEL COLOR CODING SYSTEM =====

    // Get overall row style based on intelligent task priority hierarchy
    fn get_row_style(&self, task: &Task) -> Style {
        // Intelligent priority hierarchy combining multiple factors:
        // 1. High priority + overdue/due soon = CRITICAL RED BOLD
        // 2. Any overdue tasks = URGENT RED BOLD
        // 3. High priority + due within 2 days = URGENT RED BOLD
        // 4. Due today/tomorrow = URGENT YELLOW BOLD
        // 5. High priority tasks = RED
        // 6. Medium priority tasks = YELLOW
        // 7. Completed tasks = DIMMED GRAY
        // 8. Low priority tasks = GREEN
        // 9. Default/no priority tasks = WHITE

        let is_high_priority = task.priority == Some(crate::data::models::Priority::High);
        let is_overdue = self.is_overdue(task.due);
        let is_due_today = self.is_due_today(task.due);
        let is_due_within_2_days = self.is_due_within_days(task.due, 2);
        let is_due_tomorrow = self.is_due_tomorrow(task.due);

        if is_overdue || is_due_today || (is_high_priority && is_due_within_2_days) {
            // CRITICAL RED:
            // - All overdue tasks (regardless of priority)
            // - All tasks due today (regardless of priority)
            // - High priority tasks due within 2 days
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if is_due_tomorrow {
            // URGENT YELLOW: Due tomorrow = high urgency
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if is_high_priority {
            // HIGH PRIORITY - Important but not time-critical
            Style::default().fg(Color::Red)
        } else if task.priority == Some(crate::data::models::Priority::Medium) {
            // MEDIUM PRIORITY - Moderate importance
            Style::default().fg(Color::Yellow)
        } else if task.status == crate::data::models::TaskStatus::Completed {
            // COMPLETED - Dimmed
            Style::default().fg(Color::DarkGray)
        } else if task.priority == Some(crate::data::models::Priority::Low) {
            // LOW PRIORITY - Less urgent
            Style::default().fg(Color::Green)
        } else if task.urgency >= 10.0 {
            // HIGH URGENCY (calculated, without explicit priority)
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            // DEFAULT - Normal tasks
            Style::default().fg(Color::White)
        }
    }

    // Helper method to check if task is due within N days
    fn is_due_within_days(&self, due: Option<chrono::DateTime<Utc>>, days: i64) -> bool {
        if let Some(due_date) = due {
            let now = Utc::now();
            let days_until_due = (due_date.date_naive() - now.date_naive()).num_days();
            days_until_due >= 0 && days_until_due <= days
        } else {
            false
        }
    }

    // Helper method to check if task is due today specifically
    fn is_due_today(&self, due: Option<chrono::DateTime<Utc>>) -> bool {
        if let Some(due_date) = due {
            let now = Utc::now();
            let days_until_due = (due_date.date_naive() - now.date_naive()).num_days();
            days_until_due == 0 // Exactly today
        } else {
            false
        }
    }

    // Helper method to check if task is due tomorrow specifically
    fn is_due_tomorrow(&self, due: Option<chrono::DateTime<Utc>>) -> bool {
        if let Some(due_date) = due {
            let now = Utc::now();
            let days_until_due = (due_date.date_naive() - now.date_naive()).num_days();
            days_until_due == 1 // Exactly tomorrow
        } else {
            false
        }
    }

    // Helper method to check if task is overdue
    fn is_overdue(&self, due: Option<chrono::DateTime<Utc>>) -> bool {
        if let Some(due_date) = due {
            let now = Utc::now();
            let days_until_due = (due_date.date_naive() - now.date_naive()).num_days();
            days_until_due < 0 // Past due date
        } else {
            false
        }
    }

    // ===== FIELD FORMATTERS =====

    fn format_id(&self, id: Option<u32>) -> String {
        id.map(|i| i.to_string()).unwrap_or_default()
    }

    fn format_priority_full(&self, priority: &Option<crate::data::models::Priority>) -> String {
        match priority {
            Some(crate::data::models::Priority::High) => "High".to_string(),
            Some(crate::data::models::Priority::Medium) => "Medium".to_string(),
            Some(crate::data::models::Priority::Low) => "Low".to_string(),
            None => "".to_string(),
        }
    }

    fn format_due(&self, due: Option<chrono::DateTime<Utc>>) -> String {
        if let Some(due) = due {
            let now = Utc::now();
            let days_until_due = (due.date_naive() - now.date_naive()).num_days();

            if days_until_due < 0 {
                format!("{}d", days_until_due)
            } else if days_until_due <= 7 {
                format!("{}d", days_until_due)
            } else {
                due.format("%m/%d").to_string()
            }
        } else {
            "".to_string()
        }
    }
}
