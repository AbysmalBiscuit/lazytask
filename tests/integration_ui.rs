// Integration test: drive the UI through ratatui's TestBackend so we can
// confirm the screens render without panicking and contain the right markers.
// This catches regressions like the UTF-8 byte-slicing panics or missing
// modules without requiring a real terminal.

use lazytask::config::Config;
use lazytask::handlers::sync::SyncHandler;
use lazytask::taskchampion::TaskChampionIntegration;
use lazytask::ui::app_ui::AppUI;
use ratatui::{backend::TestBackend, Terminal};

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
    let engine = TaskChampionIntegration::new(Some(tmp.path().to_path_buf()))
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

    let loaded = Config::load(Some(config_path.to_str().unwrap())).expect("config loads");
    assert!(!loaded.config.ui.show_help_bar);

    let mut ui = AppUI::new(&loaded.config).expect("AppUI::new");
    ui.warn_unknown_config_keys(&loaded.unknown_keys);
    let sync_handler = SyncHandler::new();
    let mut engine = TaskChampionIntegration::new(Some(tmp.path().join("data")))
        .await
        .expect("engine");
    ui.load_tasks(&mut engine).await.expect("load_tasks");

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    terminal
        .draw(|f| ui.render_with_sync(f, &sync_handler))
        .expect("draw");

    assert!(buffer_contains(&terminal, "ui.colour"), "ui.colour not shown");
    assert!(
        buffer_contains(&terminal, "taskwarrior.sync_enabled"),
        "taskwarrior.sync_enabled not shown"
    );
}
