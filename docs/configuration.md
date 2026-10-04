# LazyTask Configuration

LazyTask is highly configurable through TOML configuration files. The main configuration is read from `~/.config/lazytask/config.toml`. The file is optional and LazyTask never writes it: without one, LazyTask runs on built-in defaults. A config file may set any subset of keys; every key it leaves out keeps its default. [`config/default.toml`](../config/default.toml) lists the defaults.

## Configuration File Locations

LazyTask looks for configuration in the following locations (in order of precedence):

1. `--config` command line argument
2. `$XDG_CONFIG_HOME/lazytask/config.toml`
3. `~/.config/lazytask/config.toml`
4. Built-in defaults

## Main Configuration File

### Theme Configuration

```toml
[theme]
name = "catppuccin-mocha"  # Available: catppuccin-mocha, catppuccin-latte, dracula, gruvbox

[theme.colors]
# Override specific theme colors
background = "#1e1e2e"
foreground = "#cdd6f4"
primary = "#89b4fa"
secondary = "#f38ba8"
success = "#a6e3a1"
warning = "#f9e2af"
error = "#f38ba8"
```

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

LazyTask watches the task database and reloads whenever it changes, so tasks changed outside it, for example with `task add` or a sync, show up without a restart. The selected task stays selected across reloads. Where the system offers no file change events, LazyTask checks the database for changes every second instead. `F5` reloads right away.

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

Automatic sync runs only once sync is configured. It syncs quietly in the background and reports failures in the footer. Pressing `s` syncs right away and restarts the countdown.

### Taskwarrior Integration

```toml
[taskwarrior]
taskrc_path = "/path/to/.taskrc"     # Empty string or absent means unset: TASKRC, then ~/.taskrc
data_location = "~/path/to/data"     # Empty string or absent means unset: TASKDATA, then the taskrc, then ~/.task
```

### Taskrc

lazytask reads Taskwarrior's taskrc at startup, so it stores and syncs tasks where `task` does. It never writes to the taskrc. A missing taskrc is not an error.

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

When a target is set, sync works from launch without the sync config modal. If the target is missing a key it needs, sync stays unconfigured and lazytask shows a warning at startup. The sync config modal only sets up a sync server; cloud buckets are configured through the taskrc.

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
- `XDG_CONFIG_HOME` - Alternative config directory
- `NO_COLOR` - Disable colors when set

## Configuration Examples

### Minimal Configuration

```toml
[theme]
name = "dracula"

[ui]
show_help_bar = false

[keybindings.global]
quit = "Ctrl+q"
```

### Power User Configuration

```toml
[theme]
name = "gruvbox"

[ui]
default_view = "calendar"
task_list_columns = ["id", "project", "priority", "due", "urgency", "description", "tags"]

# Vim-style navigation
[keybindings.task_list]
move_up = "k"
move_down = "j"
move_left = "h"
move_right = "l"
first_task = "gg"
last_task = "G"
```

## Validation and Errors

LazyTask refuses to start when the config file has invalid TOML syntax or a value of the wrong type.

Unknown keys do not stop LazyTask. It loads the rest of the file and shows a warning in the footer naming each unknown key by its dotted path, for example `ui.colour` or `taskwarrior.sync_enabled`.

An unknown `default_view` or `task_list_columns` entry also only warns in the footer: the view falls back to `task_list`, and the column is skipped.

## Upgrading

LazyTask never rewrites your config file. Keys you leave out pick up the current built-in defaults, so new settings and changed defaults reach you without editing the file. Keys a newer version no longer reads show up in the unknown-key warning.

## Troubleshooting

### Configuration Not Loading

1. Check file path: `~/.config/lazytask/config.toml`
2. Verify TOML syntax with `toml-validate config.toml`
3. Check file permissions (should be readable)
4. Run with `--verbose` to see config loading messages

### Keybindings Not Working

1. Check for conflicting keybindings
2. Ensure key names are correct (case-sensitive)
3. Some terminals may not support all key combinations
4. Use `F1` to see current active keybindings

### Colors Not Showing

1. Check terminal color support (`echo $TERM`)
2. Try different theme
3. Check if `NO_COLOR` environment variable is set
4. Some terminals may not support all colors

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

