# Pomolog

A pomodoro timer that also keeps your tasks, calendar and habits, and shows
you where the time went. A native Linux app written in Rust, inspired by
[PomodoroLogger](https://github.com/zxch3n/PomodoroLogger).

- **Timer**: focus, short and long breaks; pause, skip, abandon, extend; an
  optional flow mode that counts up and sizes the break to the time worked.
  Interruptions are logged, with a reason if you give one.
- **Board**: kanban boards with custom lists. Cards have estimates, due
  dates, tags and notes. Time is logged against the card you are working on.
- **Calendar**: month and week views that keep four things apart:
  holidays (tinted days), appointments (timed), deadlines (flagged, can be
  ticked off) and habits (lines that join up across days).
- **Habits**: don't-break-the-chain tracking. Yes/no, quantity, or
  focus-linked habits that tick themselves off from logged focus time.
  Rest days, holidays and a few skips a month keep one bad day from ending
  a long chain.
- **Analytics**: focus per day, by task, tag and board, time of day,
  estimate accuracy, interruption reasons, a year heatmap.
- **Themes**: follows your GTK or Qt colours by default and updates when
  they change. Dark unless the desktop says otherwise.

Everything is stored locally in one SQLite file.

## Install

Try it:

```sh
nix run github:OWNER/pomolog
```

With home-manager:

```nix
{
  inputs.pomolog.url = "github:OWNER/pomolog";

  # in your home-manager configuration
  imports = [ inputs.pomolog.homeManagerModules.default ];
  programs.pomolog = {
    enable = true;
    autostart = true;   # tray icon and CLI available from login
  };
}
```

Or add `inputs.pomolog.packages.${system}.default` to your packages, or use
`inputs.pomolog.overlays.default` and install `pkgs.pomolog`.

## Command line

Without arguments `pomolog` opens the window, or raises it if it is already
running. Closing the window leaves the timer running in the tray.

```sh
pomolog toggle            # start, pause or resume
pomolog skip              # on to the next phase
pomolog abandon           # give up on this focus session
pomolog show calendar     # open the window on a view
pomolog status            # focus 12:34 (running) · Write the report
pomolog status --json     # for bars and scripts
pomolog add "Write the report" --due fri --board work
pomolog quit
```

`add` works whether or not the app is running. Due dates accept
`2026-12-01`, `today`, `tomorrow`, a weekday name, `+3d` and `+2w`.

Hyprland keybinds, for example:

```
bind = SUPER, P, exec, pomolog toggle
bind = SUPER SHIFT, P, exec, pomolog show timer
```

In the app: Space starts and pauses, Ctrl+K opens the command palette,
Ctrl+1 to Ctrl+7 switch views, Esc closes dialogs and zen mode.

## Themes

The default theme, `system`, reads:

1. dark or light from the desktop portal (dark when it has no preference);
2. colours from `~/.config/gtk-4.0/gtk.css` (then `gtk-3.0`), following
   `@import` and `@define-color`, the same names libadwaita uses;
3. or, with `source = "qt"`, from the qt6ct/qt5ct colour scheme or
   `kdeglobals`.

These files are watched, so tools that rewrite them (matugen, for example)
restyle Pomolog as they go.

On top of that you can override any colour, in Settings or in
`~/.config/pomolog/config.toml`:

```toml
[appearance]
theme = "system"      # or a preset: rose-pine, gruvbox, nord, paper
mode = "system"       # system | dark | light
source = "auto"       # auto | gtk | qt
radius = 10
scale = 1.0

[overrides]
accent = "#ff8800"    # focus follows the accent unless set itself
habit = "#7fd49e"
```

### Theme files

A theme is a TOML file in `~/.config/pomolog/themes/`. It can stand alone:

```toml
name = "Mine"
mode = "dark"

[colors]
bg = "#101418"
text = "#e6e6e6"
accent = "#5fd7a7"
```

Only `bg`, `text` and `accent` are needed; the rest is derived. Or it can
adjust the system theme:

```toml
inherit = "system"

[colors]
focus = "#ff8800"
deadline = "#ff5577"
```

Colour names: `bg`, `surface`, `surface2`, `surface3`, `text`, `text_dim`,
`accent`, `on_accent`, `focus`, `short_break`, `long_break`, `holiday`,
`appointment`, `deadline`, `habit`, `success`, `warning`, `danger`,
`outline`. "Save this look as a theme file" in Settings writes the current
palette out in full as a starting point.

## Data

| What | Where |
|---|---|
| Database | `~/.local/share/pomolog/pomolog.db` |
| Settings | `~/.config/pomolog/config.toml` |
| Themes | `~/.config/pomolog/themes/*.toml` |

Settings has export and import of everything as one JSON file, and a
delete-all button. Holiday calendars and other `.ics` files can be imported
from the calendar's day panel.

## Development

```sh
nix develop
cargo test
cargo run

# a throwaway database with two months of sample data
POMOLOG_DATA_DIR=/tmp/pomolog-demo cargo run --example seed
POMOLOG_DATA_DIR=/tmp/pomolog-demo cargo run
```

`POMOLOG_DATA_DIR`, `POMOLOG_CONFIG_DIR` and `POMOLOG_SOCKET` relocate the
app's own files; `POMOLOG_SYSTEM_CONFIG_HOME` points it at a different GTK/Qt
configuration, which is handy for trying a theme.

The logic (timer, habits, recurrence, statistics, theme parsing) lives in
`src/core` and `src/theme` and has no GUI dependency. `src/ui` is the
[iced](https://iced.rs) interface.

## Not there yet

- App usage logging and the distraction score from the original.
- Calendar sync (CalDAV, `.ics` subscriptions by URL) and `.ics` export.
- Do-not-disturb during focus, idle detection.
- Native file pickers: import and export take a typed path.

## Licence

GPL-3.0-or-later.
