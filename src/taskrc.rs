//! Taskwarrior's taskrc, see taskrc(5) and task-sync(5).

use anyhow::{bail, ensure, Context, Result};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::app::LaunchEnv;
use crate::taskchampion::{AwsCredentials, SyncSettings};
use crate::utils::helpers::expand_tilde;

/// Taskwarrior refuses a chain of more files than this, which also stops
/// include cycles.
const MAX_INCLUDE_DEPTH: usize = 10;

const ENCRYPTION_SECRET: &str = "sync.encryption_secret";

/// A taskrc on disk, with the launch environment its includes and values
/// resolve against.
#[derive(Debug, Clone)]
pub struct TaskrcFile {
    path: PathBuf,
    env: LaunchEnv,
}

impl TaskrcFile {
    pub fn new(path: PathBuf, env: LaunchEnv) -> Self {
        TaskrcFile { path, env }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Taskrc> {
        Taskrc::load(&self.path, &self.env)
    }

    /// Sets each key to its value. A key already assigned is changed on the
    /// line of the assignment in effect, in whichever file holds it, keeping
    /// that line's comment; a key assigned nowhere is appended to this file,
    /// which is created readable only by its owner when missing. A key that
    /// already reads as its value is left alone.
    ///
    /// Nothing is written when a value would not read back as given, or
    /// when `sync.encryption_secret` would go into a file other users can
    /// read.
    pub fn set(&self, assignments: &[(&str, &str)]) -> Result<()> {
        let taskrc = self.load()?;
        let mut edits: BTreeMap<&Path, Vec<(usize, String)>> = BTreeMap::new();
        let mut appended = String::new();
        for &(key, value) in assignments {
            ensure_reads_back(key, value, &self.env)?;
            let current = taskrc.values.get(key);
            if current.is_some_and(|assigned| assigned.value == value) {
                continue;
            }
            if key == ENCRYPTION_SECRET {
                ensure_private(current.map_or(&self.path, |assigned| &assigned.file))?;
            }
            match current {
                Some(assigned) => edits
                    .entry(&assigned.file)
                    .or_default()
                    .push((assigned.line, format!("{key}={value}"))),
                None => appended.push_str(&format!("{key}={value}\n")),
            }
        }
        for (file, lines) in edits {
            replace_lines(file, &lines)?;
        }
        if !appended.is_empty() {
            append(&self.path, &appended)?;
        }
        Ok(())
    }
}

/// The settings of a taskrc and every file it includes, later assignments
/// overriding earlier ones. As in Taskwarrior, a leading `~` and any
/// `$NAME` variable in a value or include path are expanded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Taskrc {
    values: HashMap<String, Assignment>,
}

/// A key's value and the line that assigned it, by its index in `file`.
#[derive(Debug, Clone, PartialEq)]
struct Assignment {
    value: String,
    file: PathBuf,
    line: usize,
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
                self.values.insert(
                    key.to_string(),
                    Assignment {
                        value,
                        file: path.to_path_buf(),
                        line: index,
                    },
                );
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
            .map(|assigned| assigned.value.as_str())
            .filter(|v| !v.is_empty())
    }

    pub fn data_location(&self) -> Option<PathBuf> {
        self.get("data.location").map(PathBuf::from)
    }

    /// The sync target Taskwarrior's `task sync` would use, checked the same
    /// way: a local server directory first, then an AWS bucket, then a GCP
    /// bucket, then a sync server, whose URL may also be given by the
    /// deprecated `sync.server.origin`.
    pub fn sync_settings(&self) -> Result<Option<SyncSettings>> {
        let value = |key| self.get(key).map(str::to_string);
        let value_or_empty = |key| value(key).unwrap_or_default();
        let encryption_secret =
            || value("sync.encryption_secret").context("sync.encryption_secret is required");

        if let Some(server_dir) = self.get("sync.local.server_dir") {
            return Ok(Some(SyncSettings::Local {
                server_dir: Some(PathBuf::from(server_dir)),
            }));
        }
        if let Some(bucket) = value("sync.aws.bucket") {
            let region = value("sync.aws.region").context("sync.aws.region is required")?;
            let encryption_secret = encryption_secret()?;
            let profile = value("sync.aws.profile");
            let access_key = ["sync.aws.access_key_id", "sync.aws.secret_access_key"]
                .iter()
                .any(|key| self.get(key).is_some());
            let default = self.get("sync.aws.default_credentials").is_some();
            let credentials = match (profile, access_key, default) {
                (Some(profile), false, false) => AwsCredentials::Profile(profile),
                (None, true, false) => AwsCredentials::AccessKey {
                    access_key_id: value_or_empty("sync.aws.access_key_id"),
                    secret_access_key: value_or_empty("sync.aws.secret_access_key"),
                },
                (None, false, true) => AwsCredentials::Default,
                _ => bail!("exactly one method of specifying AWS credentials is required"),
            };
            return Ok(Some(SyncSettings::Aws {
                region,
                bucket,
                credentials,
                encryption_secret,
            }));
        }
        if let Some(bucket) = value("sync.gcp.bucket") {
            return Ok(Some(SyncSettings::Gcp {
                bucket,
                credential_path: value("sync.gcp.credential_path"),
                encryption_secret: encryption_secret()?,
            }));
        }
        let Some(url) = value("sync.server.url").or_else(|| value("sync.server.origin")) else {
            return Ok(None);
        };
        let (Some(client_id), Some(encryption_secret)) = (
            value("sync.server.client_id"),
            value("sync.encryption_secret"),
        ) else {
            bail!("sync.server.client_id and sync.encryption_secret are required");
        };
        Ok(Some(SyncSettings::Server {
            url,
            client_id,
            encryption_secret,
        }))
    }
}

/// Fails unless `key=value` written to a taskrc reads back as `value`.
fn ensure_reads_back(key: &str, value: &str, env: &LaunchEnv) -> Result<()> {
    let mut parsed = Taskrc::default();
    let reads_back = !value.contains(['\n', '\r'])
        && parsed
            .parse(Path::new(key), &format!("{key}={value}"), env, 0)
            .is_ok()
        && parsed.get(key) == Some(value);
    ensure!(
        reads_back,
        "{key} cannot be saved as entered: a taskrc value cannot be empty, contain '#' \
         or start or end with a space, and a leading '~' or a '$NAME' in it expands"
    );
    Ok(())
}

#[cfg(unix)]
fn ensure_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = match fs::metadata(path) {
        Ok(meta) => meta.permissions().mode(),
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("Failed to read {}", path.display())),
    };
    ensure!(
        mode & 0o044 == 0,
        "Not saving {ENCRYPTION_SECRET} to {path}: other users can read it. Run `chmod 600 {path}`, \
         then save again",
        path = path.display()
    );
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private(_path: &Path) -> Result<()> {
    Ok(())
}

/// Replaces each numbered line of `file` with its new text, keeping the
/// line's comment and line ending.
fn replace_lines(file: &Path, lines: &[(usize, String)]) -> Result<()> {
    let contents =
        fs::read_to_string(file).with_context(|| format!("Failed to read {}", file.display()))?;
    let mut out = String::with_capacity(contents.len());
    for (index, raw) in contents.split_inclusive('\n').enumerate() {
        let Some((_, text)) = lines.iter().find(|(line, _)| *line == index) else {
            out.push_str(raw);
            continue;
        };
        let body = raw.trim_end_matches(['\n', '\r']);
        let code = body.split('#').next().unwrap_or_default();
        let comment = if code.len() == body.len() {
            ""
        } else {
            &body[code.trim_end().len()..]
        };
        out.push_str(text);
        out.push_str(comment);
        out.push_str(&raw[body.len()..]);
    }
    fs::write(file, out).with_context(|| format!("Failed to write {}", file.display()))
}

/// Appends `lines` to `file`, first ending its last line when it has no
/// line ending. A missing file is created readable only by its owner.
fn append(file: &Path, lines: &str) -> Result<()> {
    let ends_mid_line = match fs::read(file) {
        Ok(contents) => contents.last().is_some_and(|&byte| byte != b'\n'),
        Err(err) if err.kind() == ErrorKind::NotFound => false,
        Err(err) => return Err(err).with_context(|| format!("Failed to read {}", file.display())),
    };
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut handle = options
        .open(file)
        .with_context(|| format!("Failed to open {}", file.display()))?;
    let separator = if ends_mid_line { "\n" } else { "" };
    write!(handle, "{separator}{lines}")
        .with_context(|| format!("Failed to write {}", file.display()))
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
