//! Read-only view of Taskwarrior's taskrc, see taskrc(5) and task-sync(5).

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::taskchampion::SyncSettings;
use crate::utils::helpers::expand_tilde;

/// Taskwarrior refuses deeper nesting, which also stops include cycles.
const MAX_INCLUDE_DEPTH: usize = 10;

/// The settings of a taskrc and every file it includes, later assignments
/// overriding earlier ones.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Taskrc {
    values: HashMap<String, String>,
}

impl Taskrc {
    /// Parses the taskrc at `path`. A missing file yields an empty taskrc;
    /// a missing include or a malformed line is an error naming its file
    /// and line number.
    pub fn load(path: &Path, home: Option<&Path>) -> Result<Self> {
        let mut taskrc = Taskrc::default();
        match fs::read_to_string(path) {
            Ok(contents) => taskrc.parse(path, &contents, home, 0)?,
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to read taskrc {}", path.display()))
            }
        }
        Ok(taskrc)
    }

    fn parse(
        &mut self,
        path: &Path,
        contents: &str,
        home: Option<&Path>,
        depth: usize,
    ) -> Result<()> {
        for (index, raw) in contents.lines().enumerate() {
            let location = || format!("{}:{}", path.display(), index + 1);
            let line = raw.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if key.is_empty() {
                    bail!("{}: malformed entry '{}'", location(), line);
                }
                self.values
                    .insert(key.to_string(), value.trim().to_string());
            } else if let Some(target) = include_target(line) {
                if depth == MAX_INCLUDE_DEPTH {
                    bail!(
                        "{}: includes nested more than {} deep",
                        location(),
                        MAX_INCLUDE_DEPTH
                    );
                }
                let included = resolve_include(path, target, home).with_context(|| {
                    format!("{}: cannot resolve include '{}'", location(), target)
                })?;
                let contents = fs::read_to_string(&included).with_context(|| {
                    format!("{}: cannot read include {}", location(), included.display())
                })?;
                self.parse(&included, &contents, home, depth + 1)?;
            } else {
                bail!("{}: malformed entry '{}'", location(), line);
            }
        }
        Ok(())
    }

    /// The value of `key`; an empty assignment (`key=`) reads as unset.
    fn get(&self, key: &str) -> Option<&str> {
        self.values
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    pub fn data_location(&self) -> Option<PathBuf> {
        self.get("data.location").map(PathBuf::from)
    }

    /// The sync target Taskwarrior would use: a local server directory
    /// first, then a sync server, whose URL may also be given by the
    /// deprecated `sync.server.origin`. A leading `~` in the local server
    /// directory expands to `home`.
    pub fn sync_settings(&self, home: Option<&Path>) -> Result<Option<SyncSettings>> {
        if let Some(server_dir) = self.get("sync.local.server_dir") {
            return Ok(Some(SyncSettings {
                local_server_dir: Some(expand_tilde(Path::new(server_dir), home)?),
                ..Default::default()
            }));
        }
        let Some(server_url) = self
            .get("sync.server.url")
            .or_else(|| self.get("sync.server.origin"))
        else {
            return Ok(None);
        };
        let owned = |key| self.get(key).unwrap_or_default().to_string();
        Ok(Some(SyncSettings {
            server_url: server_url.to_string(),
            client_id: owned("sync.server.client_id"),
            encryption_secret: owned("sync.encryption_secret"),
            local_server_dir: None,
        }))
    }
}

fn include_target(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("include")?;
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

/// Expands a leading `~` to `home`; a relative path is taken from the
/// directory of the file that includes it.
fn resolve_include(including: &Path, target: &str, home: Option<&Path>) -> Result<PathBuf> {
    let target = expand_tilde(Path::new(target), home)?;
    Ok(match including.parent() {
        Some(dir) if target.is_relative() => dir.join(target),
        _ => target,
    })
}
