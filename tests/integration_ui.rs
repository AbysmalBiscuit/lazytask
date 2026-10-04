// Integration test: drive the UI through ratatui's TestBackend so we can
// confirm the screens render without panicking and contain the right markers.
// This catches regressions like the UTF-8 byte-slicing panics or missing
// modules without requiring a real terminal.

use lazytask::app::{LaunchEnv, Session};
use lazytask::config::Config;
use lazytask::handlers::input::Action;
use lazytask::handlers::sync::SyncHandler;
use lazytask::taskchampion::TaskChampionIntegration;
use lazytask::ui::app_ui::AppUI;
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use std::time::{Duration, Instant};

fn buffer_contains(terminal: &Terminal<TestBackend>, needle: &str) -> bool {
    let buf = terminal.backend().buffer();
    let mut joined = String::with_capacity((buf.area.width * buf.area.height) as usize);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            joined.push_str(buf[(x, y)].symbol());
        }
        joined.push('\n');
    }
    joined.contains(needle)
}

async fn make_ui() -> (
    AppUI,
    SyncHandler,
    TaskChampionIntegration,
    tempfile::TempDir,
) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = Config::default();
    let ui = AppUI::new(&cfg).expect("AppUI::new");
    let sync_handler = SyncHandler::new();
    let engine = TaskChampionIntegration::new(tmp.path().to_path_buf())
        .await
        .expect("engine");
    (ui, sync_handler, engine, tmp)
}

#[tokio::test]
async fn renders_empty_task_list_without_panic() {
    let (mut ui, sync_handler, mut engine, _tmp) = make_ui().await;
    ui.load_tasks(&mut engine).await.expect("load_tasks");

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| ui.render_with_sync(f, &sync_handler))
        .expect("first draw");

    // Header should mention the app + a few keybinding hints.
    assert!(buffer_contains(&terminal, "LazyTask"), "header missing");
    assert!(
        buffer_contains(&terminal, "Help") || buffer_contains(&terminal, "F1"),
        "F1/Help hint missing"
    );
}

#[tokio::test]
async fn renders_task_list_with_user_data() {
    let (mut ui, sync_handler, mut engine, _tmp) = make_ui().await;
    engine
        .add_task("Buy groceries", &[("project", "home"), ("priority", "H")])
        .await
        .unwrap();
    engine
        .add_task("Send invoice", &[("project", "work")])
        .await
        .unwrap();
    ui.load_tasks(&mut engine).await.unwrap();

    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| ui.render_with_sync(f, &sync_handler))
        .unwrap();

    assert!(
        buffer_contains(&terminal, "Buy groceries"),
        "task description missing from rendered buffer"
    );
    assert!(
        buffer_contains(&terminal, "Send invoice"),
        "second task description missing"
    );
    assert!(
        buffer_contains(&terminal, "home") || buffer_contains(&terminal, "work"),
        "project column missing"
    );
}

#[tokio::test]
async fn renders_task_with_unicode_safely() {
    // Regression test for the byte-slicing panics: a multi-byte description
    // long enough to trigger truncation must not panic the renderer.
    let (mut ui, sync_handler, mut engine, _tmp) = make_ui().await;

    let long_emoji_desc = "🚀".repeat(30) + " send rocket to Mars";
    engine.add_task(&long_emoji_desc, &[]).await.unwrap();
    let long_chinese =
        "中文测试任务描述非常长应该被截断处理而不是崩溃应用程序的渲染过程".to_string();
    engine.add_task(&long_chinese, &[]).await.unwrap();

    ui.load_tasks(&mut engine).await.unwrap();

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    // The assertion is simply that this does not panic.
    terminal
        .draw(|f| ui.render_with_sync(f, &sync_handler))
        .expect("render must not panic on multi-byte strings");
}

#[tokio::test]
async fn small_terminal_does_not_panic() {
    // Regression test for terminal resize / minimum-size handling.
    let (mut ui, sync_handler, mut engine, _tmp) = make_ui().await;
    engine.add_task("Test", &[]).await.unwrap();
    ui.load_tasks(&mut engine).await.unwrap();

    for (w, h) in [(40, 12), (60, 18), (80, 24), (200, 60)] {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| ui.render_with_sync(f, &sync_handler))
            .unwrap_or_else(|e| panic!("draw panicked at {w}x{h}: {e}"));
    }
}

#[tokio::test]
async fn unknown_config_keys_are_named_in_tui_warning() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.toml");
    std::fs::write(
        &config_path,
        "[ui]\nshow_help_bar = false\ncolour = \"red\"\n\n[taskwarrior]\nsync_enabled = true\n",
    )
    .expect("write config");

    let mut session = Session::open(
        Some(config_path.to_str().unwrap()),
        LaunchEnv {
            taskrc_var: Some(tmp.path().join("no-taskrc").into()),
            taskdata_var: Some(tmp.path().join("data").into()),
            home: None,
            ..LaunchEnv::default()
        },
    )
    .await
    .expect("session opens");
    assert!(!session.config.ui.show_help_bar);

    let mut ui = AppUI::new(&session.config).expect("AppUI::new");
    ui.show_config_warnings(&session.warnings);
    let sync_handler = SyncHandler::new();
    ui.load_tasks(&mut session.taskchampion)
        .await
        .expect("load_tasks");

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    terminal
        .draw(|f| ui.render_with_sync(f, &sync_handler))
        .expect("draw");

    assert!(
        buffer_contains(&terminal, "ui.colour"),
        "ui.colour not shown"
    );
    assert!(
        buffer_contains(&terminal, "taskwarrior.sync_enabled"),
        "taskwarrior.sync_enabled not shown"
    );
}

#[test]
fn lazytask_refuses_to_start_when_quit_has_no_key() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.toml");
    std::fs::write(
        &config_path,
        "[keybindings.global]\nquit = \"Ctrl+Nope\"\nhelp = \"q\"\n",
    )
    .expect("write config");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lazytask"))
        .arg("--config")
        .arg(&config_path)
        .env("TASKDATA", tmp.path().join("data"))
        .output()
        .expect("run lazytask");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "lazytask started: {stderr}");
    assert!(
        stderr.contains("keybindings.global.quit")
            && stderr.contains("Ctrl+Nope")
            && stderr.contains("keybindings.global.help"),
        "error should name quit and why it has no key: {stderr}"
    );
}

async fn render_with_config(cfg: Config) -> Terminal<TestBackend> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut engine = TaskChampionIntegration::new(tmp.path().to_path_buf())
        .await
        .expect("engine");
    let mut ui = AppUI::new(&cfg).expect("AppUI::new");
    ui.show_config_warnings(&[]);
    ui.load_tasks(&mut engine).await.expect("load_tasks");
    let mut terminal = Terminal::new(TestBackend::new(160, 40)).expect("terminal");
    terminal
        .draw(|f| ui.render_with_sync(f, &SyncHandler::new()))
        .expect("draw");
    terminal
}

#[tokio::test]
async fn show_help_bar_false_hides_keybinding_hints() {
    let shown = render_with_config(Config::default()).await;
    assert!(
        buffer_contains(&shown, "[a] add"),
        "help bar missing by default"
    );

    let mut cfg = Config::default();
    cfg.ui.show_help_bar = false;
    let hidden = render_with_config(cfg).await;
    assert!(!buffer_contains(&hidden, "[a] add"), "help bar still shown");
}

fn with_default_view(view: &str) -> Config {
    let mut cfg = Config::default();
    cfg.ui.default_view = view.to_string();
    cfg
}

#[tokio::test]
async fn default_view_picks_the_view_lazytask_opens_on() {
    let task_list = render_with_config(with_default_view("task_list")).await;
    assert!(
        buffer_contains(&task_list, " Tasks ("),
        "task list not shown"
    );

    let reports = render_with_config(with_default_view("reports")).await;
    assert!(buffer_contains(&reports, "Burndown"), "reports not shown");
    assert!(!buffer_contains(&reports, " Tasks ("), "task list shown");

    let calendar = render_with_config(with_default_view("calendar")).await;
    assert!(
        buffer_contains(&calendar, "Daily Details"),
        "calendar not shown"
    );
    assert!(!buffer_contains(&calendar, " Tasks ("), "task list shown");
}

#[tokio::test]
async fn unknown_default_view_warns_and_opens_task_list() {
    let terminal = render_with_config(with_default_view("kanban")).await;
    assert!(
        buffer_contains(&terminal, " Tasks ("),
        "task list not shown"
    );
    assert!(buffer_contains(&terminal, "kanban"), "warning missing");
}

#[tokio::test]
async fn config_warnings_and_unknown_keys_show_together() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut ui = AppUI::new(&with_default_view("kanban")).expect("AppUI::new");
    ui.show_config_warnings(&["Unknown config keys: ui.colour".to_string()]);
    let mut engine = TaskChampionIntegration::new(tmp.path().to_path_buf())
        .await
        .expect("engine");
    ui.load_tasks(&mut engine).await.expect("load_tasks");
    let mut terminal = Terminal::new(TestBackend::new(160, 40)).expect("terminal");
    terminal
        .draw(|f| ui.render_with_sync(f, &SyncHandler::new()))
        .expect("draw");

    assert!(buffer_contains(&terminal, "kanban"), "view warning missing");
    assert!(
        buffer_contains(&terminal, "ui.colour"),
        "key warning missing"
    );
}

fn buffer_line(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

/// The task list's column headers, left to right.
fn task_list_headers(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buf = terminal.backend().buffer();
    let title_row = (0..buf.area.height)
        .find(|&y| buffer_line(buf, y).contains(" Tasks ("))
        .expect("task list title missing");
    let header_row = buffer_line(buf, title_row + 1);
    let table_cells = header_row.split('│').nth(1).expect("table border missing");
    table_cells.split_whitespace().map(String::from).collect()
}

#[tokio::test]
async fn task_list_columns_choose_and_order_the_columns() {
    let mut cfg = Config::default();
    cfg.ui.task_list_columns = ["urgency", "description", "tags"]
        .map(String::from)
        .to_vec();
    let terminal = render_with_config(cfg).await;
    assert_eq!(
        task_list_headers(&terminal),
        ["Urgency", "Description", "Tags"]
    );
}

#[tokio::test]
async fn unknown_task_list_column_warns_and_is_skipped() {
    let mut cfg = Config::default();
    cfg.ui.task_list_columns = ["id", "bogus", "description"].map(String::from).to_vec();
    let terminal = render_with_config(cfg).await;
    assert_eq!(task_list_headers(&terminal), ["ID", "Description"]);
    assert!(buffer_contains(&terminal, "bogus"), "warning missing");
}

#[tokio::test]
async fn task_list_columns_match_case_insensitively_and_drop_duplicates() {
    let mut cfg = Config::default();
    cfg.ui.task_list_columns = ["Description", "ID", "description"]
        .map(String::from)
        .to_vec();
    let terminal = render_with_config(cfg).await;
    assert_eq!(task_list_headers(&terminal), ["Description", "ID"]);
}

#[tokio::test]
async fn empty_task_list_columns_warn_and_fall_back_to_defaults() {
    for columns in [vec![], vec!["bogus".to_string()]] {
        let mut cfg = Config::default();
        cfg.ui.task_list_columns = columns;
        let terminal = render_with_config(cfg).await;
        assert_eq!(
            task_list_headers(&terminal),
            ["ID", "Project", "Priority", "Due", "Description"]
        );
        assert!(
            buffer_contains(&terminal, "default columns"),
            "fallback warning missing"
        );
    }
}

struct RefreshFixture {
    ui: AppUI,
    engine: TaskChampionIntegration,
    data_dir: tempfile::TempDir,
    terminal: Terminal<TestBackend>,
}

impl RefreshFixture {
    async fn new(refresh_interval: u64) -> Self {
        let data_dir = tempfile::tempdir().expect("tempdir");
        let mut cfg = Config::default();
        cfg.ui.refresh_interval = refresh_interval;
        let engine = TaskChampionIntegration::new(data_dir.path().to_path_buf())
            .await
            .expect("engine");
        RefreshFixture {
            ui: AppUI::new(&cfg).expect("AppUI::new"),
            engine,
            data_dir,
            terminal: Terminal::new(TestBackend::new(160, 40)).expect("terminal"),
        }
    }

    /// Adds a task through a second handle on the replica, the way `task add`
    /// writes while lazytask is open.
    async fn add_task_externally(&self, description: &str) {
        let mut other = TaskChampionIntegration::new(self.data_dir.path().to_path_buf())
            .await
            .expect("second handle");
        other.add_task(description, &[]).await.expect("add_task");
    }

    async fn refresh_at(&mut self, now: Instant) -> bool {
        self.ui.refresh_if_due(now, &mut self.engine).await
    }

    /// The description shown in the task detail panel.
    fn selected_description(&self) -> String {
        let buf = self.terminal.backend().buffer();
        (0..buf.area.height)
            .map(|y| buffer_line(buf, y))
            .find_map(|line| {
                let (_, rest) = line.split_once("│Description   ")?;
                Some(rest.split('│').next()?.trim().to_string())
            })
            .expect("task detail panel missing")
    }

    fn draw(&mut self) {
        self.terminal
            .draw(|f| self.ui.render_with_sync(f, &SyncHandler::new()))
            .expect("draw");
    }
}

#[tokio::test]
async fn task_added_externally_appears_within_one_refresh_interval() {
    let mut fx = RefreshFixture::new(1000).await;
    fx.ui.load_tasks(&mut fx.engine).await.expect("load_tasks");
    let loaded_at = Instant::now();

    fx.add_task_externally("added with task add").await;

    assert!(
        !fx.refresh_at(loaded_at).await,
        "refreshed before the interval"
    );
    fx.draw();
    assert!(!buffer_contains(&fx.terminal, "added with task add"));

    assert!(fx.refresh_at(loaded_at + Duration::from_millis(1000)).await);
    fx.draw();
    assert!(
        buffer_contains(&fx.terminal, "added with task add"),
        "external task missing after one refresh interval"
    );
}

#[tokio::test]
async fn refresh_keeps_the_selected_task_selected() {
    let mut fx = RefreshFixture::new(1000).await;
    fx.engine.add_task("older task", &[]).await.expect("add");
    fx.engine.add_task("newer task", &[]).await.expect("add");
    fx.ui.load_tasks(&mut fx.engine).await.expect("load_tasks");
    fx.ui
        .handle_action(Action::MoveDown, &mut fx.engine, &mut SyncHandler::new())
        .await
        .expect("move down");
    let loaded_at = Instant::now();
    fx.draw();
    let selected_before = fx.selected_description();

    assert!(fx.refresh_at(loaded_at + Duration::from_millis(1000)).await);
    fx.draw();
    assert_eq!(fx.selected_description(), selected_before);
}

#[tokio::test]
async fn zero_refresh_interval_never_refreshes() {
    let mut fx = RefreshFixture::new(0).await;
    fx.ui.load_tasks(&mut fx.engine).await.expect("load_tasks");
    assert!(
        !fx.refresh_at(Instant::now() + Duration::from_secs(3600))
            .await
    );
}
