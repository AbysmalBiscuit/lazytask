use std::path::Path;
use std::time::Duration;

use notify::{EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::Instant;

/// How long a burst of replica writes may run before one reload covers it.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// Poll period when the platform's native file events are unavailable.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Watches a TaskChampion data directory for writes to the replica database,
/// whoever makes them: `task`, a sync, or lazytask itself.
pub struct ReplicaWatcher {
    _watcher: Box<dyn Watcher + Send>,
    changes: mpsc::UnboundedReceiver<()>,
    reload_at: Option<Instant>,
}

impl ReplicaWatcher {
    /// Watches `data_dir` with native file events, falling back to polling
    /// where those are unavailable.
    pub fn new(data_dir: &Path) -> notify::Result<Self> {
        let (tx, changes) = mpsc::unbounded_channel();
        let handler = move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|e| is_replica_write(&e)) {
                let _ = tx.send(());
            }
        };
        let mut watcher: Box<dyn Watcher + Send> =
            match RecommendedWatcher::new(handler.clone(), notify::Config::default()) {
                Ok(native) => Box::new(native),
                Err(_) => Box::new(PollWatcher::new(
                    handler,
                    notify::Config::default().with_poll_interval(POLL_INTERVAL),
                )?),
            };
        watcher.watch(data_dir, RecursiveMode::NonRecursive)?;
        Ok(ReplicaWatcher {
            _watcher: watcher,
            changes,
            reload_at: None,
        })
    }

    /// Resolves once the replica has changed and the debounce window after
    /// the first change has passed. Cancel-safe: a change seen by a dropped
    /// call is still reported by the next one.
    pub async fn changed(&mut self) {
        loop {
            match self.reload_at {
                None => {
                    if self.changes.recv().await.is_none() {
                        return std::future::pending().await;
                    }
                    self.reload_at = Some(Instant::now() + DEBOUNCE);
                }
                Some(at) => {
                    tokio::time::sleep_until(at).await;
                    while self.changes.try_recv().is_ok() {}
                    self.reload_at = None;
                    return;
                }
            }
        }
    }
}

/// Writes to `taskchampion.sqlite3` or its `-wal`/`-journal` files. The
/// `-shm` index changes on reads too, so it is left out to keep a reload
/// from triggering the next one.
fn is_replica_write(event: &notify::Event) -> bool {
    let is_write = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    );
    is_write
        && event.paths.iter().any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    matches!(
                        name,
                        "taskchampion.sqlite3"
                            | "taskchampion.sqlite3-wal"
                            | "taskchampion.sqlite3-journal"
                    )
                })
        })
}
