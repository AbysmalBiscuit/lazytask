//! `lazytask doctor` run as the binary, in a scratch home directory with no
//! inherited environment, so every source it reports is one the test set up.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const SCHEMA_HEADER: &str =
    "#:schema https://github.com/AbysmalBiscuit/lazytask/releases/latest/download/lazytask-config.json\n";

struct Doctor {
    home: TempDir,
    env: Vec<(&'static str, PathBuf)>,
}

struct Run {
    stdout: String,
    success: bool,
}

impl Doctor {
    fn new() -> Self {
        Doctor {
            home: tempfile::tempdir().expect("tempdir"),
            env: Vec::new(),
        }
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    fn env(mut self, name: &'static str, value: impl Into<PathBuf>) -> Self {
        self.env.push((name, value.into()));
        self
    }

    /// Writes `contents` to `name` under the home directory, creating its
    /// parents, and returns the file's path.
    fn file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.home().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// A config at the default location that passes every config check.
    fn clean_default_config(&self) -> PathBuf {
        self.file(".config/lazytask/config.toml", SCHEMA_HEADER)
    }

    fn run(&self, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_lazytask"))
            .arg("doctor")
            .args(args)
            .env_clear()
            .env("HOME", self.home())
            .env("XDG_CONFIG_HOME", self.home().join(".config"))
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .output()
            .expect("run lazytask doctor");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.stderr.is_empty(),
            "doctor wrote to stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Run {
            stdout,
            success: output.status.success(),
        }
    }
}

impl Run {
    /// The report block for check `name`: its `[status] name` line and the
    /// indented lines under it.
    fn check(&self, name: &str) -> String {
        let mut lines = self.stdout.lines();
        let header = lines
            .by_ref()
            .find(|line| line.starts_with('[') && line.ends_with(&format!("] {name}")))
            .unwrap_or_else(|| panic!("no {name:?} check in:\n{}", self.stdout));
        std::iter::once(header)
            .chain(lines.take_while(|line| line.starts_with(' ')))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[test]
fn missing_config_flag_file_fails_the_config_check() {
    let doctor = Doctor::new();
    let missing = doctor.home().join("typo.toml");

    let run = doctor.run(&["--config", missing.to_str().unwrap()]);

    let check = run.check("Config file");
    assert!(check.starts_with("[fail]"), "{check}");
    assert!(
        check.contains(&format!("value:  {}", missing.display())),
        "{check}"
    );
    assert!(check.contains("source: --config"), "{check}");
    assert!(check.contains("fail:   not found"), "{check}");
}

#[test]
fn unknown_keys_and_a_missing_schema_header_are_warnings() {
    let doctor = Doctor::new();
    let config = doctor.file(
        ".config/lazytask/config.toml",
        "[ui]\nshow_help_bar = false\ncolour = \"red\"\n",
    );

    let run = doctor.run(&[]);

    let check = run.check("Config file");
    assert!(check.starts_with("[warn]"), "{check}");
    assert!(
        check.contains(&format!("value:  {}", config.display())),
        "{check}"
    );
    assert!(check.contains("source: default"), "{check}");
    assert!(check.contains("warn:   unknown keys: ui.colour"), "{check}");
    assert!(check.contains("warn:   no #:schema header"), "{check}");
    assert!(check.contains("lazytask schema init"), "{check}");
}

#[test]
fn config_with_a_header_and_known_keys_passes() {
    let doctor = Doctor::new();
    doctor.clean_default_config();

    let run = doctor.run(&[]);

    let check = run.check("Config file");
    assert!(check.starts_with("[pass]"), "{check}");
}

/// A run's `value:` and `source:` lines for check `name`.
fn value_and_source(run: &Run, name: &str) -> (String, String) {
    let check = run.check(name);
    let line = |label: &str| {
        check
            .lines()
            .find_map(|line| line.trim_start().strip_prefix(label))
            .unwrap_or_else(|| panic!("no {label:?} in:\n{check}"))
            .trim()
            .to_string()
    };
    (line("value:"), line("source:"))
}

#[test]
fn taskrc_reports_each_source_in_precedence_order() {
    let doctor = Doctor::new();
    doctor.clean_default_config();
    let run = doctor.run(&[]);
    let default = doctor.home().join(".taskrc");
    assert_eq!(
        value_and_source(&run, "Taskrc"),
        (default.display().to_string(), "default".into())
    );

    let doctor = Doctor::new();
    doctor.clean_default_config();
    let from_env = doctor.file("env.taskrc", "");
    let doctor = doctor.env("TASKRC", &from_env);
    assert_eq!(
        value_and_source(&doctor.run(&[]), "Taskrc"),
        (from_env.display().to_string(), "TASKRC".into())
    );

    let doctor = Doctor::new();
    let from_config = doctor.file("config.taskrc", "");
    let config = doctor.file(
        "config.toml",
        &format!(
            "{SCHEMA_HEADER}[taskwarrior]\ntaskrc_path = \"{}\"\n",
            from_config.display()
        ),
    );
    let from_env = doctor.file("env.taskrc", "");
    let doctor = doctor.env("TASKRC", &from_env);
    assert_eq!(
        value_and_source(
            &doctor.run(&["--config", config.to_str().unwrap()]),
            "Taskrc"
        ),
        (
            from_config.display().to_string(),
            "config taskwarrior.taskrc_path".into()
        )
    );
}

#[test]
fn data_directory_reports_each_source_in_precedence_order() {
    let doctor = Doctor::new();
    doctor.clean_default_config();
    assert_eq!(
        value_and_source(&doctor.run(&[]), "Data directory"),
        (
            doctor.home().join(".task").display().to_string(),
            "default".into()
        )
    );

    let doctor = Doctor::new();
    doctor.clean_default_config();
    let from_taskrc = doctor.home().join("taskrc-data");
    doctor.file(
        ".taskrc",
        &format!("data.location={}\n", from_taskrc.display()),
    );
    let run = doctor.run(&[]);
    assert_eq!(
        value_and_source(&run, "Data directory"),
        (
            from_taskrc.display().to_string(),
            "taskrc data.location".into()
        )
    );

    let doctor = Doctor::new();
    doctor.clean_default_config();
    doctor.file(".taskrc", "data.location=/from/taskrc\n");
    let from_env = doctor.home().join("env-data");
    let doctor = doctor.env("TASKDATA", &from_env);
    assert_eq!(
        value_and_source(&doctor.run(&[]), "Data directory"),
        (from_env.display().to_string(), "TASKDATA".into())
    );

    let doctor = Doctor::new();
    let from_config = doctor.home().join("config-data");
    let config = doctor.file(
        "config.toml",
        &format!(
            "{SCHEMA_HEADER}[taskwarrior]\ndata_location = \"{}\"\n",
            from_config.display()
        ),
    );
    let doctor = doctor.env("TASKDATA", "/from/env");
    assert_eq!(
        value_and_source(
            &doctor.run(&["--config", config.to_str().unwrap()]),
            "Data directory"
        ),
        (
            from_config.display().to_string(),
            "config taskwarrior.data_location".into()
        )
    );
}

#[test]
fn missing_data_directory_is_reported_and_left_uncreated() {
    let doctor = Doctor::new();
    doctor.clean_default_config();
    let missing = doctor.home().join("no-such-data");
    let doctor = doctor.env("TASKDATA", &missing);

    let run = doctor.run(&[]);

    let check = run.check("Data directory");
    assert!(check.starts_with("[warn]"), "{check}");
    assert!(check.contains("warn:   does not exist"), "{check}");
    assert!(!missing.exists(), "doctor created {}", missing.display());
}

#[tokio::test]
async fn existing_replica_opens_and_reports_its_tasks() {
    let doctor = Doctor::new();
    doctor.clean_default_config();
    let data = doctor.home().join("data");
    let mut replica = lazytask::taskchampion::TaskChampionIntegration::new(data.clone())
        .await
        .unwrap();
    replica.add_task("one", &[]).await.unwrap();
    drop(replica);
    let doctor = doctor.env("TASKDATA", &data);

    let run = doctor.run(&[]);

    let check = run.check("Data directory");
    assert!(check.starts_with("[pass]"), "{check}");
    assert!(check.ends_with("note:   replica opens, 1 task"), "{check}");
}

#[test]
fn warnings_alone_exit_zero() {
    let doctor = Doctor::new();
    doctor.file(".config/lazytask/config.toml", "[ui]\ncolour = \"red\"\n");

    let run = doctor.run(&[]);

    assert!(run.stdout.contains("[warn] Config file"), "{}", run.stdout);
    assert!(!run.stdout.contains("[fail]"), "{}", run.stdout);
    assert!(run.success, "exited non-zero:\n{}", run.stdout);
}

#[test]
fn a_failed_check_exits_non_zero() {
    let doctor = Doctor::new();
    doctor.clean_default_config();
    doctor.file(".taskrc", "include missing.rc\n");

    let run = doctor.run(&[]);

    let check = run.check("Taskrc");
    assert!(check.starts_with("[fail]"), "{check}");
    assert!(
        check.contains("cannot find include 'missing.rc'"),
        "{check}"
    );
    assert!(!run.success, "exited zero:\n{}", run.stdout);
}

const CLIENT_ID: &str = "8bd7ed3a-2f1b-4fa0-8e4e-6ad2c1e2cd2f";

/// A taskrc syncing with the sync server at `url`.
fn sync_server_taskrc(doctor: &Doctor, url: &str) {
    doctor.clean_default_config();
    doctor.file(
        ".taskrc",
        &format!(
            "sync.server.url={url}\nsync.server.client_id={CLIENT_ID}\n\
             sync.encryption_secret=hunter2\n"
        ),
    );
}

#[test]
fn without_sync_flag_the_server_is_not_contacted() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let doctor = Doctor::new();
    sync_server_taskrc(&doctor, &url);

    let run = doctor.run(&[]);

    let settings = run.check("Sync settings");
    assert!(settings.starts_with("[pass]"), "{settings}");
    assert!(settings.contains(&url), "{settings}");
    assert!(!settings.contains("hunter2"), "secret printed:\n{settings}");
    let server = run.check("Sync server");
    assert!(server.starts_with("[skip]"), "{server}");
    assert!(server.contains("--sync"), "{server}");
    assert!(run.success, "exited non-zero:\n{}", run.stdout);
    assert_eq!(
        listener.accept().map(|_| ()).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "doctor connected to the sync server without --sync"
    );
}

#[test]
fn with_sync_flag_an_unreachable_server_fails() {
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let url = format!("http://{closed}");
    let doctor = Doctor::new();
    sync_server_taskrc(&doctor, &url);

    let run = doctor.run(&["--sync"]);

    let server = run.check("Sync server");
    assert!(server.starts_with("[fail]"), "{server}");
    assert!(server.contains(&url), "{server}");
    assert!(!run.success, "exited zero:\n{}", run.stdout);
}

#[test]
fn invalid_sync_settings_fail() {
    let doctor = Doctor::new();
    doctor.clean_default_config();
    doctor.file(
        ".taskrc",
        "sync.server.url=http://127.0.0.1:1\nsync.server.client_id=not-a-uuid\n\
         sync.encryption_secret=hunter2\n",
    );

    let run = doctor.run(&[]);

    let check = run.check("Sync settings");
    assert!(check.starts_with("[fail]"), "{check}");
    assert!(check.contains("client_id must be a UUID"), "{check}");
}

#[cfg(unix)]
#[test]
fn taskrc_in_an_unsearchable_directory_fails() {
    use std::os::unix::fs::PermissionsExt;

    let doctor = Doctor::new();
    doctor.clean_default_config();
    let taskrc = doctor.file("locked/taskrc", "");
    let locked = taskrc.parent().unwrap().to_path_buf();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let doctor = doctor.env("TASKRC", &taskrc);

    let run = doctor.run(&[]);

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let check = run.check("Taskrc");
    assert!(check.starts_with("[fail]"), "{check}");
    assert!(!run.success, "exited zero:\n{}", run.stdout);
}

/// An HTTP server answering every request with a 404 whose body is `body`,
/// as taskchampion-sync-server does when it has no version to give.
struct NotFoundServer {
    url: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    requests: std::thread::JoinHandle<Vec<String>>,
}

impl NotFoundServer {
    fn start(body: &'static str) -> Self {
        use std::io::{BufRead, BufReader, ErrorKind, Write};
        use std::sync::atomic::Ordering;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let requests = std::thread::spawn(move || {
            let mut requests = Vec::new();
            loop {
                let stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        if stopped.load(Ordering::SeqCst) {
                            return requests;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    Err(err) => panic!("accept: {err}"),
                };
                stream.set_nonblocking(false).unwrap();
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                let response = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                reader.get_mut().write_all(response.as_bytes()).unwrap();
                requests.push(request_line);
            }
        });
        NotFoundServer {
            url,
            stop,
            requests,
        }
    }

    /// The request line of every request the server answered.
    fn finish(self) -> Vec<String> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.requests.join().unwrap()
    }
}

/// `doctor --sync` against a server answering 404 with `body`: the run and
/// its Sync server check.
fn sync_against_not_found(body: &'static str) -> (Run, String) {
    let server = NotFoundServer::start(body);
    let doctor = Doctor::new();
    sync_server_taskrc(&doctor, &server.url);

    let run = doctor.run(&["--sync"]);

    let requests = server.finish();
    assert!(
        !requests.is_empty()
            && requests
                .iter()
                .all(|r| r.starts_with("GET /v1/client/get-child-version/")),
        "requests: {requests:?}"
    );
    let check = run.check("Sync server");
    (run, check)
}

#[test]
fn with_sync_flag_a_server_answering_not_found_is_a_warning() {
    let (run, check) = sync_against_not_found("");

    assert!(check.starts_with("[warn]"), "{check}");
    assert!(check.contains("cannot confirm"), "{check}");
    assert!(run.success, "exited non-zero:\n{}", run.stdout);
}

#[test]
fn with_sync_flag_an_unknown_client_is_named() {
    let (run, check) = sync_against_not_found("no such client");

    assert!(check.starts_with("[warn]"), "{check}");
    assert!(
        check.contains(&format!("does not know client {CLIENT_ID}")),
        "{check}"
    );
    assert!(check.contains("sync.server.client_id is wrong"), "{check}");
    assert!(run.success, "exited non-zero:\n{}", run.stdout);
}

#[test]
fn with_sync_flag_a_known_client_without_history_is_named() {
    let (run, check) = sync_against_not_found("no such version");

    assert!(check.starts_with("[warn]"), "{check}");
    assert!(
        check.contains("knows this client but holds no history"),
        "{check}"
    );
    assert!(run.success, "exited non-zero:\n{}", run.stdout);
}
