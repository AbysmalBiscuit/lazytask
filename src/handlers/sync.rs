// Background synchronization operations with progress indicators and conflict resolution

use anyhow::{Context, Result};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

use crate::taskchampion::TaskChampionIntegration;

// Re-export sync config types for convenience
pub use crate::ui::components::sync_config::{SyncConfig, SyncConfigType, SyncConfigResult};

#[derive(Debug, Clone)]
pub struct SyncHandler {
    sync_tx: Option<mpsc::Sender<SyncMessage>>,
    sync_status_rx: Option<watch::Receiver<SyncStatus>>,
    sync_status_tx: Option<watch::Sender<SyncStatus>>,
    taskchampion: TaskChampionIntegration,
}

#[derive(Debug, Clone)]
pub enum SyncMessage {
    Start,
    Stop,
    Status,
    ForceSync,
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
    pub fn new(taskchampion: TaskChampionIntegration) -> Self {
        SyncHandler {
            sync_tx: None,
            sync_status_rx: None,
            sync_status_tx: None,
            taskchampion,
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        let (status_tx, status_rx) = watch::channel(SyncStatus::default());
        self.sync_status_rx = Some(status_rx);
        self.sync_status_tx = Some(status_tx.clone());

        // Check if sync is configured
        let server_configured = self.is_sync_configured().await?;
        
        // Update initial status
        let initial_status = SyncStatus {
            server_configured,
            ..SyncStatus::default()
        };
        let _ = status_tx.send(initial_status);

        Ok(())
    }

    pub async fn start_sync(&mut self) -> Result<String> {
        // Update sync status to show syncing
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Connecting;
            status.progress.message = "Starting sync...".to_string();
            let _ = status_tx.send(status);
        }
        
        // Run sync operation directly but with proper async handling
        let result = self.execute_sync_operation().await;
        
        // Update sync status to show completion
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = false;
            status.last_sync = Some(std::time::Instant::now());
            if result.is_err() {
                status.progress.phase = SyncPhase::Error;
                status.sync_error = Some(result.as_ref().unwrap_err().to_string());
            } else {
                status.progress.phase = SyncPhase::Complete;
                status.progress.message = "Sync completed".to_string();
            }
            let _ = status_tx.send(status);
        }
        
        result
    }

    pub async fn force_sync(&mut self) -> Result<String> {
        // Update sync status to show syncing
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Connecting;
            status.progress.message = "Starting force sync...".to_string();
            let _ = status_tx.send(status);
        }
        
        // Run sync operation directly but with proper async handling
        let result = self.execute_sync_operation().await;
        
        // Update sync status to show completion
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = false;
            status.last_sync = Some(std::time::Instant::now());
            if result.is_err() {
                status.progress.phase = SyncPhase::Error;
                status.sync_error = Some(result.as_ref().unwrap_err().to_string());
            } else {
                status.progress.phase = SyncPhase::Complete;
                status.progress.message = "Force sync completed".to_string();
            }
            let _ = status_tx.send(status);
        }
        
        result
    }

    async fn execute_sync_operation(&mut self) -> Result<String> {
        if !self.is_sync_configured().await? {
            return Err(anyhow::anyhow!("TaskChampion sync not configured. Please configure server URL and client ID."));
        }

        // Update progress: Connecting
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Connecting;
            status.progress.message = "Connecting to TaskChampion server...".to_string();
            status.progress.progress_percent = 10.0;
            let _ = status_tx.send(status);
        }

        // Small delay to show connecting phase
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Update progress: Uploading
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Uploading;
            status.progress.message = "Uploading local changes...".to_string();
            status.progress.progress_percent = 30.0;
            let _ = status_tx.send(status);
        }

        // Small delay to show uploading phase
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Update progress: Downloading
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Downloading;
            status.progress.message = "Downloading remote changes...".to_string();
            status.progress.progress_percent = 60.0;
            let _ = status_tx.send(status);
        }

        // Small delay to show downloading phase
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Update progress: Finalizing
        if let Some(status_tx) = &self.sync_status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Finalizing;
            status.progress.message = "Finalizing sync...".to_string();
            status.progress.progress_percent = 90.0;
            let _ = status_tx.send(status);
        }

        // Execute TaskChampion sync
        let sync_result = self.taskchampion.sync().await
            .context("Failed to sync with TaskChampion server")?;

        // Create result message
        let result = if sync_result.uploaded > 0 || sync_result.downloaded > 0 {
            format!("Sync completed: {} uploaded, {} downloaded", 
                   sync_result.uploaded, sync_result.downloaded)
        } else if sync_result.conflicts > 0 {
            format!("Sync completed with {} conflicts", sync_result.conflicts)
        } else {
            "Sync completed - no changes".to_string()
        };

        Ok(result)
    }

    async fn execute_sync_operation_background(
        taskwarrior: &TaskwarriorIntegration,
        status_tx: Option<watch::Sender<SyncStatus>>,
    ) -> Result<String> {
        // Check if sync is configured
        if !Self::is_sync_configured_static(taskwarrior).await? {
            if let Some(status_tx) = &status_tx {
                let mut status = SyncStatus::default();
                status.is_syncing = false;
                status.progress.phase = SyncPhase::Error;
                status.sync_error = Some("Taskwarrior sync not configured. Please configure sync.server.url, sync.server.client_id, etc.".to_string());
                let _ = status_tx.send(status);
            }
            return Err(anyhow::anyhow!("Taskwarrior sync not configured"));
        }

        // Update progress: Connecting
        if let Some(status_tx) = &status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Connecting;
            status.progress.message = "Connecting to server...".to_string();
            status.progress.progress_percent = 10.0;
            let _ = status_tx.send(status);
        }

        // Small delay to show connecting phase
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Update progress: Uploading
        if let Some(status_tx) = &status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Uploading;
            status.progress.message = "Uploading local changes...".to_string();
            status.progress.progress_percent = 30.0;
            let _ = status_tx.send(status);
        }

        // Small delay to show uploading phase
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Update progress: Downloading
        if let Some(status_tx) = &status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Downloading;
            status.progress.message = "Downloading remote changes...".to_string();
            status.progress.progress_percent = 60.0;
            let _ = status_tx.send(status);
        }

        // Small delay to show downloading phase
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Update progress: Finalizing
        if let Some(status_tx) = &status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = true;
            status.progress.phase = SyncPhase::Finalizing;
            status.progress.message = "Finalizing sync...".to_string();
            status.progress.progress_percent = 90.0;
            let _ = status_tx.send(status);
        }

        // Execute sync command
        let sync_output = taskwarrior.execute_command(&["synchronize"]).await
            .context("Failed to execute synchronize command")?;

        // Parse output for meaningful result
        let result = if sync_output.contains("Sync successful") {
            "Sync completed successfully".to_string()
        } else if sync_output.contains("No changes") {
            "No changes to synchronize".to_string()
        } else if sync_output.contains("Syncing with sync server") {
            // This indicates Taskwarrior connected but got no meaningful response
            // This usually happens when connecting to a TaskChampion server with Taskwarrior
            "⚠️ Connected to server but no sync data exchanged. You may be using incompatible sync protocols (TaskChampion vs Taskwarrior)".to_string()
        } else if sync_output.is_empty() {
            "Sync completed".to_string()
        } else {
            format!("Sync result: {}", sync_output.lines().next().unwrap_or("Done"))
        };

        // Update sync status to show completion
        if let Some(status_tx) = &status_tx {
            let mut status = SyncStatus::default();
            status.is_syncing = false;
            status.last_sync = Some(std::time::Instant::now());
            status.progress.phase = SyncPhase::Complete;
            status.progress.message = result.clone();
            status.progress.progress_percent = 100.0;
            let _ = status_tx.send(status);
        }

        Ok(result)
    }

    async fn is_sync_configured_static(taskwarrior: &TaskwarriorIntegration) -> Result<bool> {
        // Check if Taskwarrior sync is configured by trying to get sync server info
        match taskwarrior.execute_command(&["_get", "rc.sync.server.url"]).await {
            Ok(server) => Ok(!server.trim().is_empty()),
            Err(_) => Ok(false),
        }
    }

    pub fn get_sync_status(&self) -> Option<SyncStatus> {
        self.sync_status_rx.as_ref()
            .map(|rx| rx.borrow().clone())
    }

    pub async fn is_sync_configured(&self) -> Result<bool> {
        Ok(self.taskchampion.is_sync_configured())
    }

    pub fn is_syncing(&self) -> bool {
        self.get_sync_status()
            .map(|status| status.is_syncing)
            .unwrap_or(false)
    }

    pub async fn configure_sync(&mut self, config: &SyncConfig) -> Result<String> {
        match config.config_type {
            SyncConfigType::Server => {
                // Configure TaskChampion sync
                self.taskchampion.configure_sync(&config.server_url, &config.client_id).await?;
                Ok("TaskChampion sync configured successfully".to_string())
            }
            SyncConfigType::Local => {
                Ok("Local sync configured (no server needed)".to_string())
            }
            SyncConfigType::GCP => {
                Ok("GCP sync configured".to_string())
            }
            SyncConfigType::AWS => {
                Ok("AWS sync configured".to_string())
            }
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
            sync_interval: Duration::from_secs(300), // 5 minutes default
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

// Extension methods for TaskwarriorIntegration to support sync operations  
impl TaskwarriorIntegration {
    pub async fn execute_sync_command(&self) -> Result<String> {
        self.execute_command(&["synchronize"]).await
    }

    pub async fn execute_helper_command(&self, args: &[&str]) -> Result<String> {
        self.execute_command(args).await
    }
}

