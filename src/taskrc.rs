//! Read-only view of Taskwarrior's taskrc, see taskrc(5) and task-sync(5).

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::app::LaunchEnv;
use crate::taskchampion::SyncSettings;
use crate::utils::helpers::expand_tilde;

/// Taskwarrior refuses a chain of more files than this, which also stops
/// include cycles.
const MAX_INCLUDE_DEPTH: usize = 10;

/// The settings of a taskrc and every file it includes, later assignments
/// overriding earlier ones. As in Taskwarrior, a leading `~` and any
/// `$NAME` variable in a value or include path are expanded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Taskrc {
    values: HashMap<String, String>,
}

impl Taskrc {
    /// Parses the taskrc at `path`. A missing file yields an empty taskrc;
    /// a missing include or a malformed line is an error naming its file
    /// and line number.
    pub fn load(path: &Path, env: &LaunchEnv) -> Result<Self> {
        let mut taskrc = Taskrc::default();
        match fs::read_to_string(path) {
            Ok(contents) => taskrc.parse(path, &contents, env, 0)?,
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to read taskrc {}", path.display()))
            }
        }
        Ok(taskrc)
    }

    fn parse(&mut self, path: &Path, contents: &str, env: &LaunchEnv, depth: usize) -> Result<()> {
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
                let value = expand(value.trim(), env).with_context(location)?;
                self.values.insert(key.to_string(), value);
            } else if let Some(target) = include_target(line) {
                if depth + 1 == MAX_INCLUDE_DEPTH {
                    bail!(
                        "{}: includes nested more than {} deep",
                        location(),
                        MAX_INCLUDE_DEPTH
                    );
                }
                let included = find_include(path, target, env)
                    .with_context(|| format!("{}: cannot find include '{}'", location(), target))?;
                let contents = fs::read_to_string(&included).with_context(|| {
                    format!("{}: cannot read include {}", location(), included.display())
                })?;
                self.parse(&included, &contents, env, depth + 1)?;
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
    /// deprecated `sync.server.origin`.
    pub fn sync_settings(&self) -> Result<Option<SyncSettings>> {
        if let Some(server_dir) = self.get("sync.local.server_dir") {
            return Ok(Some(SyncSettings {
                local_server_dir: Some(PathBuf::from(server_dir)),
                ..Default::default()
            }));
        }
        let Some(server_url) = self
            .get("sync.server.url")
            .or_else(|| self.get("sync.server.origin"))
        else {
            return Ok(None);
        };
        let value_or_empty = |key| self.get(key).unwrap_or_default().to_string();
        Ok(Some(SyncSettings {
            server_url: server_url.to_string(),
            client_id: value_or_empty("sync.server.client_id"),
            encryption_secret: value_or_empty("sync.encryption_secret"),
            local_server_dir: None,
        }))
    }
}

fn include_target(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("include")?;
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

/// Finds an include the way Taskwarrior does: an absolute path as is, a
/// relative one in the working directory, then next to the real path of
/// the including file, then in the package rc directories.
fn find_include(including: &Path, target: &str, env: &LaunchEnv) -> Result<PathBuf> {
    let target = PathBuf::from(expand(target, env)?);
    if target.is_absolute() {
        return Ok(target);
    }
    let including_dir = fs::canonicalize(including)
        .ok()
        .and_then(|real| real.parent().map(Path::to_path_buf));
    env.cwd
        .iter()
        .chain(&including_dir)
        .chain(&env.rc_dirs)
        .map(|dir| dir.join(&target))
        .find(|candidate| candidate.exists())
        .context("not in the working directory, the taskrc's directory or a package rc directory")
}

/// Expands a leading `~` to the home directory and each `$NAME` to that
/// variable, or to nothing when it is unset.
fn expand(text: &str, env: &LaunchEnv) -> Result<String> {
    let text = if text == "~" || text.starts_with("~/") {
        expand_tilde(Path::new(text), env.home.as_deref())?
            .to_string_lossy()
            .into_owned()
    } else {
        text.to_string()
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        let after = &rest[dollar + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if name_len == 0 {
            out.push('$');
        } else if let Some(value) = env.vars.get(&after[..name_len]) {
            out.push_str(value);
        }
        rest = &after[name_len..];
    }
    out.push_str(rest);
    Ok(out)
}
