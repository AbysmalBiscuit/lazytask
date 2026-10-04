# LazyTask Keybindings

Every key LazyTask responds to is an action you can rebind in the `[keybindings]` sections of `~/.config/lazytask/config.toml`. A config file only needs the actions it changes; every other action keeps its default key. Press the `help` key, `F1` by default, to see the keys in effect.

```toml
[keybindings.global]
quit = "Ctrl+q"

[keybindings.task_list]
add_task = "Insert"
```

## Key syntax

A key string is an optional chain of modifiers joined by `+`, followed by one key.

- Characters: any single character, such as `a`, `S`, `/`, `<` or `1`. Case matters: `S` is Shift+s.
- Named keys: `Enter`, `Esc`, `Tab`, `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, `Right`, `Space`.
- Function keys: `F1` through `F12`.
- Modifiers: `Ctrl+`, `Alt+` and `Shift+`, in any order, such as `Ctrl+s`, `Alt+Enter` or `Ctrl+Shift+x`.
- `Shift+` on a letter is the same as its capital: `Shift+s` and `S` are one key. `Shift+Tab` is the back-tab key.
- The `+` key itself is written `+`, or `Ctrl++` with a modifier.

Key names and modifiers ignore case: `ctrl+PAGEUP` works. Single characters do not.

## Sections

Bindings are grouped by where they apply. The task list uses `task_list` and `global` bindings, the reports view uses `reports` and `global`, and other views such as help use `global` alone. While a form is open, such as the add/edit task form, the filter panel or sync setup, only `form` bindings apply, and any other printable key types itself.

### `[keybindings.global]`

Active in every view outside a form.

| Action        | Default  | Description                     |
| ------------- | -------- | ------------------------------- |
| `quit`        | `q`      | Quit                            |
| `force_quit`  | `Ctrl+c` | Quit                            |
| `help`        | `F1`     | Show the help overlay           |
| `refresh`     | `F5`     | Reload tasks from the replica   |
| `back`        | `Esc`    | Return to the task list         |
| `reports`     | `r`      | Open the reports view           |
| `sync`        | `s`      | Sync, once sync is configured   |
| `force_sync`  | none     | Force a full sync               |
| `sync_config` | `S`      | Open the sync setup form        |

### `[keybindings.task_list]`

| Action        | Default  | Description                |
| ------------- | -------- | -------------------------- |
| `move_up`     | `Up`     | Select the previous task   |
| `move_down`   | `Down`   | Select the next task       |
| `add_task`    | `a`      | Add a task                 |
| `edit_task`   | `e`      | Edit the selected task     |
| `done_task`   | `d`      | Mark the selected task done |
| `delete_task` | `Delete` | Delete the selected task   |
| `filter`      | `/`      | Open the filter panel      |

### `[keybindings.reports]`

The day, week, month and today actions move the calendar, so they only act in calendar mode.

| Action            | Default | Description                         |
| ----------------- | ------- | ----------------------------------- |
| `toggle_calendar` | `c`     | Switch between calendar and dashboard |
| `prev_day`        | `Left`  | Previous day                        |
| `next_day`        | `Right` | Next day                            |
| `prev_week`       | `Up`    | Previous week                       |
| `next_week`       | `Down`  | Next week                           |
| `prev_month`      | `<`     | Previous month                      |
| `next_month`      | `>`     | Next month                          |
| `today`           | `t`     | Jump to today                       |

### `[keybindings.form]`

| Action       | Default     | Description                                   |
| ------------ | ----------- | --------------------------------------------- |
| `next_field` | `Tab`       | Next field, or next filter section            |
| `prev_field` | `Shift+Tab` | Previous field                                |
| `move_up`    | `Up`        | Previous field, or previous filter item       |
| `move_down`  | `Down`      | Next field, or next filter item               |
| `move_left`  | `Left`      | Move the cursor left                          |
| `move_right` | `Right`     | Move the cursor right                         |
| `toggle`     | `Space`     | Toggle the filter item, or type a space       |
| `erase`      | `Backspace` | Erase a character                             |
| `confirm`    | `Enter`     | Commit the field, then save                   |
| `cancel`     | `Esc`       | Cancel and close the form                     |

## Conflicts and mistakes

LazyTask warns at startup, naming the entry by its dotted path such as `keybindings.global.quit`, when an entry names an action it does not have, when a key string does not parse, or when two entries in one section bind the same key. The action in a rejected entry keeps its default key.

A key you configure takes over every default on that key in the views where its section applies, including defaults from other sections. With `quit = "d"` in `[keybindings.global]` and nothing else, `d` quits everywhere, including the task list, and `done_task` has no key until you give it one. With `add_task = "q"` in `[keybindings.task_list]`, `q` adds a task in the task list and still quits in the other views.

When two configured keys apply in the same view, the view's own section wins over `global`. Among defaults, the same rule holds, though the defaults never overlap. The help overlay lists only keys that do something in at least one view.
