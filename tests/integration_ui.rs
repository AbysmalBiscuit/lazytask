// Integration test: drive the UI through ratatui's TestBackend so we can
// confirm the screens render without panicking and contain the right markers.
// This catches regressions like the UTF-8 byte-slicing panics or missing
// modules without requiring a real terminal.

use lazytask::app::{LaunchEnv, Session};
use lazytask::config::Config;
use lazytask::handlers::sync::SyncHandler;
use lazytask::taskchampion::TaskChampionIntegration;
use lazytask::ui::app_ui::AppUI;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};

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
    let sync_handler = SyncHandler::new(None);
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

    let session = open_session(&tmp, &config_path).await;
    assert!(!session.config.ui.show_help_bar);
    let terminal = render_session(session).await;

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

/// Opens a session from `config_path` the way startup does, with no taskrc.
async fn open_session(tmp: &tempfile::TempDir, config_path: &std::path::Path) -> Session {
    Session::open(
        Some(config_path.to_str().unwrap()),
        LaunchEnv {
            taskrc_var: Some(tmp.path().join("no-taskrc").into()),
            taskdata_var: Some(tmp.path().join("data").into()),
            home: None,
            ..LaunchEnv::default()
        },
    )
    .await
    .expect("session opens")
}

/// Builds the UI for `session` the way startup does, startup warnings
/// included, and draws one frame.
async fn render_session(mut session: Session) -> Terminal<TestBackend> {
    let mut ui = AppUI::new(&session.config).expect("AppUI::new");
    ui.show_config_warnings(&session.warnings);
    ui.load_tasks(&mut session.taskchampion)
        .await
        .expect("load_tasks");
    let mut terminal = Terminal::new(TestBackend::new(160, 40)).expect("terminal");
    terminal
        .draw(|f| ui.render_with_sync(f, &SyncHandler::new(None)))
        .expect("draw");
    terminal
}

async fn render_with_config(config: Config) -> Terminal<TestBackend> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let taskchampion = TaskChampionIntegration::new(tmp.path().to_path_buf())
        .await
        .expect("engine");
    render_session(Session {
        config,
        warnings: Vec::new(),
        taskchampion,
        taskrc: None,
    })
    .await
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
    let config_path = tmp.path().join("config.toml");
    std::fs::write(
        &config_path,
        "[ui]\ndefault_view = \"kanban\"\ncolour = \"red\"\n",
    )
    .expect("write config");
    let terminal = render_session(open_session(&tmp, &config_path).await).await;

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

/// The foreground color of the first cell of `needle` on screen.
fn fg_at(terminal: &Terminal<TestBackend>, needle: &str) -> Color {
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .find_map(|y| {
            let line = buffer_line(buf, y);
            let byte = line.find(needle)?;
            let x = line[..byte].chars().count() as u16;
            Some(buf[(x, y)].fg)
        })
        .unwrap_or_else(|| panic!("{needle:?} not on screen"))
}

async fn render_config_file(contents: &str) -> Terminal<TestBackend> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("config.toml");
    std::fs::write(&config_path, contents).expect("write config");
    render_session(open_session(&tmp, &config_path).await).await
}

#[tokio::test]
async fn overriding_primary_recolors_the_cells_that_use_it() {
    let default = render_config_file("").await;
    let overridden = render_config_file("[theme.colors]\nprimary = \"#ff0000\"\n").await;

    let red = Color::Rgb(255, 0, 0);
    assert_ne!(fg_at(&default, "LazyTask v0.1"), red);
    assert_eq!(fg_at(&overridden, "LazyTask v0.1"), red, "header title");
    assert_eq!(fg_at(&overridden, " Tasks ("), red, "task list title");
    assert_eq!(
        overridden.backend().buffer()[(0, 0)].fg,
        red,
        "header border"
    );
}

#[tokio::test]
async fn invalid_color_warns_naming_the_key_and_falls_back() {
    let default = render_config_file("").await;
    let terminal = render_config_file("[theme.colors]\nprimary = \"not-a-color\"\n").await;

    assert!(
        buffer_contains(&terminal, "theme.colors.primary"),
        "warning does not name the key"
    );
    assert_eq!(
        fg_at(&terminal, "LazyTask v0.1"),
        fg_at(&default, "LazyTask v0.1"),
        "primary did not fall back to the palette"
    );
}

#[tokio::test]
async fn unknown_theme_name_warns_and_uses_the_default() {
    let default = render_config_file("").await;
    let terminal = render_config_file("[theme]\nname = \"solarized\"\n").await;

    assert!(buffer_contains(&terminal, "solarized"), "warning missing");
    assert_eq!(
        fg_at(&terminal, "LazyTask v0.1"),
        fg_at(&default, "LazyTask v0.1")
    );
}

/// Draws a session holding a task with every colored attribute, with
/// `no_color` as the `NO_COLOR` variable.
async fn render_with_no_color(no_color: Option<&OsStr>) -> Terminal<TestBackend> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut session = Session::open(
        Some(tmp.path().join("no-config.toml").to_str().unwrap()),
        LaunchEnv {
            taskrc_var: Some(tmp.path().join("no-taskrc").into()),
            taskdata_var: Some(tmp.path().join("data").into()),
            home: None,
            no_color_var: no_color.map(OsStr::to_os_string),
            ..LaunchEnv::default()
        },
    )
    .await
    .expect("session opens");
    session
        .taskchampion
        .add_task(
            "Colorful",
            &[
                ("project", "home"),
                ("priority", "H"),
                ("due", "2020-01-01"),
            ],
        )
        .await
        .expect("add task");
    render_session(session).await
}

fn colored_cells(terminal: &Terminal<TestBackend>) -> usize {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .filter(|cell| {
            [cell.fg, cell.bg, cell.underline_color]
                .iter()
                .any(|&c| c != Color::Reset)
        })
        .count()
}

#[tokio::test]
async fn no_color_renders_without_color() {
    assert_ne!(colored_cells(&render_with_no_color(None).await), 0);
    assert_ne!(
        colored_cells(&render_with_no_color(Some(OsStr::new(""))).await),
        0,
        "an empty NO_COLOR must not disable color"
    );
    assert_eq!(
        colored_cells(&render_with_no_color(Some(OsStr::new("1"))).await),
        0
    );
    assert_eq!(
        colored_cells(&render_with_no_color(Some(OsStr::from_bytes(b"\xff"))).await),
        0,
        "a NO_COLOR that is not UTF-8 is still set"
    );
}
