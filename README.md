# LazyTask

A keyboard-driven terminal UI for task management, written in Rust with [Ratatui](https://ratatui.rs/). LazyTask stores tasks in [TaskChampion](https://github.com/GothenburgBitFactory/taskchampion), the task engine inside Taskwarrior 3.x. It reads and writes the same database Taskwarrior does and syncs with the official [`taskchampion-sync-server`](https://github.com/GothenburgBitFactory/taskchampion-sync-server).

You don't need the `task` binary installed. LazyTask opens the same replica as Taskwarrior (see [Data location](#data-location)), so a task added in either tool shows up in the other.

<img width="1561" height="977" alt="image" src="https://github.com/user-attachments/assets/0441da8f-e2ea-483d-ba4f-2ec61ad75fd9" />
<img width="1561" height="977" alt="image" src="https://github.com/user-attachments/assets/761e174a-fe67-4987-aab4-3d5821b42b73" />

## Features

- Add, edit, complete, soft-delete and purge tasks in an embedded TaskChampion replica.
- A modal form for description, project, priority (H/M/L), due date and tags (`+tag` adds, `-tag` removes).
- An inline filter bar. Toggle the Pending, Active, Overdue, Completed and Deleted statuses, pick any number of projects and tags, and search description, project and tags as free text.
- A reports dashboard with a summary, a 30-day burndown, per-project stats and recent activity.
- A 3-month calendar that marks overdue, pending and completed tasks per day, navigable from the keyboard.
- Sync with any `taskchampion-sync-server` over the standard encrypted protocol, given a server URL, client ID and encryption secret.
- Emoji and CJK descriptions render without panicking.
- Failed operations show up in a red footer panel instead of disappearing into stderr.

## Requirements

- Rust 1.90 or newer, the MSRV declared in `Cargo.toml`.
- A terminal that renders Unicode.
- For sync across devices over HTTP, a running [`taskchampion-sync-server`](https://github.com/GothenburgBitFactory/taskchampion-sync-server). Local use needs nothing extra.

LazyTask can also sync through an Amazon S3 or Google Cloud Storage bucket, set up with Taskwarrior's `sync.aws.*` or `sync.gcp.*` keys in your taskrc. See [Configuration](docs/configuration.md#taskrc).

## Installation

### Installer scripts

This is the easiest route. Each [GitHub Release](https://github.com/AbysmalBiscuit/lazytask/releases) ships installers that download the binary for your platform into `$CARGO_HOME/bin` (`~/.cargo/bin` by default).

Linux and macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/AbysmalBiscuit/lazytask/releases/latest/download/lazytask-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/AbysmalBiscuit/lazytask/releases/latest/download/lazytask-installer.ps1 | iex"
```

### Pre-built archives

Each release also carries one archive per supported platform. An archive holds the `lazytask` binary, `LICENSE` and `README.md`, with a matching `.sha256` file next to it. [docs/releasing.md](docs/releasing.md#targets) lists the platforms and target triples.

### From source

```bash
git clone https://github.com/AbysmalBiscuit/lazytask
cd lazytask
cargo build --release
./target/release/lazytask
```

The build writes the binary to `target/release/lazytask`. On first run it opens Taskwarrior's TaskChampion database, or creates one if none exists (see [Data location](#data-location)).

## Usage

```bash
# Run from the source tree
cargo run --release

# Or run the built binary
./target/release/lazytask

# CLI options
lazytask --help
lazytask --config /custom/path/config.toml

# Check the config, taskrc, data directory and sync settings
lazytask doctor
lazytask doctor --sync   # also contacts the sync server
```

`lazytask doctor` prints each setting with its resolved value, where that value came from, and a `pass`, `warn` or `fail` status. It changes nothing and makes no network calls unless given `--sync`. It exits non-zero when any check fails; warnings alone exit zero.

### Application modes

| Mode | Trigger | Description |
|---|---|---|
| Task list | default | Browse, select and act on tasks |
| Form | `a` (add) / `e` (edit) | Create or edit a task |
| Filter | `/` | Filter by status, project, tag or search text |
| Reports dashboard | `r` | Summary, burndown, project stats, activity |
| Reports calendar | `r` then `c` | 3-month view with daily task markers |
| Sync config | `Shift+S` | Enter server URL, client ID and secret |
| Help | `F1` | Keyboard reference |

## Keyboard interface

These are the default keys. You can rebind any of them in `[keybindings]`; see [docs/keybindings.md](docs/keybindings.md).

### Global

| Key | Action |
|---|---|
| `q` | Quit |
| `Ctrl+C` | Quit |
| `F1` | Toggle help overlay |
| `F5` | Reload tasks from the local replica |
| `Esc` | Cancel, go back or close a modal |
| `Enter` | Confirm or save |

### Task list

| Key | Action |
|---|---|
| `Up` / `Down` | Move selection |
| `a` | Open the add-task form |
| `e` | Open the edit-task form, prefilled |
| `d` | Mark the selected task done (status becomes Completed) |
| `Delete` | Soft-delete the selected task (status becomes Deleted) |
| `/` | Toggle filter mode |
| `r` | Open reports |
| `s` | Sync, or show a hint if sync isn't configured |
| `Shift+S` | Open the sync config modal |

### Filter mode (after `/`)

| Key | Action |
|---|---|
| `Tab` | Cycle sections: Status, Project, Tags, Search |
| `Up` / `Down` | Move within the current section |
| `Space` | Toggle the highlighted item |
| typing | Edit the search text (Search section only) |
| `Backspace` | Erase a search character |
| `Esc` | Leave filter mode, keeping the selections |

The Status section toggles each flag on its own: Pending, Active (started tasks), Overdue (past due and still pending), Completed and Deleted. With nothing selected, every task shows.

### Calendar (after `r`, then `c`)

| Key | Action |
|---|---|
| `Left` / `Right` | Move one day |
| `Up` / `Down` | Move one week |
| `<` / `>` | Previous or next month |
| `t` | Jump to today |
| `c` | Back to the dashboard |
| `Esc` | Back to the task list |

### Form mode

| Key | Action |
|---|---|
| `Tab` / `Down` | Next field |
| `Up` / `Shift+Tab` | Previous field |
| `Left` / `Right` | Move the cursor in the current field |
| typing | Edit the field (description, project, tags, due) |
| `Backspace` | Erase a character |
| `Enter` | Commit the field, then save the form |
| `Esc` | Cancel without saving |

The Tags field takes Taskwarrior syntax: `+work +urgent` adds tags and `-old` removes one. Saving with the Tags field empty clears all user tags.

## Sync

LazyTask uses the official TaskChampion sync protocol, so you need a `taskchampion-sync-server` it can reach over HTTP or HTTPS.

### Quick start with Compose

The repo ships a `compose.yaml` that works with `docker compose`, `podman compose` and `podman-compose`:

```bash
# 1. Generate a client UUID and put it in .env
echo "CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')" > .env

# 2. Start the server
docker compose up -d            # or: podman compose up -d

# 3. Check that it's listening
docker compose logs sync-server | tail
```

The server now listens on `http://localhost:8810`. Enter the UUID from `.env` as the Client ID in LazyTask's sync config modal (see below).

To stop it:

```bash
docker compose down              # keep the database
docker compose down --volumes    # also wipe the database
```

[docs/sync-server.md](docs/sync-server.md) covers custom ports, log levels, syncing several devices and the plain `docker run` form.

### Run the sync server without Compose

One command does the same job:

```bash
CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')   # macOS BSD uuidgen emits uppercase

podman run -d --name=lazytask-sync -p 8810:8080 \
  -e CLIENT_ID=$CLIENT_ID \
  ghcr.io/gothenburgbitfactory/taskchampion-sync-server:0.7.1
# docker takes the same flags.

echo "Use this client_id in LazyTask: $CLIENT_ID"
```

`-p 8810:8080` maps the container port to host port 8810. Any free host port works.

The image's entrypoint reads `CLIENT_ID` from the environment. Passing it as a flag like `--allow-client-id <uuid>` fails with `eval: --allow-client-id: not found`.

### Configure LazyTask

1. Launch LazyTask: `./target/release/lazytask`
2. Press `Shift+S` to open the sync config modal.
3. Fill in:
   - Server URL: `http://localhost:8810`, or your remote URL
   - Client ID: the UUID in `$CLIENT_ID`
   - Encryption secret: any string, as long as every replica uses the same one
4. Press `Enter` to save and `Esc` to close the modal.
5. Press `s` to sync.

The modal keeps its settings for the current session only. To keep them, put them in your taskrc. LazyTask reads sync settings from it at startup, and so does `task`:

```ini
sync.server.url=http://localhost:8810
sync.server.client_id=<CLIENT_ID>
sync.encryption_secret=<secret>
```

[Configuration](docs/configuration.md#taskrc) lists the taskrc keys LazyTask reads.

## Architecture

```
src/
├── main.rs              # CLI entry, calls into the lib crate
├── lib.rs               # Library root (re-exports modules)
├── app.rs               # Event loop, terminal setup, App struct
├── config.rs            # TOML config loading
├── doctor.rs            # `lazytask doctor` checks and report
├── taskrc.rs            # Taskwarrior taskrc reader
├── taskchampion.rs      # Data engine, wraps Replica/Operations
├── data/
│   ├── models.rs        # Canonical Task / TaskStatus / Priority types
│   ├── filters.rs       # TaskFilter predicate engine
│   ├── cache.rs         # In-memory task cache (helper)
│   └── export.rs        # JSON / CSV import-export utilities
├── handlers/
│   ├── input.rs         # KeyEvent -> Action mapping
│   ├── sync.rs          # SyncHandler with watch-channel status updates
│   └── watcher.rs       # Reloads when the replica changes on disk
├── ui/
│   ├── app_ui.rs        # Top-level UI dispatcher (AppUI)
│   ├── views/
│   │   ├── main_view.rs        # Task list with inline filter
│   │   ├── reports_view.rs     # Dashboard + Calendar
│   │   └── settings_view.rs    # Placeholder
│   └── components/
│       ├── task_list.rs        # StatefulWidget with TableState cursor
│       ├── task_detail.rs
│       ├── task_form.rs        # Add/edit modal
│       ├── calendar_view.rs    # 3-month calendar
│       ├── report_panel.rs     # Dashboard panels
│       ├── sync_status.rs      # Sync progress overlay
│       └── sync_config.rs      # Sync configuration modal
└── utils/
    ├── formatting.rs    # truncate_chars (UTF-8 safe), date helpers
    ├── helpers.rs
    ├── keybindings.rs
    └── validation.rs
```

`App::run` polls crossterm events. `AppUI::action` maps each `KeyEvent` to an `Action` through the keymap, and `AppUI::handle_action` updates the engine and the view state. The engine, `TaskChampionIntegration`, builds `Operations` and commits them to the replica through TaskChampion's API. LazyTask never shells out to `task` or talks JSON to another process. Every read and write goes to the embedded SQLite replica.

## Configuration

LazyTask reads its config from `~/.config/lazytask/config.toml`. The file is optional, and LazyTask never writes it. Set only the keys you want to change; the rest keep their defaults, and an empty path string counts as unset. Unknown keys still load, and the footer shows a warning naming each one. The defaults:

```toml
[theme]
name = "catppuccin-mocha"

[theme.colors]
background = "#1e1e2e"
foreground = "#cdd6f4"
primary    = "#89b4fa"
secondary  = "#f38ba8"

[ui]
default_view      = "task_list"
show_help_bar     = true
task_list_columns = ["id", "project", "priority", "due", "description"]

[sync]
auto_sync_interval = 5      # seconds; 0 turns automatic sync off

[keybindings.global]
quit    = "q"               # every key is rebindable, see docs/keybindings.md
help    = "F1"
refresh = "F5"
```

The parser accepts the `[theme]` and `[taskwarrior]` sections, but LazyTask doesn't use every field in them yet.

## Data location

LazyTask shares Taskwarrior's TaskChampion database. It picks the data directory from the first of these that is set:

1. `[taskwarrior] data_location` in the LazyTask config
2. The `TASKDATA` environment variable
3. `data.location` in your taskrc
4. `~/.task`, Taskwarrior's default

A leading `~` in the path expands to your home directory.

## Development

```bash
# Dev build
cargo build

# Release build
cargo build --release

# Run from source
cargo run

# Format / lint
cargo fmt
cargo clippy
```

The code uses Rust 2021, `anyhow` for application errors, `tokio` for the async event loop (single-threaded, with `block_in_place` around sync I/O), and `serde` with `toml` for configuration.

## Testing

```bash
# Run the full suite. The remote-sync tests skip themselves
# when LAZYTASK_REMOTE_SYNC_URL is unset.
cargo test

# To run the remote-sync tests, point them at a live server:
CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')
podman run -d --name=lazytask-sync-test -p 8810:8080 -e CLIENT_ID=$CLIENT_ID \
  ghcr.io/gothenburgbitfactory/taskchampion-sync-server:0.7.1
LAZYTASK_REMOTE_SYNC_URL=http://localhost:8810 \
LAZYTASK_REMOTE_CLIENT_ID=$CLIENT_ID \
cargo test
podman rm -f lazytask-sync-test
```

| Suite | Scope |
|---|---|
| `tests/unit/config.rs` | Config loading and data-location resolution |
| `tests/config_schema.rs` | The committed `schema/lazytask-config.json` matches `lazytask schema` |
| `tests/doctor.rs` | `lazytask doctor` findings, sources and exit codes, run as the binary |
| `tests/unit/filters.rs` | `TaskFilter` predicate matrix |
| `tests/unit/models.rs` | `Task` JSON parsing (RFC3339, Taskwarrior-compact, Unix timestamps) |
| `tests/integration_engine.rs` | CRUD on a real TaskChampion replica in a temp dir |
| `tests/integration_sync.rs` | Two replicas synced through a file-based `LocalServer` |
| `tests/integration_remote_sync.rs` | A live `taskchampion-sync-server` over HTTP (opt-in) |
| `tests/integration_taskrc.rs` | Startup reading the taskrc, `TASKDATA` and home directory |
| `tests/integration_app.rs` | The real event loop: key input, file-watcher reloads, auto-sync |
| `tests/integration_ui.rs` | Render snapshots on `TestBackend`, including Unicode and small terminals |
| `tests/integration_tui_input.rs` | Every shortcut, driven by synthetic key events |

## Known limitations

- Sync settings entered in the modal last only for the session. Put them in your taskrc to keep them (see [Configure LazyTask](#configure-lazytask)).
- Force sync has no default key. Bind `force_sync` in `[keybindings.global]` to use it; see [docs/keybindings.md](docs/keybindings.md).
- The Settings view is a placeholder that renders "Coming Soon".
- Soft-deleted tasks pile up. A `purge_task` API exists, but no key triggers it, and LazyTask doesn't call TaskChampion's 180-day expiry.
- The theme config is ignored. LazyTask draws with the terminal's named colors.

## Releasing

Maintainers, see [docs/releasing.md](docs/releasing.md). [release-please](https://github.com/googleapis/release-please) keeps a release PR open on `main`. Merging it tags the version, and [cargo-dist](https://github.com/axodotdev/cargo-dist) attaches the archives and installers to the GitHub Release.

## Contributing

Issues and pull requests are welcome. [Known limitations](#known-limitations) lists the gaps most worth closing.

## License

MIT. See [LICENSE](LICENSE).

## Acknowledgments

- [TaskChampion](https://github.com/GothenburgBitFactory/taskchampion), the task engine under both Taskwarrior 3.x and LazyTask.
- [Taskwarrior](https://taskwarrior.org/), the original CLI task manager.
- [Ratatui](https://ratatui.rs/), the Rust TUI framework.
- [Lazygit](https://github.com/jesseduffield/lazygit), which inspired the keyboard-driven feel.
- [Yazi](https://yazi-rs.github.io/), a reference for modern TUI design.
