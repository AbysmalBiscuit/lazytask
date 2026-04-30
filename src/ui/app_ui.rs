use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::data::models::Task;
use crate::handlers::input::Action;
use crate::handlers::sync::{SyncHandler, SyncPhase};
use crate::taskchampion::TaskChampionIntegration;
use crate::ui::components::sync_config::{SyncConfigResult, SyncConfigWidget};
use crate::ui::components::sync_status::SyncStatusWidget;
use crate::ui::components::task_form::{TaskForm, TaskFormResult};
use crate::ui::views::main_view::MainView;
use crate::ui::views::reports_view::ReportsView;

pub enum AppView {
    TaskList,
    TaskDetail,
    Reports,
    Settings,
    Help,
}

pub struct AppUI {
    current_view: AppView,
    main_view: MainView,
    reports_view: ReportsView,
    sync_status_widget: SyncStatusWidget,
    sync_config_widget: SyncConfigWidget,
    tasks: Vec<Task>,
    filtered_tasks: Vec<Task>,
    task_form: Option<TaskForm>,
    show_sync_overlay: bool,
    status_message: Option<String>,
    status_message_at: Option<std::time::Instant>,
    needs_task_refresh: bool,
    preserve_selection_uuid: Option<String>,
}

impl AppUI {
    pub fn new(_config: &crate::config::Config) -> Result<Self> {
        Ok(AppUI {
            current_view: AppView::TaskList,
            main_view: MainView::new(),
            reports_view: ReportsView::new(),
            sync_status_widget: SyncStatusWidget::new(),
            sync_config_widget: SyncConfigWidget::new(),
            tasks: Vec::new(),
            filtered_tasks: Vec::new(),
            task_form: None,
            show_sync_overlay: false,
            status_message: None,
            status_message_at: None,
            needs_task_refresh: false,
            preserve_selection_uuid: None,
        })
    }

    pub async fn load_tasks(&mut self, taskchampion: &mut TaskChampionIntegration) -> Result<()> {
        let mut tasks = taskchampion.list_tasks().await?;
        tasks.sort_by(|a, b| b.entry.cmp(&a.entry));

        self.tasks = tasks.clone();
        self.main_view.update_available_filters(&self.tasks);
        self.reports_view.update_tasks(tasks);
        self.apply_filters();
        Ok(())
    }

    fn apply_filters(&mut self) {
        self.filtered_tasks = self
            .tasks
            .iter()
            .filter(|task| self.main_view.matches_filters(task))
            .cloned()
            .collect();

        let preserve_uuid = self.preserve_selection_uuid.as_deref();
        self.main_view
            .set_tasks_with_preserved_selection(self.filtered_tasks.clone(), preserve_uuid);
        self.preserve_selection_uuid = None;
    }

    pub fn has_active_form(&self) -> bool {
        self.task_form.is_some()
            || self.main_view.is_filter_focused()
            || self.sync_config_widget.is_active()
    }

    pub fn set_status_message(&mut self, message: String) {
        self.status_message = Some(message);
        self.status_message_at = Some(std::time::Instant::now());
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
        self.status_message_at = None;
    }

    pub fn check_status_message_timeout(&mut self) {
        if let (Some(_), Some(at)) = (&self.status_message, &self.status_message_at) {
            if at.elapsed().as_secs() >= 4 {
                self.clear_status_message();
            }
        }
    }

    fn task_to_attributes(task: &Task) -> Vec<(String, String)> {
        let mut attributes = Vec::new();
        attributes.push(("description".to_string(), task.description.clone()));
        attributes.push((
            "project".to_string(),
            task.project.clone().unwrap_or_default(),
        ));
        attributes.push((
            "priority".to_string(),
            task.priority
                .as_ref()
                .map(|p| match p {
                    crate::data::models::Priority::High => "H",
                    crate::data::models::Priority::Medium => "M",
                    crate::data::models::Priority::Low => "L",
                })
                .unwrap_or("")
                .to_string(),
        ));
        // Clear all tags first, then add new ones
        attributes.push(("tags".to_string(), "".to_string()));
        for tag in &task.tags {
            attributes.push((format!("+{}", tag), "".to_string()));
        }
        if let Some(due) = task.due {
            attributes.push(("due".to_string(), due.format("%Y-%m-%d").to_string()));
        } else {
            attributes.push(("due".to_string(), "".to_string()));
        }
        attributes
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let size = f.area();

        let terminal_height = size.height;
        let (header_size, footer_size) = if terminal_height < 20 {
            (2, 2)
        } else if terminal_height < 30 {
            (3, 2)
        } else {
            (3, 3)
        };

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_size),
                Constraint::Min(10),
                Constraint::Length(footer_size),
            ])
            .split(size);

        self.draw_header(f, main_chunks[0]);

        match self.current_view {
            AppView::TaskList => {
                self.main_view.render(f, main_chunks[1], size.width);
            }
            AppView::TaskDetail => self.draw_task_detail(f, main_chunks[1]),
            AppView::Reports => self.draw_reports(f, main_chunks[1]),
            AppView::Settings => self.draw_settings(f, main_chunks[1]),
            AppView::Help => self.draw_help(f, main_chunks[1]),
        }

        self.draw_footer_panel(f, main_chunks[2]);

        if let Some(ref form) = self.task_form {
            form.render(f, size);
        }

        if self.sync_config_widget.is_active() {
            self.sync_config_widget.render(f, size);
        }
    }

    pub fn render_with_sync(&mut self, f: &mut Frame, sync_handler: &SyncHandler) {
        self.check_status_message_timeout();

        let size = f.area();
        self.draw(f);

        if self.show_sync_overlay {
            if let Some(sync_status) = sync_handler.get_sync_status() {
                self.sync_status_widget
                    .render_sync_overlay(f, size, &sync_status);

                if !sync_status.is_syncing {
                    self.show_sync_overlay = false;
                    if sync_status.progress.phase == SyncPhase::Complete {
                        self.set_status_message(format!("✅ {}", sync_status.progress.message));
                        self.needs_task_refresh = true;
                    } else if sync_status.progress.phase == SyncPhase::Error {
                        let msg = sync_status
                            .sync_error
                            .clone()
                            .unwrap_or_else(|| "Sync failed".to_string());
                        self.set_status_message(format!("❌ {}", msg));
                    }
                }
            }
        }
    }

    pub async fn handle_action(
        &mut self,
        action: Action,
        taskchampion: &mut TaskChampionIntegration,
        sync_handler: &mut SyncHandler,
    ) -> Result<()> {
        if self.needs_task_refresh {
            self.load_tasks(taskchampion).await?;
            self.needs_task_refresh = false;
        }

        if let Some(ref mut form) = self.task_form {
            if let Some(result) = form.handle_input(action.clone())? {
                match result {
                    TaskFormResult::Save(task) => {
                        let attributes = Self::task_to_attributes(&task);
                        let attribute_refs: Vec<(&str, &str)> = attributes
                            .iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect();

                        if !task.uuid.is_empty()
                            && self.tasks.iter().any(|t| t.uuid == task.uuid)
                        {
                            self.preserve_selection_uuid = Some(task.uuid.clone());
                            if let Err(e) = taskchampion
                                .modify_task(&task.uuid, &attribute_refs)
                                .await
                            {
                                self.set_status_message(format!("❌ Edit failed: {}", e));
                            }
                        } else {
                            self.preserve_selection_uuid = None;
                            if let Err(e) = taskchampion
                                .add_task(&task.description, &attribute_refs)
                                .await
                            {
                                self.set_status_message(format!("❌ Add failed: {}", e));
                            }
                        }
                        self.task_form = None;
                        self.load_tasks(taskchampion).await?;
                    }
                    TaskFormResult::Cancel => {
                        self.task_form = None;
                    }
                }
                return Ok(());
            }
        }

        if self.sync_config_widget.is_active() {
            if let Some(result) = self.sync_config_widget.handle_input(action.clone())? {
                match result {
                    SyncConfigResult::Save(config) => {
                        match sync_handler.configure_sync(taskchampion, &config).await {
                            Ok(msg) => {
                                self.set_status_message(format!("✅ {}", msg));
                            }
                            Err(e) => {
                                self.set_status_message(format!("❌ Sync config failed: {}", e));
                            }
                        }
                        self.sync_config_widget.deactivate();
                    }
                    SyncConfigResult::Cancel => {
                        self.sync_config_widget.deactivate();
                    }
                }
            }
            return Ok(());
        }

        match action {
            Action::Quit => {}
            Action::Help => {
                self.current_view = AppView::Help;
            }
            Action::Reports => {
                self.current_view = AppView::Reports;
            }
            Action::Context => {
                if matches!(self.current_view, AppView::Reports) {
                    self.reports_view.toggle_mode();
                }
            }
            Action::Back => {
                if self.task_form.is_some() {
                    self.task_form = None;
                } else if matches!(self.current_view, AppView::TaskList)
                    && self.main_view.is_filter_focused()
                {
                    self.main_view.exit_filter_mode();
                    self.apply_filters();
                } else {
                    self.current_view = AppView::TaskList;
                }
            }
            Action::MoveUp => {
                if matches!(self.current_view, AppView::TaskList)
                    && self.main_view.is_filter_focused()
                {
                    self.main_view.handle_filter_navigation_up();
                } else if matches!(self.current_view, AppView::Reports)
                    && self.reports_view.is_calendar_mode()
                {
                    self.reports_view.navigate_date(
                        crate::ui::views::reports_view::DateNavigation::PrevWeek,
                    );
                } else if self.task_form.is_none()
                    && matches!(self.current_view, AppView::TaskList)
                {
                    self.main_view.previous_task();
                }
            }
            Action::MoveDown => {
                if matches!(self.current_view, AppView::TaskList)
                    && self.main_view.is_filter_focused()
                {
                    self.main_view.handle_filter_navigation_down();
                } else if matches!(self.current_view, AppView::Reports)
                    && self.reports_view.is_calendar_mode()
                {
                    self.reports_view.navigate_date(
                        crate::ui::views::reports_view::DateNavigation::NextWeek,
                    );
                } else if self.task_form.is_none()
                    && matches!(self.current_view, AppView::TaskList)
                {
                    self.main_view.next_task();
                }
            }
            Action::MoveLeft => {
                if matches!(self.current_view, AppView::Reports)
                    && self.reports_view.is_calendar_mode()
                {
                    self.reports_view.navigate_date(
                        crate::ui::views::reports_view::DateNavigation::PrevDay,
                    );
                }
            }
            Action::MoveRight => {
                if matches!(self.current_view, AppView::Reports)
                    && self.reports_view.is_calendar_mode()
                {
                    self.reports_view.navigate_date(
                        crate::ui::views::reports_view::DateNavigation::NextDay,
                    );
                }
            }
            Action::Refresh => {
                self.load_tasks(taskchampion).await?;
            }
            Action::Sync => {
                if !sync_handler.is_sync_configured(taskchampion) {
                    self.set_status_message(
                        "ℹ️ Sync not configured. Press Shift+S to configure.".to_string(),
                    );
                } else {
                    self.show_sync_overlay = true;
                    if let Err(e) = sync_handler.start_sync(taskchampion).await {
                        self.show_sync_overlay = false;
                        self.set_status_message(format!("❌ Sync failed: {}", e));
                    }
                }
            }
            Action::ForceSync => {
                if !sync_handler.is_sync_configured(taskchampion) {
                    self.set_status_message(
                        "ℹ️ Sync not configured. Press Shift+S to configure.".to_string(),
                    );
                } else {
                    self.show_sync_overlay = true;
                    if let Err(e) = sync_handler.force_sync(taskchampion).await {
                        self.show_sync_overlay = false;
                        self.set_status_message(format!("❌ Force sync failed: {}", e));
                    }
                }
            }
            Action::SyncConfig => {
                if sync_handler.is_sync_configured(taskchampion) {
                    self.set_status_message(
                        "ℹ️ Sync already configured. Press 's' to sync.".to_string(),
                    );
                } else {
                    self.sync_config_widget.activate();
                }
            }
            Action::Filter => {
                if matches!(self.current_view, AppView::TaskList) {
                    self.main_view.toggle_filter_focus();
                    if !self.main_view.is_filter_focused() {
                        self.apply_filters();
                    }
                }
            }
            Action::Tab => {
                if matches!(self.current_view, AppView::TaskList)
                    && self.main_view.is_filter_focused()
                {
                    self.main_view.next_filter_section();
                }
            }
            _ => {
                if matches!(self.current_view, AppView::TaskList)
                    && self.main_view.is_filter_focused()
                {
                    match action {
                        Action::MoveUp | Action::MoveDown => {}
                        Action::Space => {
                            self.main_view.toggle_current_selection();
                            self.apply_filters();
                        }
                        Action::Character(c) => {
                            self.main_view.handle_search_character(c);
                            self.apply_filters();
                        }
                        Action::Backspace => {
                            self.main_view.handle_search_backspace();
                            self.apply_filters();
                        }
                        Action::Select => {
                            self.apply_filters();
                        }
                        _ => {}
                    }
                } else if self.task_form.is_none() {
                    if matches!(self.current_view, AppView::Reports)
                        && self.reports_view.is_calendar_mode()
                    {
                        match action {
                            Action::Character('<') => {
                                self.reports_view.navigate_date(
                                    crate::ui::views::reports_view::DateNavigation::PrevMonth,
                                );
                            }
                            Action::Character('>') => {
                                self.reports_view.navigate_date(
                                    crate::ui::views::reports_view::DateNavigation::NextMonth,
                                );
                            }
                            Action::Character('t') => {
                                self.reports_view.navigate_date(
                                    crate::ui::views::reports_view::DateNavigation::Today,
                                );
                            }
                            _ => {}
                        }
                    }

                    if matches!(self.current_view, AppView::TaskList) {
                        self.handle_task_list_action(action, taskchampion).await?;
                    }
                }
            }
        }
        Ok(())
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let header_content = Line::from(vec![
            Span::styled(
                "LazyTask v0.1",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("                    "),
            Span::styled(
                "[F1]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Help", Style::default().fg(Color::White)),
            Span::raw("    "),
            Span::styled(
                "[F5]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Refresh", Style::default().fg(Color::White)),
            Span::raw("    "),
            Span::styled(
                "[/]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Filter", Style::default().fg(Color::White)),
            Span::raw("    "),
            Span::styled(
                "[r]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Reports", Style::default().fg(Color::White)),
            Span::raw("    "),
            Span::styled(
                "[s]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Sync", Style::default().fg(Color::White)),
            Span::raw("    "),
            Span::styled(
                "[S]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Config", Style::default().fg(Color::White)),
        ]);

        let header = Paragraph::new(header_content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .style(Style::default().fg(Color::White))
            .alignment(ratatui::layout::Alignment::Left);

        f.render_widget(header, area);
    }

    fn draw_task_detail(&self, f: &mut Frame, area: Rect) {
        let selected_task = self.main_view.selected_task();
        let detail_text = if let Some(task) = selected_task {
            format!("Task Detail:\n\n{}", task.description)
        } else {
            "No task selected".to_string()
        };
        let detail = Paragraph::new(detail_text)
            .block(Block::default().title("Task Detail").borders(Borders::ALL));
        f.render_widget(detail, area);
    }

    fn draw_reports(&self, f: &mut Frame, area: Rect) {
        self.reports_view.render(f, area);
    }

    fn draw_settings(&self, f: &mut Frame, area: Rect) {
        let settings = Paragraph::new("Settings View - Coming Soon")
            .block(Block::default().title("Settings").borders(Borders::ALL));
        f.render_widget(settings, area);
    }

    fn draw_help(&self, f: &mut Frame, area: Rect) {
        // Render the outer block, then split the inner area into two columns.
        let block = Block::default()
            .title("Help — Keyboard Shortcuts (Esc to close)")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        f.render_widget(block, area);

        // If we're horizontally constrained, fall back to one column.
        let two_columns = area.width >= 90;

        let header = |s: &str| {
            Line::from(Span::styled(
                s.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        let row = |key: &str, desc: &str| {
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("{:<10}", key),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(Color::White)),
            ])
        };
        let blank = || Line::from("");
        let note = |s: &str| Line::from(Span::styled(s.to_string(), Style::default().fg(Color::Gray)));

        let left = vec![
            header("Global"),
            row("q",       "Quit"),
            row("Ctrl+C",  "Quit"),
            row("F1",      "Toggle this help"),
            row("F5",      "Reload tasks from replica"),
            row("Esc",     "Cancel / back / close modal"),
            row("Enter",   "Confirm / save"),
            blank(),
            header("Task list"),
            row("↑ ↓",      "Move selection"),
            row("a",       "Add new task"),
            row("e",       "Edit selected task"),
            row("d",       "Mark task done"),
            row("Delete",  "Soft-delete task"),
            row("/",       "Toggle filter mode"),
            row("r",       "Open Reports view"),
            row("s",       "Sync (needs sync config)"),
            row("Shift+S", "Open Sync Config modal"),
            blank(),
            header("Filter mode"),
            row("Tab",       "Cycle Status→Project→Tags→Search"),
            row("↑ ↓",        "Navigate items"),
            row("Space",     "Toggle item (multi-select)"),
            row("type",      "Search (Search section only)"),
            row("Backspace", "Erase a character"),
            row("Esc",       "Exit (selections stay applied)"),
        ];

        let right = vec![
            header("Reports → Calendar"),
            row("c",      "Toggle Calendar / Dashboard"),
            row("← →",     "Move by one day"),
            row("↑ ↓",      "Move by one week"),
            row("< >",     "Previous / next month"),
            row("t",      "Jump to today"),
            blank(),
            header("Form (add / edit task)"),
            row("Tab / ↓",      "Next field"),
            row("Shift+Tab/↑",  "Previous field"),
            row("← →",          "Move cursor in text field"),
            row("type",         "Edit active field"),
            row("Backspace",    "Erase a character"),
            row("Enter",        "Commit field, then save"),
            row("Esc",          "Cancel without saving"),
            blank(),
            header("Sync setup"),
            note(" 1. Run a taskchampion-sync-server"),
            note("    (see README §Sync)"),
            note(" 2. Press Shift+S, fill URL,"),
            note("    client_id (UUID), and secret"),
            note(" 3. Press Enter to save"),
            note(" 4. Press s to sync"),
            blank(),
            header("Tag syntax (Tags field)"),
            note(" +work     add tag"),
            note(" -old      remove tag"),
            note(" (empty)   clear all user tags"),
        ];

        if two_columns {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .horizontal_margin(1)
                .split(inner);
            f.render_widget(Paragraph::new(left), columns[0]);
            f.render_widget(Paragraph::new(right), columns[1]);
        } else {
            // Narrow terminals: render left column followed by right column.
            let mut combined = left;
            combined.push(blank());
            combined.extend(right);
            f.render_widget(Paragraph::new(combined), inner);
        }
    }

    fn draw_footer_panel(&self, f: &mut Frame, area: Rect) {
        if let Some(ref message) = self.status_message {
            let color = if message.starts_with('❌') {
                Color::Red
            } else if message.starts_with('✅') {
                Color::Green
            } else {
                Color::Yellow
            };
            let panel = Paragraph::new(message.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(color)),
                )
                .style(Style::default().fg(color))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(panel, area);
            return;
        }

        let help_content = if self.task_form.is_some() {
            Line::from(vec![
                Span::styled(
                    "↑↓",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Navigate fields  "),
                Span::styled(
                    "←→",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Move cursor  "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Save  "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Cancel"),
            ])
        } else if self.main_view.is_filter_focused() {
            Line::from(vec![
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Next section  "),
                Span::styled(
                    "↑↓",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Navigate  "),
                Span::styled(
                    "Space",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Toggle  "),
                Span::styled(
                    "Type",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Search  "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Exit"),
            ])
        } else {
            match self.current_view {
                AppView::TaskList => Line::from(vec![
                    Span::styled(
                        "[a]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("dd  "),
                    Span::styled(
                        "[e]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("dit  "),
                    Span::styled(
                        "[d]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("one  "),
                    Span::styled(
                        "[Del]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("ete  "),
                    Span::styled(
                        "[/]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("filter  "),
                    Span::styled(
                        "[r]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("eports  "),
                    Span::styled(
                        "[s]",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("ync  "),
                    Span::styled(
                        "[q]",
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("uit"),
                ]),
                AppView::Reports => {
                    if self.reports_view.is_calendar_mode() {
                        Line::from(vec![
                            Span::styled(
                                "[←→]",
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" day  "),
                            Span::styled(
                                "[↑↓]",
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" week  "),
                            Span::styled(
                                "[< >]",
                                Style::default()
                                    .fg(Color::Magenta)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" month  "),
                            Span::styled(
                                "[t]",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("oday  "),
                            Span::styled(
                                "[c]",
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" dashboard  "),
                            Span::styled(
                                "[ESC]",
                                Style::default()
                                    .fg(Color::Red)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" back"),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled(
                                "[c]",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("alendar  "),
                            Span::styled(
                                "[ESC]",
                                Style::default()
                                    .fg(Color::Red)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" back  "),
                            Span::styled(
                                "[q]",
                                Style::default()
                                    .fg(Color::Red)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("uit"),
                        ])
                    }
                }
                AppView::Help => Line::from(vec![
                    Span::styled(
                        "[ESC]",
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" back"),
                ]),
                _ => Line::from(vec![
                    Span::styled(
                        "[ESC]",
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" back  "),
                    Span::styled(
                        "[q]",
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("uit"),
                ]),
            }
        };

        let footer_panel = Paragraph::new(help_content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray)),
            )
            .style(Style::default().fg(Color::White))
            .alignment(ratatui::layout::Alignment::Center);

        f.render_widget(footer_panel, area);
    }

    async fn handle_task_list_action(
        &mut self,
        action: Action,
        taskchampion: &mut TaskChampionIntegration,
    ) -> Result<()> {
        match action {
            Action::AddTask => {
                self.task_form = Some(TaskForm::new_task());
            }
            Action::EditTask => {
                if let Some(task) = self.main_view.selected_task() {
                    self.task_form = Some(TaskForm::edit_task(task.clone()));
                }
            }
            Action::DoneTask => {
                if let Some(task) = self.main_view.selected_task().cloned() {
                    let next_uuid = self.next_selection_uuid_after_current();
                    self.preserve_selection_uuid = next_uuid;
                    match taskchampion.done_task(&task.uuid).await {
                        Ok(_) => {
                            self.load_tasks(taskchampion).await?;
                        }
                        Err(e) => {
                            self.set_status_message(format!("❌ Done failed: {}", e));
                            self.preserve_selection_uuid = None;
                        }
                    }
                }
            }
            Action::DeleteTask => {
                if let Some(task) = self.main_view.selected_task().cloned() {
                    let next_uuid = self.next_selection_uuid_after_current();
                    self.preserve_selection_uuid = next_uuid;
                    match taskchampion.delete_task(&task.uuid).await {
                        Ok(_) => {
                            self.load_tasks(taskchampion).await?;
                        }
                        Err(e) => {
                            self.set_status_message(format!("❌ Delete failed: {}", e));
                            self.preserve_selection_uuid = None;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn next_selection_uuid_after_current(&self) -> Option<String> {
        let current_index = self.main_view.selected_index().unwrap_or(0);
        if current_index + 1 < self.filtered_tasks.len() {
            Some(self.filtered_tasks[current_index + 1].uuid.clone())
        } else if current_index > 0 {
            Some(self.filtered_tasks[current_index - 1].uuid.clone())
        } else {
            None
        }
    }
}
