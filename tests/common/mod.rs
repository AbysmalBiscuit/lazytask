//! Drives the real App event loop on a TestBackend, with key presses sent
//! through its input channel.

// Each test crate uses its own subset of the driver.
#![allow(dead_code)]

use std::time::Duration;

use anyhow::{bail, Result};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazytask::app::{App, Session};
use ratatui::{backend::TestBackend, Terminal};
use tokio::sync::mpsc;

pub struct Driver {
    pub app: App<TestBackend>,
    keys: mpsc::UnboundedSender<Event>,
}

impl Driver {
    /// The app on `session` as the binary runs it, on a 220x40 screen.
    pub async fn new(session: Session) -> Result<Self> {
        let (keys, input) = mpsc::unbounded_channel();
        let terminal = Terminal::new(TestBackend::new(220, 40))?;
        let app = App::with_terminal(terminal, session, input).await?;
        Ok(Driver { app, keys })
    }

    pub fn press(&self, code: KeyCode) {
        self.keys
            .send(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
            .expect("app input closed");
    }

    pub fn type_text(&self, text: &str) {
        text.chars().for_each(|c| self.press(KeyCode::Char(c)));
    }

    /// Replaces the focused field's `old` text with `new`.
    pub fn retype(&self, old: &str, new: &str) {
        old.chars().for_each(|_| self.press(KeyCode::Backspace));
        self.type_text(new);
    }

    pub fn open_sync_modal(&self) {
        self.type_text("S");
    }

    /// Runs the event loop until `done` holds for the screen, or `within`
    /// passes. Returns whether `done` held; a failed step is an error.
    pub async fn step_until(
        &mut self,
        within: Duration,
        done: impl Fn(&str) -> bool,
    ) -> Result<bool> {
        let deadline = tokio::time::Instant::now() + within;
        while !done(&self.screen()) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, self.app.step()).await {
                Ok(stepped) => stepped?,
                Err(_) => return Ok(false),
            }
        }
        Ok(true)
    }

    /// Runs the event loop until the screen contains `needle`, returning the
    /// screen, or fails with the last screen after five seconds.
    pub async fn wait_for(&mut self, needle: &str) -> Result<String> {
        if !self
            .step_until(Duration::from_secs(5), |s| s.contains(needle))
            .await?
        {
            bail!("{needle:?} never appeared:\n{}", self.screen());
        }
        Ok(self.screen())
    }

    pub fn screen(&self) -> String {
        let buf = self.app.terminal.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
