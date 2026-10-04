use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::time::{Duration, Instant};

use crate::data::models::Task;
use crate::handlers::input::Action;
use crate::handlers::sync::{SyncHandler, SyncPhase};
use crate::taskchampion::TaskChampionIntegration;
use crate::ui::components::sync_config::{SyncConfigResult, SyncConfigWidget};
use crate::ui::components::sync_status::SyncStatusWidget;
use crate::ui::components::task_form::{TaskForm, TaskFormResult};
use crate::ui::components::task_list::Column;
use crate::ui::views::main_view::MainView;
use crate::ui::views::reports_view::{DateNavigation, ReportsView};
use crossterm::event::KeyEvent;

use crate::utils::keybindings::{
    Bindable, Binding, FormAction, GlobalAction, InputContext, Keymap, ReportsAction, Section,
    TaskListAction,
};

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
    keymap: Keymap,
    keymap_warnings: Vec<String>,
    show_help_bar: bool,
    config_warnings: Vec<String>,
    refresh_interval: Option<Duration>,
    last_refresh: Instant,
}

impl AppUI {
    pub fn new(config: &crate::config::Config) -> Result<Self> {
        let (keymap, keymap_warnings) = Keymap::from_config(&config.keybindings)?;
        let mut columns = Vec::new();
        let mut unknown_columns = Vec::new();
        for name in &config.ui.task_list_columns {
            match Column::from_name(name) {
                Some(column) => columns.push(column),
                None => unknown_columns.push(name.as_str()),
            }
        }

        let mut ui = AppUI {
            current_view: AppView::TaskList,
            main_view: MainView::new(columns),
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
            keymap,
            keymap_warnings,
            show_help_bar: config.ui.show_help_bar,
            config_warnings: Vec::new(),
            refresh_interval: match config.ui.refresh_interval {
                0 => None,
                ms => Some(Duration::from_millis(ms)),
            },
            last_refresh: Instant::now(),
        };

        match config.ui.default_view.as_str() {
            "task_list" => {}
            "reports" => ui.current_view = AppView::Reports,
            "calendar" => {
                ui.current_view = AppView::Reports;
                ui.reports_view.open_calendar();
            }
            unknown => ui.config_warnings.push(format!(
                "Unknown default_view \"{unknown}\", opening task_list"
            )),
        }

        if !unknown_columns.is_empty() {
            ui.config_warnings.push(format!(
                "Unknown task_list_columns skipped: {}",
                unknown_columns.join(", ")
            ));
        }

        Ok(ui)
    }

    pub async fn load_tasks(&mut self, taskchampion: &mut TaskChampionIntegration) -> Result<()> {
        let mut tasks = taskchampion.list_tasks().await?;
        // The replica returns tasks in no fixed order, so tie-break on uuid to
        // keep rows from swapping places on every refresh.
        tasks.sort_by(|a, b| b.entry.cmp(&a.entry).then_with(|| a.uuid.cmp(&b.uuid)));

        self.last_refresh = Instant::now();
        self.tasks = tasks.clone();
        self.main_view.update_available_filters(&self.tasks);
        self.reports_view.update_tasks(tasks);
        self.apply_filters();
        Ok(())
    }

    /// Reloads tasks from the replica once `ui.refresh_interval` has passed
    /// since the last load, keeping the selected task selected. A failed
    /// reload shows in the footer and is retried an interval later. Returns
    /// whether the screen needs a redraw.
    pub async fn refresh_if_due(
        &mut self,
        now: Instant,
        taskchampion: &mut TaskChampionIntegration,
    ) -> bool {
        if self.time_until_refresh(now) != Some(Duration::ZERO) {
            return false;
        }
        self.preserve_selection_uuid = self.main_view.selected_task_uuid();
        if let Err(e) = self.load_tasks(taskchampion).await {
            self.last_refresh = now;
            self.preserve_selection_uuid = None;
            self.set_status_message(format!("❌ Refresh failed: {e}"));
        }
        true
    }

    /// Time left until `refresh_if_due` reloads, or `None` when
    /// auto-refresh is off.
    pub fn time_until_refresh(&self, now: Instant) -> Option<Duration> {
        let interval = self.refresh_interval?;
        Some(interval.saturating_sub(now.saturating_duration_since(self.last_refresh)))
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

    /// The action a key press triggers, from the bindings of the view or
    /// form that has focus.
    pub fn action(&self, key: KeyEvent) -> Action {
        self.keymap.action(self.input_context(), key)
    }

    fn input_context(&self) -> InputContext {
        if self.has_active_form() {
            return InputContext::Form;
        }
        match self.current_view {
            AppView::TaskList => InputContext::TaskList,
            AppView::Reports => InputContext::Reports,
            AppView::TaskDetail | AppView::Settings | AppView::Help => InputContext::Other,
        }
    }

    /// The key bound to `binding`, for hints such as "Press S".
    fn key_label(&self, binding: impl Into<Binding>) -> String {
        self.keymap
            .key(binding)
            .map_or_else(|| "(unbound)".to_string(), |key| key.to_string())
    }

    pub fn set_status_message(&mut self, message: String) {
        self.status_message = Some(message);
        self.status_message_at = Some(std::time::Instant::now());
    }

    /// Shows one warning naming every startup problem that did not stop
    /// lazytask: `startup_warnings` from loading the config and taskrc, then
    /// unusable keybindings and `[ui]` values.
    pub fn show_config_warnings(&mut self, startup_warnings: &[String]) {
        let warnings: Vec<&str> = startup_warnings
            .iter()
            .chain(&self.keymap_warnings)
            .chain(&self.config_warnings)
            .map(String::as_str)
            .collect();
        if !warnings.is_empty() {
            self.set_status_message(format!("⚠ {}", warnings.join("; ")));
        }
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

        // The footer also carries status messages, so it stays up for them
        // even when the help bar is off.
        let show_footer = self.show_help_bar || self.status_message.is_some();
        let footer_size = if show_footer { footer_size } else { 0 };

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

        if show_footer {
            self.draw_footer_panel(f, main_chunks[2]);
        }

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

                        if !task.uuid.is_empty() && self.tasks.iter().any(|t| t.uuid == task.uuid) {
                            self.preserve_selection_uuid = Some(task.uuid.clone());
                            if let Err(e) =
                                taskchampion.modify_task(&task.uuid, &attribute_refs).await
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
            Action::ToggleCalendar => {
                if matches!(self.current_view, AppView::Reports) {
                    self.reports_view.toggle_mode();
                }
            }
            Action::PrevMonth | Action::NextMonth | Action::Today => {
                if matches!(self.current_view, AppView::Reports)
                    && self.reports_view.is_calendar_mode()
                {
                    self.reports_view.navigate_date(match action {
                        Action::PrevMonth => DateNavigation::PrevMonth,
                        Action::NextMonth => DateNavigation::NextMonth,
                        _ => DateNavigation::Today,
                    });
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
                    self.reports_view
                        .navigate_date(crate::ui::views::reports_view::DateNavigation::PrevWeek);
                } else if self.task_form.is_none() && matches!(self.current_view, AppView::TaskList)
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
                    self.reports_view
                        .navigate_date(crate::ui::views::reports_view::DateNavigation::NextWeek);
                } else if self.task_form.is_none() && matches!(self.current_view, AppView::TaskList)
                {
                    self.main_view.next_task();
                }
            }
            Action::MoveLeft => {
                if matches!(self.current_view, AppView::Reports)
                    && self.reports_view.is_calendar_mode()
                {
                    self.reports_view
                        .navigate_date(crate::ui::views::reports_view::DateNavigation::PrevDay);
                }
            }
            Action::MoveRight => {
                if matches!(self.current_view, AppView::Reports)
                    && self.reports_view.is_calendar_mode()
                {
                    self.reports_view
                        .navigate_date(crate::ui::views::reports_view::DateNavigation::NextDay);
                }
            }
            Action::Refresh => {
                self.load_tasks(taskchampion).await?;
            }
            Action::Sync => {
                if !sync_handler.is_sync_configured(taskchampion) {
                    self.set_status_message(format!(
                        "ℹ️ Sync not configured. Press {} to configure.",
                        self.key_label(GlobalAction::SyncConfig)
                    ));
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
                    self.set_status_message(format!(
                        "ℹ️ Sync not configured. Press {} to configure.",
                        self.key_label(GlobalAction::SyncConfig)
                    ));
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
                    self.set_status_message(format!(
                        "ℹ️ Sync already configured. Press {} to sync.",
                        self.key_label(GlobalAction::Sync)
                    ));
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
            Action::NextField => {
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
                        Action::Toggle => {
                            self.main_view.toggle_current_selection();
                            self.apply_filters();
                        }
                        Action::Character(c) => {
                            self.main_view.handle_search_character(c);
                            self.apply_filters();
                        }
                        Action::Erase => {
                            self.main_view.handle_search_backspace();
                            self.apply_filters();
                        }
                        Action::Select => {
                            self.apply_filters();
                        }
                        _ => {}
                    }
                } else if self.task_form.is_none() && matches!(self.current_view, AppView::TaskList)
                {
                    self.handle_task_list_action(action, taskchampion).await?;
                }
            }
        }
        Ok(())
    }

    /// `[key] label` hints for the bound actions, skipping actions with no
    /// key. Each hint lists one or more actions whose keys share a label.
    fn hints(&self, hints: &[(&[Binding], &str, Color)]) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for &(bindings, label, color) in hints {
            let keys: Vec<String> = bindings
                .iter()
                .filter_map(|&binding| self.keymap.key(binding))
                .map(|key| key.to_string())
                .collect();
            if keys.is_empty() {
                continue;
            }
            if !spans.is_empty() {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                format!("[{}]", keys.join("/")),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(format!(" {label}")));
        }
        spans
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        use GlobalAction as G;
        let mut spans = vec![
            Span::styled(
                "LazyTask v0.1",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("                    "),
        ];
        spans.extend(self.hints(&[
            (&[G::Help.into()], "Help", Color::Yellow),
            (&[G::Refresh.into()], "Refresh", Color::Yellow),
            (&[TaskListAction::Filter.into()], "Filter", Color::Yellow),
            (&[G::Reports.into()], "Reports", Color::Yellow),
            (&[G::Sync.into()], "Sync", Color::Yellow),
            (&[G::SyncConfig.into()], "Config", Color::Yellow),
        ]));
        let header_content = Line::from(spans);

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
            .title(format!(
                "Help — Keyboard Shortcuts ({} to close)",
                self.key_label(GlobalAction::Back)
            ))
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
                    format!("{:<11}", key),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(Color::White)),
            ])
        };
        let blank = || Line::from("");
        let note = |s: &str| {
            Line::from(Span::styled(
                s.to_string(),
                Style::default().fg(Color::Gray),
            ))
        };

        let rows = |section: Section| {
            self.keymap
                .bindings(section)
                .map(|(binding, key)| row(&key.to_string(), binding.description()))
                .collect::<Vec<_>>()
        };

        let mut left = vec![header("Global")];
        left.extend(rows(Section::Global));
        left.extend([blank(), header("Task list")]);
        left.extend(rows(Section::TaskList));
        left.extend([blank(), header("Form and filter panel")]);
        left.extend(rows(Section::Form));
        left.push(row("type", "Edit active field / search"));

        let sync_config = self.key_label(GlobalAction::SyncConfig);
        let confirm = self.key_label(FormAction::Confirm);
        let sync = self.key_label(GlobalAction::Sync);
        let mut right = vec![header("Reports")];
        right.extend(rows(Section::Reports));
        right.extend([
            blank(),
            header("Sync setup"),
            note(" 1. Run a taskchampion-sync-server"),
            note("    (see README §Sync)"),
            note(&format!(" 2. Press {sync_config}, fill URL,")),
            note("    client_id (UUID), and secret"),
            note(&format!(" 3. Press {confirm} to save")),
            note(&format!(" 4. Press {sync} to sync")),
            blank(),
            header("Tag syntax (Tags field)"),
            note(" +work     add tag"),
            note(" -old      remove tag"),
            note(" (empty)   clear all user tags"),
        ]);

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

        use FormAction as F;
        use GlobalAction as G;
        use ReportsAction as R;
        use TaskListAction as T;
        let hints = if self.task_form.is_some() {
            self.hints(&[
                (
                    &[F::MoveUp.into(), F::MoveDown.into()],
                    "Navigate fields",
                    Color::Cyan,
                ),
                (
                    &[F::MoveLeft.into(), F::MoveRight.into()],
                    "Move cursor",
                    Color::Magenta,
                ),
                (&[F::Confirm.into()], "Save", Color::Green),
                (&[F::Cancel.into()], "Cancel", Color::Red),
            ])
        } else if self.main_view.is_filter_focused() {
            let mut spans = self.hints(&[
                (&[F::NextField.into()], "Next section", Color::Magenta),
                (
                    &[F::MoveUp.into(), F::MoveDown.into()],
                    "Navigate",
                    Color::Cyan,
                ),
                (&[F::Toggle.into()], "Toggle", Color::Green),
            ]);
            spans.extend([
                Span::raw("  "),
                Span::styled(
                    "Type",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Search  "),
            ]);
            spans.extend(self.hints(&[(&[F::Cancel.into()], "Exit", Color::Red)]));
            spans
        } else {
            match self.current_view {
                AppView::TaskList => self.hints(&[
                    (&[T::AddTask.into()], "add", Color::Yellow),
                    (&[T::EditTask.into()], "edit", Color::Yellow),
                    (&[T::DoneTask.into()], "done", Color::Yellow),
                    (&[T::DeleteTask.into()], "delete", Color::Yellow),
                    (&[T::Filter.into()], "filter", Color::Yellow),
                    (&[G::Reports.into()], "reports", Color::Yellow),
                    (&[G::Sync.into()], "sync", Color::Yellow),
                    (&[G::Quit.into()], "quit", Color::Red),
                ]),
                AppView::Reports if self.reports_view.is_calendar_mode() => self.hints(&[
                    (&[R::PrevDay.into(), R::NextDay.into()], "day", Color::Cyan),
                    (
                        &[R::PrevWeek.into(), R::NextWeek.into()],
                        "week",
                        Color::Cyan,
                    ),
                    (
                        &[R::PrevMonth.into(), R::NextMonth.into()],
                        "month",
                        Color::Magenta,
                    ),
                    (&[R::Today.into()], "today", Color::Yellow),
                    (&[R::ToggleCalendar.into()], "dashboard", Color::Green),
                    (&[G::Back.into()], "back", Color::Red),
                ]),
                AppView::Reports => self.hints(&[
                    (&[R::ToggleCalendar.into()], "calendar", Color::Yellow),
                    (&[G::Back.into()], "back", Color::Red),
                    (&[G::Quit.into()], "quit", Color::Red),
                ]),
                AppView::Help => self.hints(&[(&[G::Back.into()], "back", Color::Red)]),
                _ => self.hints(&[
                    (&[G::Back.into()], "back", Color::Red),
                    (&[G::Quit.into()], "quit", Color::Red),
                ]),
            }
        };
        let help_content = Line::from(hints);

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
