//! The JSON Schema for `config.toml`, derived from the types serde reads so
//! the two cannot drift. Editors running the TOML language server (taplo)
//! use it for completion, hover docs and validation.

use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::config::Config;

/// The release pipeline attaches the schema to every GitHub Release, so
/// `releases/latest/download` always serves the newest released schema.
pub const ID: &str =
    "https://github.com/AbysmalBiscuit/lazytask/releases/latest/download/lazytask-config.json";

/// The schema document, pretty-printed with a trailing newline.
pub fn document() -> Result<String> {
    let mut schema = serde_json::to_value(schemars::schema_for!(Config))?;
    strip_null(&mut schema);
    let root = schema
        .as_object_mut()
        .context("a struct schema is a JSON object")?;
    root.insert("$id".into(), ID.into());
    root.insert("title".into(), "lazytask config.toml".into());
    Ok(format!("{}\n", serde_json::to_string_pretty(&schema)?))
}

/// TOML has no null, so the `null` schemars allows for an `Option` field
/// could never match; drop it from every `type` list along with the `null`
/// default of an unset `Option`.
fn strip_null(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.get("default") == Some(&Value::Null) {
                map.remove("default");
            }
            if let Some(Value::Array(types)) = map.get_mut("type") {
                types.retain(|t| t != "null");
                if let [only] = types.as_slice() {
                    let only = only.clone();
                    map.insert("type".into(), only);
                }
            }
            map.values_mut().for_each(strip_null);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_null),
        _ => {}
    }
}

const DEFAULTS: &str = include_str!("../config/default.toml");

/// `config/default.toml` with every setting commented out, so nothing is
/// active until its owner uncomments it, and the starter shows the same
/// defaults the shipped file does.
fn starter() -> String {
    let mut starter =
        String::from("# Every key below shows its default. Uncomment a key to change it.\n\n");
    let body = DEFAULTS
        .lines()
        .skip_while(|line| line.is_empty() || line.starts_with('#'));
    for line in body {
        if !line.is_empty() && !line.starts_with('#') {
            starter.push_str("# ");
        }
        starter.push_str(line.trim_end());
        starter.push('\n');
    }
    starter
}

/// What [`init`] did to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitOutcome {
    /// The file already had a `#:schema` directive and was left untouched.
    AlreadyLinked,
    /// The directive was added above the existing contents.
    AddedHeader,
    /// No file existed; a commented-out starter was written.
    WroteStarter,
}

/// Whether `body` opens with a `#:schema` directive among its leading blank
/// and comment lines, the only place taplo reads one.
pub fn has_schema_header(body: &str) -> bool {
    body.lines()
        .map(str::trim_start)
        .take_while(|line| line.is_empty() || line.starts_with('#'))
        .any(|line| line.starts_with("#:schema"))
}

/// Points the config at `path` to the published schema with a `#:schema`
/// header. A file whose header already has a directive is left exactly as it
/// is, so running this again changes nothing.
pub fn init(path: &Path) -> Result<InitOutcome> {
    let header = format!("#:schema {ID}\n");
    let (contents, outcome) = match std::fs::read_to_string(path) {
        Ok(body) if has_schema_header(&body) => return Ok(InitOutcome::AlreadyLinked),
        Ok(body) => (header + &body, InitOutcome::AddedHeader),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            (header + &starter(), InitOutcome::WroteStarter)
        }
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(outcome)
}
