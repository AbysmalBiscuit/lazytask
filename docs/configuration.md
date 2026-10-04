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
refresh_interval = 1000              # Auto-refresh interval (milliseconds)
task_list_columns = [                # Columns to show in task list
    "id",
    "project",
    "priority",
    "due",
    "description"
]
```

Available columns:

- `id` - Task ID number
- `uuid` - Task UUID (shortened)
- `project` - Project name
- `priority` - Priority (H/M/L)
- `due` - Due date
- `description` - Task description
- `tags` - Task tags
- `urgency` - Calculated urgency
- `entry` - Creation date
- `modified` - Last modified date
- `status` - Task status

### Taskwarrior Integration

```toml
[taskwarrior]
taskrc_path = "/path/to/.taskrc"     # Empty string or absent means unset (auto-detect)
data_location = "/path/to/data"      # Empty string or absent means unset (auto-detect)
```

### Keybindings

Keybindings are organized by context:

```toml
[keybindings.global]
quit = "q"
help = "F1"
refresh = "F5"
force_quit = "Ctrl+c"

[keybindings.task_list]
add_task = "a"
edit_task = "e"
done_task = "d"
delete_task = "Delete"
# ... more keybindings

[keybindings.task_detail]
save = "Ctrl+s"
cancel = "Esc"
# ... more keybindings
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
refresh_interval = 5000
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

