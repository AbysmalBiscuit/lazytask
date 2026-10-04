# LazyTask Configuration

LazyTask is highly configurable through TOML configuration files. The main configuration is read from `~/.config/lazytask/config.toml`. The file is optional, and only `lazytask schema init` writes it: without one, LazyTask runs on built-in defaults. A config file may set any subset of keys; every key it leaves out keeps its default. [`config/default.toml`](../config/default.toml) lists the defaults.

## Configuration File Locations

LazyTask looks for configuration in the following locations (in order of precedence):

1. `--config` command line argument
2. `$XDG_CONFIG_HOME/lazytask/config.toml`
3. `~/.config/lazytask/config.toml`
4. Built-in defaults

## Editor Support

lazytask publishes a JSON schema for its config file. Editors running the TOML language server [taplo](https://taplo.tamasfe.dev) (for example VS Code with Even Better TOML) use it to complete keys, show each key's description and default on hover, and flag values of the wrong type.

Point your config at the schema with:

```bash
lazytask schema init
```

This adds a `#:schema` header to the top of the config file. When the file does not exist yet, it writes a starter with every key commented out at its default instead. A file that already has a `#:schema` header is left unchanged, so running it again does nothing. It updates the file named by `lazytask schema init <path>`, else the `--config` file, else the default location.

The header points at the schema attached to the latest release. `lazytask schema` prints the schema of the version you run.

## Main Configuration File

### Theme Configuration

```toml
[theme]
name = "catppuccin-mocha"  # The only built-in theme

[theme.colors]
# Override any role; the others keep the theme's color
primary = "#89b4fa"
error = "red"
```

Every color in the UI comes from one of these roles:

- `background` - Screen and dialog background
- `foreground` - Plain text
- `muted` - De-emphasized text, such as dates and the UUID, and unfocused borders
- `primary` - Titles, field labels and borders
- `secondary` - Tags and waiting tasks
- `accent` - Key hints, column headers and the focused panel or field
- `info` - Active and recurring tasks, sync in progress
- `success` - Completed tasks and confirmations
- `warning` - Pending tasks, things due soon, and footer warnings
- `error` - Overdue tasks, deletions and failures
- `selection` - Background of the selected task row and calendar day
- `priority_high`, `priority_medium`, `priority_low` - Task priority

[`config/default.toml`](../config/default.toml) lists each role's `catppuccin-mocha` color. A color is a hex `#rrggbb`, a terminal color name such as `red`, `light-blue` or `dark-gray`, an ANSI color index from `0` to `255`, or `reset` for the terminal's own color.

When `NO_COLOR` is set to a non-empty value, LazyTask draws with the terminal's default colors only and marks the selected row with reverse video. Setting `selection = "reset"` also marks the selected row and calendar day with reverse video.

### UI Configuration

```toml
[ui]
default_view = "task_list"           # Initial view: task_list, calendar, reports
show_help_bar = true                 # Show keybinding hints at bottom
task_list_columns = [                # Columns to show in task list, in this order
    "id",
    "project",
    "priority",
    "due",
    "description"
]
```

`default_view` falls back to `task_list` when the name is not one of the views above. With `show_help_bar = false` the footer still appears while it shows a status message or warning.

LazyTask watches the task database and reloads whenever it changes, so tasks changed outside it, for example with `task add` or a sync, show up without a restart. The selected task stays selected across reloads. Where the system offers no file change events, LazyTask checks the database for changes every second instead. The `refresh` key (`F5` by default) reloads right away.

Column names in `task_list_columns` ignore case. A column named twice shows once, where it is first listed. When no listed name is a known column, including an empty list, the task list shows the default columns and the footer says so.

Available columns:

- `id` - Task ID, the same one `task` shows; blank once `task` stops numbering the task, as it does for completed tasks
- `uuid` - Task UUID (shortened)
- `project` - Project name
- `priority` - Priority (High/Medium/Low)
- `due` - Due date
- `description` - Task description
- `tags` - Task tags
- `urgency` - Urgency, computed as Taskwarrior 3 does with its default coefficients
- `entry` - Creation date
- `modified` - Last modified date
- `status` - Task status

### Sync Configuration

```toml
[sync]
auto_sync_interval = 5               # Seconds between automatic syncs; 0 turns it off
```

Automatic sync runs only once sync is configured, from the taskrc or the sync config modal. It syncs quietly in the background and reports failures in the footer. The `sync` and `force_sync` keys (`s` by default) sync right away and restart the countdown.

### Taskwarrior Integration

```toml
[taskwarrior]
taskrc_path = "/path/to/.taskrc"     # Empty string or absent means unset: found as Taskwarrior finds it, see Taskrc
data_location = "~/path/to/data"     # Empty string or absent means unset: TASKDATA, then the taskrc, then ~/.task
```

### Taskrc

lazytask reads Taskwarrior's taskrc at startup, so it stores and syncs tasks where `task` does. It writes to the taskrc only when you save the sync config modal. A missing taskrc is not an error.

It reads the first of these:

1. `[taskwarrior] taskrc_path` in the lazytask config
2. The `TASKRC` environment variable
3. `~/.taskrc`, if it exists
4. `$XDG_CONFIG_HOME/task/taskrc`, or `~/.config/task/taskrc` when `XDG_CONFIG_HOME` is unset or empty, if it exists
5. `~/.taskrc`, where saving the sync config modal creates it

The file follows taskrc(5): `key=value` lines, `#` comments, and `include <file>`. A later assignment overrides an earlier one. Any other line stops startup with an error naming its file and line number.

As in Taskwarrior, values and include paths expand a leading `~` to the home directory and `$NAME` to that environment variable, or to nothing when it is unset. A relative include is looked up in this order:

1. The working directory lazytask was started from
2. The directory of the including file, after following symlinks
3. The package rc directories that hold Taskwarrior's themes and holiday files: `/usr/share/taskwarrior`, `/usr/share/doc/task/rc`, `/usr/local/share/doc/task/rc` and `/opt/homebrew/share/doc/task/rc`

So `include dark-16.theme` finds the packaged theme.

lazytask uses these keys:

- `data.location`: the data directory, when neither `[taskwarrior] data_location` nor `TASKDATA` is set
- The sync keys from task-sync(5), which pick one sync target

As in Taskwarrior's `task sync`, the first target whose key is set wins:

1. `sync.local.server_dir`: a local directory
2. `sync.aws.bucket`: an Amazon S3 bucket. Needs `sync.aws.region`, `sync.encryption_secret`, and exactly one way to get credentials: `sync.aws.profile`, the pair `sync.aws.access_key_id` and `sync.aws.secret_access_key`, or `sync.aws.default_credentials` (any value turns it on)
3. `sync.gcp.bucket`: a Google Cloud Storage bucket. Needs `sync.encryption_secret`; `sync.gcp.credential_path` names a service-account key, otherwise Application Default Credentials are used
4. `sync.server.url`, or its deprecated synonym `sync.server.origin` (`sync.server.url` wins): a TaskChampion sync server. Needs `sync.server.client_id` (a UUID) and `sync.encryption_secret`

When a target is set, sync works from launch without the sync config modal. If the target is missing a key it needs, sync stays unconfigured and lazytask shows a warning at startup. The sync config modal sets up a sync server or a local directory; cloud buckets are configured through the taskrc.

#### Saving the sync config modal

The sync config modal (`Shift+S`) has four fields, each standing for one taskrc key:

| Field | Key |
|-------|-----|
| Server URL | `sync.server.url`, filled from `sync.server.origin` when only that is set |
| Client ID | `sync.server.client_id` |
| Encryption Secret | `sync.encryption_secret` |
| Local server dir | `sync.local.server_dir` |

It opens filled in with the taskrc's values, as far as they are set, even when they are incomplete or another target wins. Saving it writes them to the taskrc lazytask read at startup, so `task sync` and the next launch use them. A filled field sets its key. An emptied field clears a key that is set by writing `key=`, which Taskwarrior and lazytask read as unset.

With the local server dir filled in, sync goes to that directory and the server fields are optional. With it empty, the server URL, client ID and secret are required, and sync goes to the server.

How a save edits the taskrc:


- A key already assigned is changed on the line of the assignment in effect, in whichever file holds it, included files too. The line keeps its indentation and any comment after the value. A `sync.server.origin` line, when it is the only server URL, is rewritten as `sync.server.url`.
- A key assigned nowhere is appended to the main taskrc, using its line ending (`\n` or `\r\n`). With no taskrc at that path, one is created holding only the sync keys, readable only by you (mode 0600).
- A key whose value did not change is left as written, so a value given as `$NAME` or `~/...` keeps that form.
- Every other line, comment and include stays as it was.
- The file that gets `sync.encryption_secret` is set to mode 0600 first when other users can read it.

lazytask then syncs to the target the saved taskrc selects. While the taskrc also sets `sync.aws.bucket` or `sync.gcp.bucket`, that bucket still wins over the server, as it does for `task sync`, and the footer says so until you remove it.

A save is refused, and nothing is written, when:

- The local server dir is empty and a server field is empty, or the client ID is not a UUID
- A value would not read back as typed: a taskrc value cannot contain `#` or start or end with a space, and a leading `~` or a `$NAME` in it expands
- `sync.encryption_secret` would go into a file other users can read, and lazytask cannot change its mode, for example because you do not own it. Make the file the message names private with `chmod 600`, then save again

### Keybindings

Keybindings are grouped into `global`, `task_list`, `reports` and `form` sections. Set only the actions you want to move; the rest keep their default keys.

```toml
[keybindings.global]
quit = "Ctrl+q"

[keybindings.task_list]
add_task = "Insert"

[keybindings.form]
confirm = "Ctrl+s"
```

See [keybindings.md](keybindings.md) for complete keybinding reference.

## Environment Variables

LazyTask respects these environment variables:

- `TASKRC` - Path to taskrc file
- `TASKDATA` - Path to task data directory
- `XDG_CONFIG_HOME` - Alternative config directory, also searched for `task/taskrc`
- `NO_COLOR` - Disable colors when set to a non-empty value

## Configuration Examples

### Minimal Configuration

```toml
[theme.colors]
primary = "cyan"

[ui]
show_help_bar = false

[keybindings.global]
quit = "Ctrl+q"
```

### Power User Configuration

```toml
[ui]
default_view = "calendar"
task_list_columns = ["id", "project", "priority", "due", "urgency", "description", "tags"]

[sync]
auto_sync_interval = 60

# Vim-style navigation
[keybindings.task_list]
move_up = "k"
move_down = "j"

[keybindings.reports]
prev_day = "h"
next_day = "l"
prev_week = "k"
next_week = "j"
```

## Validation and Errors

LazyTask refuses to start when the config file has invalid TOML syntax or a value of the wrong type. An editor using the schema (see [Editor Support](#editor-support)) flags wrong types as you type.

Unknown keys do not stop LazyTask. It loads the rest of the file and shows a warning in the footer naming each unknown key by its dotted path, for example `ui.colour` or `taskwarrior.sync_enabled`.

An unknown `default_view` or `task_list_columns` entry also only warns in the footer: the view falls back to `task_list`, and the column is skipped.

Theme problems warn the same way. An unknown `theme.name` uses `catppuccin-mocha`. An unknown role in `theme.colors` is skipped, and a value that is not a color warns naming its key, such as `theme.colors.primary`, and that role keeps the theme's color.

## Upgrading

Running LazyTask never rewrites your config file. Keys you leave out pick up the current built-in defaults, so new settings and changed defaults reach you without editing the file. Keys a newer version no longer reads show up in the unknown-key warning.

## Troubleshooting

### Configuration Not Loading

1. Check file path: `~/.config/lazytask/config.toml`
2. Check syntax and value types with `taplo check config.toml` once the file has a `#:schema` header
3. Check file permissions (should be readable)

### Keybindings Not Working

1. Check for conflicting keybindings
2. Ensure key names are correct (case-sensitive)
3. Some terminals may not support all key combinations
4. Use `F1` to see current active keybindings

### Colors Not Showing

1. Check terminal color support (`echo $TERM`)
2. Check if `NO_COLOR` environment variable is set
3. Hex colors need a terminal with 24-bit color; elsewhere, set the `theme.colors` roles to color names or ANSI indexes
4. Check the footer for a warning naming a `theme` key

### Taskwarrior Integration Issues

1. Verify Taskwarrior is installed and working
2. Check `taskrc_path` and `data_location` settings
3. Test with `task version` command
4. Check file permissions on Taskwarrior data directory

## Best Practices

1. **Start Simple**: Begin with minimal configuration and add as needed
2. **Backup Configs**: Keep backups of working configurations
3. **Test Changes**: Test configuration changes in development
4. **Use Comments**: Document custom settings with comments
5. **Version Control**: Consider versioning your config files
6. **Share Configs**: Share useful configurations with the community

