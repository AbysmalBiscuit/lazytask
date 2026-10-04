# LazyTask — Modern Terminal UI for TaskChampion

A keyboard-driven Terminal User Interface for personal task management, built in Rust with [Ratatui](https://ratatui.rs/). LazyTask uses [TaskChampion](https://github.com/GothenburgBitFactory/taskchampion) — the same embeddable task engine that powers Taskwarrior 3.x — as its data layer, so it can sync with the official [`taskchampion-sync-server`](https://github.com/GothenburgBitFactory/taskchampion-sync-server) and stays storage-compatible with Taskwarrior.

LazyTask is **standalone**: you do **not** need the `task` binary installed. It opens the same replica as Taskwarrior (see [Data location](#data-location)), so tasks added in either tool show up in the other.

<img width="1561" height="977" alt="image" src="https://github.com/user-attachments/assets/0441da8f-e2ea-483d-ba4f-2ec61ad75fd9" />
<img width="1561" height="977" alt="image" src="https://github.com/user-attachments/assets/761e174a-fe67-4987-aab4-3d5821b42b73" />

## Features

- **Full task CRUD** against an embedded TaskChampion replica: add / edit / done / soft-delete / purge.
- **Modal task form** with description, project, priority (H/M/L), due date, and tags (`+tag` to add, `-tag` to remove).
- **Inline multi-select filter bar** — toggle Pending / Active / Overdue / Completed / Deleted statuses, multi-select projects and tags, free-text search across description / project / tags.
- **Reports dashboard** with four panels: Summary, 30-day Burndown, Project Analytics, Recent Activity.
- **3-month calendar view** with overdue / pending / completed indicators per day and full keyboard navigation.
- **Sync** with any `taskchampion-sync-server` instance via the standard encrypted protocol (server URL + client ID + encryption secret).
- **UTF-8-safe rendering**: emoji and CJK descriptions never panic the renderer.
- **Visible error feedback**: failed operations surface in a red footer panel, not silent stderr.
- **66 tests** across unit, integration, and live-server suites — including end-to-end TUI input tests driven by synthetic key events.

## Requirements

- **Rust 1.90+** (the declared MSRV in `Cargo.toml`; uses 2021 edition).
- A **Unicode-capable terminal**.
- *(Optional)* A running [`taskchampion-sync-server`](https://github.com/GothenburgBitFactory/taskchampion-sync-server) instance if you want cross-device sync over HTTP. Local-only use needs nothing extra.

> **Cloud sync (S3 / GCS).** lazytask can also sync through an Amazon S3 or Google Cloud Storage bucket, configured with Taskwarrior's `sync.aws.*` or `sync.gcp.*` keys in your taskrc. See [Configuration](docs/configuration.md#taskrc).

## Installation

### Installer scripts (recommended)

Each [GitHub Release](https://github.com/AbysmalBiscuit/lazytask/releases) ships installers that download the right binary for your platform and put `lazytask` in `$CARGO_HOME/bin` (`~/.cargo/bin` by default).

Linux and macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/AbysmalBiscuit/lazytask/releases/latest/download/lazytask-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/AbysmalBiscuit/lazytask/releases/latest/download/lazytask-installer.ps1 | iex"
```

### Pre-built archives

The same release carries an archive for each supported platform, holding the `lazytask` binary, `LICENSE` and `README.md`, with a matching `.sha256` file. [docs/releasing.md](docs/releasing.md#targets) lists the platforms and target triples.

### From source

```bash
git clone https://github.com/AbysmalBiscuit/lazytask
cd lazytask
cargo build --release
./target/release/lazytask
```

The binary is `target/release/lazytask`. It opens the same TaskChampion database as Taskwarrior, creating it if absent (see [Data location](#data-location)).

## Usage

```bash
# Run from the source tree
cargo run --release

# Or run the built binary
./target/release/lazytask

# CLI options
lazytask --help
lazytask --config /custom/path/config.toml
```

### Application modes

| Mode | Trigger | Description |
|---|---|---|
| Task List | default | Browse, select, and act on tasks |
| Form | `a` (add) / `e` (edit) | Modal for task creation / editing |
| Filter | `/` | Inline multi-select status / project / tag / search |
| Reports — Dashboard | `r` | Summary, burndown, project analytics, activity |
| Reports — Calendar | `r` then `c` | 3-month view with daily task indicators |
| Sync Config | `Shift+S` | Modal for entering server URL / client ID / secret |
| Help | `F1` | Keyboard reference |

## Keyboard interface

These are the default keys. Every one can be rebound through `[keybindings]`; see [docs/keybindings.md](docs/keybindings.md).

### Global

| Key | Action |
|---|---|
| `q` | Quit |
| `Ctrl+C` | Quit |
| `F1` | Toggle help overlay |
| `F5` | Reload tasks from local replica |
| `Esc` | Cancel / go back / close modal |
| `Enter` | Confirm / save |

### Task list

| Key | Action |
|---|---|
| `↑` / `↓` | Move selection |
| `a` | Open the add-task form |
| `e` | Open the edit-task form (prefilled) |
| `d` | Mark selected task done (status → Completed) |
| `Delete` | Soft-delete selected task (status → Deleted) |
| `/` | Toggle filter mode |
| `r` | Open Reports view |
| `s` | Sync (no-op with hint if not yet configured) |
| `Shift+S` | Open Sync Config modal |

### Filter mode (after `/`)

| Key | Action |
|---|---|
| `Tab` | Cycle filter sections: Status → Project → Tags → Search |
| `↑` / `↓` | Move within the current section's option list |
| `Space` | Toggle the highlighted item (multi-select) |
| typing | Filter via free-text search (only in Search section) |
| `Backspace` | Erase a search character |
| `Esc` | Exit filter mode (selections remain applied) |

The Status section toggles five flags independently: **Pending**, **Active** (started tasks), **Overdue** (past due, still pending), **Completed**, **Deleted**. With nothing selected, all tasks pass through.

### Reports → Calendar mode (after `r`, then `c`)

| Key | Action |
|---|---|
| `←` / `→` | Move by one day |
| `↑` / `↓` | Move by one week |
| `<` / `>` | Previous / next month |
| `t` | Jump to today |
| `c` | Toggle back to dashboard |
| `Esc` | Return to task list |

### Form mode

| Key | Action |
|---|---|
| `Tab` / `↓` | Next field |
| `↑` (or Shift+Tab) | Previous field |
| `←` / `→` | Cursor within the current text field |
| typing | Edit the active field (description, project, tags, due) |
| `Backspace` | Erase a character |
| `Enter` | Commit field, then save the form |
| `Esc` | Cancel without saving |

Tags use Taskwarrior-style syntax inside the Tags field: `+work +urgent` to add, `-old` to remove. Submitting the form with the Tags field empty clears all user tags.

## Sync

LazyTask speaks the official TaskChampion sync protocol. To use it you need a `taskchampion-sync-server` reachable over HTTP/HTTPS.

### Quick start with Compose (recommended)

The repo ships a `compose.yaml` that works with both `docker compose` and `podman compose` (or `podman-compose`):

```bash
# 1. Generate a client UUID and put it in .env
echo "CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')" > .env

# 2. Bring the server up
docker compose up -d            # or: podman compose up -d

# 3. Confirm it's listening
docker compose logs sync-server | tail
```

The server is now reachable at `http://localhost:8810`. Use the UUID from your `.env` file as the **Client ID** in LazyTask's sync config modal (see below).

To stop:

```bash
docker compose down              # keep the database
docker compose down --volumes    # also wipe the database
```

See [docs/sync-server.md](docs/sync-server.md) for the full reference (custom ports, log levels, multi-device sync, the no-compose `docker run` form).

### Run the sync server directly (no compose)

If you'd rather not keep `compose.yaml` around, the equivalent one-shot command is:

```bash
CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')   # macOS BSD uuidgen emits uppercase

podman run -d --name=lazytask-sync -p 8810:8080 \
  -e CLIENT_ID=$CLIENT_ID \
  ghcr.io/gothenburgbitfactory/taskchampion-sync-server:0.7.1
# Or replace `podman` with `docker` — same flags.

echo "Use this client_id in LazyTask: $CLIENT_ID"
```

`-p 8810:8080` maps the container port; pick whatever host port is free.

> The image's entrypoint reads `CLIENT_ID` from the environment, **not** as a CLI flag. Don't try `--allow-client-id <uuid>` style invocations — they fail with `eval: --allow-client-id: not found`.

### Configure LazyTask

1. Launch LazyTask: `./target/release/lazytask`
2. Press **`Shift+S`** to open the Sync Config modal.
3. Fill in:
   - **Server URL**: `http://localhost:8810` (or your remote URL)
   - **Client ID**: the UUID from `$CLIENT_ID` above
   - **Encryption Secret**: any string — both replicas must use the same one
4. Press **Enter** to save, **Esc** to close the modal.
5. Press **`s`** to sync.

Saving writes the settings to your taskrc, where `task` reads them too, and the modal opens with them next time. You can also put them in the taskrc yourself; lazytask configures sync from it at startup:

```ini
sync.server.url=http://localhost:8810
sync.server.client_id=<CLIENT_ID>
sync.encryption_secret=<secret>
```

See [Configuration](docs/configuration.md#taskrc) for the keys lazytask reads.

## Architecture

```
src/
├── main.rs              # CLI entry, calls into the lib crate
├── lib.rs               # Library root (re-exports modules)
├── app.rs               # Event loop, terminal setup, App struct
├── config.rs            # TOML config load / save
├── taskchampion.rs      # The data engine — wraps Replica/Operations
├── data/
│   ├── models.rs        # Canonical Task / TaskStatus / Priority types
│   ├── filters.rs       # TaskFilter predicate engine
│   ├── cache.rs         # In-memory task cache (helper)
│   └── export.rs        # JSON / CSV import-export utilities
├── handlers/
│   ├── input.rs         # KeyEvent → Action mapping
│   └── sync.rs          # SyncHandler with watch-channel status updates
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

The data flow is simple: `App::run` polls crossterm events, `AppUI::action` maps each `KeyEvent` to an `Action` through the keymap, and `AppUI::handle_action` mutates the engine and view state. The engine (`TaskChampionIntegration`) builds `Operations` and commits them to the replica via TaskChampion's API. There is **no CLI shim, no JSON IPC, no `task` invocation** — every read and write goes through the embedded SQLite-backed replica.

## Configuration

Config lives at `~/.config/lazytask/config.toml`. The file is optional and lazytask never writes it. Set only the keys you want to change; every other key keeps its built-in default, and an empty path string means unset. Unknown keys load with a warning in the footer naming each one. The defaults are:

```toml
[theme]
name = "catppuccin-mocha"   # the only built-in theme

[theme.colors]              # override any color role, see docs/configuration.md
# primary = "#89b4fa"

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

## Data location

lazytask shares Taskwarrior's TaskChampion database. The data directory resolves in this order, first match wins:

1. `[taskwarrior] data_location` in the lazytask config
2. The `TASKDATA` environment variable
3. `data.location` in your taskrc
4. `~/.task`, Taskwarrior's default

A leading `~` in the chosen path expands to the home directory.

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

The codebase follows Rust 2021 conventions, uses `anyhow` for application errors, `tokio` for the async event loop (single-threaded usage with `block_in_place` for sync I/O), and `serde` + `toml` for configuration.

## Testing

LazyTask has a 66-test suite across 8 files. All pass on every commit:

```bash
# Run the full suite. Of these, 64 always run and the 2 remote-sync tests
# auto-skip cleanly when LAZYTASK_REMOTE_SYNC_URL is unset.
cargo test

# To exercise the live remote-sync tests, point them at a running server:
CLIENT_ID=$(uuidgen | tr 'A-Z' 'a-z')
podman run -d --name=lazytask-sync-test -p 8810:8080 -e CLIENT_ID=$CLIENT_ID \
  ghcr.io/gothenburgbitfactory/taskchampion-sync-server:0.7.1
LAZYTASK_REMOTE_SYNC_URL=http://localhost:8810 \
LAZYTASK_REMOTE_CLIENT_ID=$CLIENT_ID \
cargo test
podman rm -f lazytask-sync-test
```

| Suite | Tests | Scope |
|---|---|---|
| `tests/unit/config.rs` | 7 | Config TOML round-trip |
| `tests/unit/filters.rs` | 9 | `TaskFilter` predicate matrix |
| `tests/unit/models.rs` | 9 | `Task` JSON parsing (RFC3339, Taskwarrior-compact, Unix timestamps) |
| `tests/integration_engine.rs` | 8 | CRUD on a real TaskChampion replica in a temp dir |
| `tests/integration_sync.rs` | 4 | Two replicas synced via `LocalServer` (file-based) |
| `tests/integration_remote_sync.rs` | 2 | Live `taskchampion-sync-server` over HTTP (opt-in) |
| `tests/integration_ui.rs` | 4 | Render snapshots via `TestBackend` (incl. unicode + small terminals) |
| `tests/integration_tui_input.rs` | 23 | End-to-end synthetic-key-event TUI tests for every shortcut |

## Known limitations

- **Sync settings aren't persisted.** They live in memory; re-enter them each session.
- **Force sync has no default key.** Bind `force_sync` in `[keybindings.global]` to use it; see [docs/keybindings.md](docs/keybindings.md).
- **`Settings` view is a placeholder.** It renders "Coming Soon".
- **Soft-deleted tasks accumulate forever.** A `purge_task` API exists but isn't wired to a keystroke. TaskChampion's automatic 180-day expiry isn't called yet.
- **One built-in theme.** `catppuccin-mocha` is the only palette; `[theme.colors]` recolors any role.

## Releasing

Maintainers: see [docs/releasing.md](docs/releasing.md). Releases come from a release PR that [release-please](https://github.com/googleapis/release-please) keeps open on `main`; merging it tags the version, and [cargo-dist](https://github.com/axodotdev/cargo-dist) attaches the archives and installers to the GitHub Release.

## Contributing

Issues and pull requests welcome. The todo list above covers the most useful next steps.

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgments

- [TaskChampion](https://github.com/GothenburgBitFactory/taskchampion) — the embeddable task engine that powers both Taskwarrior 3.x and LazyTask.
- [Taskwarrior](https://taskwarrior.org/) — the original CLI task manager.
- [Ratatui](https://ratatui.rs/) — the Rust TUI framework.
- [Lazygit](https://github.com/jesseduffield/lazygit) — inspiration for the keyboard-driven aesthetic.
- [Yazi](https://yazi-rs.github.io/) — modern TUI design reference.
