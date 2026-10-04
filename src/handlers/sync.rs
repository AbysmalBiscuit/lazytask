// Background-style synchronization wrapper around TaskChampionIntegration::sync.
// Owns the watch channel that the UI reads for live status; does not hold a replica
// (only one Replica may be open against a given on-disk database at a time).

use anyhow::{ensure, Context, Result};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use uuid::Uuid;

use crate::taskchampion::{SyncSettings, TaskChampionIntegration};
use crate::taskrc::TaskrcFile;
use crate::ui::components::sync_config::SyncConfig;

pub use crate::ui::components::sync_config::SyncConfigResult;

pub struct SyncHandler {
    sync_status_rx: Option<watch::Receiver<SyncStatus>>,
    sync_status_tx: Option<watch::Sender<SyncStatus>>,
    taskrc: Option<TaskrcFile>,
}

#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub last_sync: Option<Instant>,
    pub sync_error: Option<String>,
    pub progress: SyncProgress,
    pub server_configured: bool,
    pub auto_sync_enabled: bool,
    pub sync_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub phase: SyncPhase,
    pub progress_percent: f32,
    pub message: String,
    pub conflicts_detected: usize,
    pub tasks_uploaded: usize,
    pub tasks_downloaded: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncPhase {
    Idle,
    Connecting,
    Uploading,
    Downloading,
    ResolvingConflicts,
    Finalizing,
    Complete,
    Error,
}

impl SyncHandler {
    /// A handler that saves sync settings to `taskrc`.
    pub fn new(taskrc: Option<TaskrcFile>) -> Self {
        SyncHandler {
            sync_status_rx: None,
            sync_status_tx: None,
            taskrc,
        }
    }

    pub fn initialize(&mut self, taskchampion: &TaskChampionIntegration) -> Result<()> {
        let initial = SyncStatus {
            server_configured: taskchampion.is_sync_configured(),
            ..SyncStatus::default()
        };
        let (tx, rx) = watch::channel(initial);
        self.sync_status_tx = Some(tx);
        self.sync_status_rx = Some(rx);
        Ok(())
    }

    pub fn get_sync_status(&self) -> Option<SyncStatus> {
        self.sync_status_rx.as_ref().map(|rx| rx.borrow().clone())
    }

    pub fn is_sync_configured(&self, taskchampion: &TaskChampionIntegration) -> bool {
        taskchampion.is_sync_configured()
    }

    pub fn is_syncing(&self) -> bool {
        self.get_sync_status()
            .map(|status| status.is_syncing)
            .unwrap_or(false)
    }

    pub async fn start_sync(
        &mut self,
        taskchampion: &mut TaskChampionIntegration,
    ) -> Result<String> {
        self.run_sync(taskchampion, "Starting sync...").await
    }

    pub async fn force_sync(
        &mut self,
        taskchampion: &mut TaskChampionIntegration,
    ) -> Result<String> {
        self.run_sync(taskchampion, "Starting force sync...").await
    }

    async fn run_sync(
        &mut self,
        taskchampion: &mut TaskChampionIntegration,
        opening_message: &str,
    ) -> Result<String> {
        if !taskchampion.is_sync_configured() {
            self.set_status(SyncPhase::Error, "Sync not configured", 0.0, false);
            return Err(anyhow::anyhow!(
                "TaskChampion sync not configured. Use Shift+S to configure."
            ));
        }

        self.set_status(SyncPhase::Connecting, opening_message, 10.0, true);

        let result = taskchampion
            .sync()
            .await
            .context("Failed to sync with server");

        match result {
            Ok(stats) => {
                let msg = if stats.uploaded == 0 && stats.downloaded == 0 {
                    "Sync complete - no changes".to_string()
                } else {
                    format!(
                        "Sync complete: {} uploaded, {} downloaded",
                        stats.uploaded, stats.downloaded
                    )
                };
                self.complete_status(SyncPhase::Complete, &msg);
                Ok(msg)
            }
            Err(e) => {
                let err_msg = format!("{}", e);
                self.complete_status_error(&err_msg);
                Err(e)
            }
        }
    }

    /// The sync server the taskrc sets, as far as it is filled in, even when
    /// it is incomplete or another sync target wins. Empty without a taskrc.
    pub fn saved_server(&self) -> Result<SyncConfig> {
        let Some(taskrc) = &self.taskrc else {
            return Ok(SyncConfig::default());
        };
        let saved = taskrc.load()?;
        let value = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| saved.get(key))
                .unwrap_or_default()
                .to_string()
        };
        Ok(SyncConfig {
            server_url: value(&["sync.server.url", "sync.server.origin"]),
            client_id: value(&["sync.server.client_id"]),
            encryption_secret: value(&["sync.encryption_secret"]),
        })
    }

    /// Saves `config` to the taskrc as its sync server, then syncs to the
    /// target the taskrc now selects. That stays a local directory or cloud
    /// bucket while the taskrc names one, as it does for `task sync`.
    pub async fn configure_sync(
        &mut self,
        taskchampion: &mut TaskChampionIntegration,
        config: &SyncConfig,
    ) -> Result<String> {
        for (label, value) in [
            ("Server URL", &config.server_url),
            ("Client ID", &config.client_id),
            ("Encryption secret", &config.encryption_secret),
        ] {
            ensure!(!value.is_empty(), "{label} is required");
        }
        config
            .client_id
            .parse::<Uuid>()
            .context("Client ID must be a UUID")?;
        let taskrc = self
            .taskrc
            .as_ref()
            .context("No taskrc to save to: set TASKRC or [taskwarrior] taskrc_path")?;
        taskrc.set(&[
            ("sync.server.url", &config.server_url),
            ("sync.server.client_id", &config.client_id),
            ("sync.encryption_secret", &config.encryption_secret),
        ])?;

        let path = taskrc.path().display();
        let settings = taskrc
            .load()
            .and_then(|saved| saved.sync_settings())
            .and_then(|target| target.context("it names no sync target"))
            .with_context(|| format!("Saved to {path}, but its sync settings are unusable"))?;
        let preferred_key = match &settings {
            SyncSettings::Server { .. } => None,
            SyncSettings::Local { .. } => Some("sync.local.server_dir"),
            SyncSettings::Aws { .. } => Some("sync.aws.bucket"),
            SyncSettings::Gcp { .. } => Some("sync.gcp.bucket"),
        };
        taskchampion.configure_sync(settings)?;
        if let Some(tx) = &self.sync_status_tx {
            let mut s = tx.borrow().clone();
            s.server_configured = true;
            let _ = tx.send(s);
        }
        Ok(match preferred_key {
            None => format!("Sync settings saved to {path}"),
            Some(key) => format!(
                "Sync settings saved to {path}, but Taskwarrior syncs to {key} until it is removed"
            ),
        })
    }

    fn set_status(&self, phase: SyncPhase, message: &str, percent: f32, syncing: bool) {
        if let Some(tx) = &self.sync_status_tx {
            let mut status = tx.borrow().clone();
            status.is_syncing = syncing;
            status.progress.phase = phase;
            status.progress.message = message.to_string();
            status.progress.progress_percent = percent;
            status.sync_error = None;
            let _ = tx.send(status);
        }
    }

    fn complete_status(&self, phase: SyncPhase, message: &str) {
        if let Some(tx) = &self.sync_status_tx {
            let mut status = tx.borrow().clone();
            status.is_syncing = false;
            status.last_sync = Some(Instant::now());
            status.progress.phase = phase;
            status.progress.message = message.to_string();
            status.progress.progress_percent = 100.0;
            status.sync_error = None;
            let _ = tx.send(status);
        }
    }

    fn complete_status_error(&self, error: &str) {
        if let Some(tx) = &self.sync_status_tx {
            let mut status = tx.borrow().clone();
            status.is_syncing = false;
            status.progress.phase = SyncPhase::Error;
            status.progress.message = error.to_string();
            status.sync_error = Some(error.to_string());
            let _ = tx.send(status);
        }
    }
}

impl Default for SyncStatus {
    fn default() -> Self {
        SyncStatus {
            is_syncing: false,
            last_sync: None,
            sync_error: None,
            progress: SyncProgress::default(),
            server_configured: false,
            auto_sync_enabled: false,
            sync_interval: Duration::from_secs(300),
        }
    }
}

impl Default for SyncProgress {
    fn default() -> Self {
        SyncProgress {
            phase: SyncPhase::Idle,
            progress_percent: 0.0,
            message: "Ready".to_string(),
            conflicts_detected: 0,
            tasks_uploaded: 0,
            tasks_downloaded: 0,
        }
    }
}
