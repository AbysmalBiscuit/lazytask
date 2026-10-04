//! The committed `schema/lazytask-config.json` must match what `lazytask
//! schema` prints. A stale schema is worse than none: editors would flag valid
//! config as invalid and miss the keys it does not know.
//!
//! `LAZYTASK_UPDATE_SCHEMA=1 cargo test --test config_schema` rewrites the file
//! instead of failing, so the run that catches the drift also fixes it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SCHEMA_ID: &str =
    "https://github.com/AbysmalBiscuit/lazytask/releases/latest/download/lazytask-config.json";

fn lazytask(args: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_lazytask"))
        .args(args)
        .output()
        .expect("run lazytask");
    assert!(
        output.status.success(),
        "lazytask {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn committed_schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/lazytask-config.json")
}

#[test]
fn committed_schema_matches_the_config_types() {
    let generated = String::from_utf8(lazytask(&["schema"]).stdout).unwrap();
    let path = committed_schema_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    if committed == generated {
        return;
    }
    if std::env::var("LAZYTASK_UPDATE_SCHEMA").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
        return;
    }

    let (line, committed_line, generated_line) = committed
        .split('\n')
        .map(Some)
        .chain(std::iter::repeat(None))
        .zip(
            generated
                .split('\n')
                .map(Some)
                .chain(std::iter::repeat(None)),
        )
        .enumerate()
        .find(|(_, (c, g))| c != g)
        .map(|(i, (c, g))| {
            (
                i + 1,
                c.unwrap_or("<end of file>"),
                g.unwrap_or("<end of file>"),
            )
        })
        .expect("files differ, so some line differs");
    panic!(
        "schema/lazytask-config.json is stale; regenerate it with \
         `LAZYTASK_UPDATE_SCHEMA=1 cargo test --test config_schema`.\n\
         First difference, line {line}:\n  committed: {committed_line}\n  generated: {generated_line}"
    );
}

#[test]
fn schema_init_adds_the_header_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = "[ui]\nshow_help_bar = false\n";
    std::fs::write(&path, original).unwrap();
    let path_arg = path.to_str().unwrap();

    lazytask(&["schema", "init", path_arg]);
    let first = std::fs::read_to_string(&path).unwrap();
    assert_eq!(first, format!("#:schema {SCHEMA_ID}\n{original}"));

    lazytask(&["schema", "init", path_arg]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
}

#[test]
fn schema_init_writes_a_starter_that_loads_as_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lazytask").join("config.toml");

    lazytask(&["schema", "init", path.to_str().unwrap()]);

    let starter = std::fs::read_to_string(&path).unwrap();
    assert!(starter.starts_with(&format!("#:schema {SCHEMA_ID}\n")));
    let loaded = lazytask::config::Config::load(path.to_str()).unwrap();
    assert_eq!(loaded.config, lazytask::config::Config::default());
    assert!(loaded.unknown_keys.is_empty());
}
