# hc-tui

An unofficial terminal UI for [herdr-core](https://github.com/herdrdev/herdr-core) (`hc`): the sidebar,
splits and keys of the classic herdr TUI, on top of the new hc runtime, across every machine `hc` knows,
with predictive local echo for laggy links.

> Not affiliated with the herdr project. Built because the herdr 0.9 terminal UI and the new
> herdr-core runtime don't talk to each other, and I wanted both: herdr's keyboard workflow in a
> terminal, and sessions I can also open from the Herdr desktop and iPhone apps.

```
┌ hc-tui ──────────────────────┐┌──────────────────────────────┬─────────────────────┐
│💻 laptop              this pc││ $ cargo test                 │ ● claude            │
│ api                 1 waiting││ running 9 tests              │ > review the diff   │
│ ▌● tab 1                     ││ test result: ok              │                     │
│   ├ ● review           claude││                              │                     │
│   └ ○ pwsh                   │├──────────────────────────────┴─────────────────────┤
│  ✓ docs             your turn││ PS C:\Users\me\app> npm run dev                    │
│💻 vps               connected││                                                    │
│ shop                         ││                                                    │
│  ▲ checkout         needs you││                                                    │
└──────────────────────────────┘└────────────────────────────────────────────────────┘
 PREFIX  api › tab 1  t5 ~/src/api               2 waiting · ctrl+b a  echo⚡ ctrl+b ? help
```


## What it does

- **Shared layout.** Workspaces, tabs and splits are read from and written to the same layout document the
  Herdr desktop app keeps in each machine's core (`herdr-app`). Split a pane in hc-tui and it is split in
  the desktop and phone apps too; rearrange in the app and hc-tui follows. Unknown fields are preserved,
  writes use the core's version check.
- **herdr keys.** Prefix `ctrl+b`, then the herdr defaults: `c` new tab, `v` / `-` split, `h/j/k/l` move,
  `H/J/K/L` swap, `z` zoom, `x` close, `r` resize, `n/p/1..9` tabs, `N/W/D` workspaces, `w` navigate,
  `g` goto, `b` sidebar, `q` detach. `ctrl+b ?` lists them all.
- **Every machine.** Machines added with `hc machine add` show up in the sidebar with their own workspaces;
  their terminals are attached through your local core.
- **Tree.** Machines (💻) → workspaces → tabs → panes; a tab with splits lists its panes, Enter on one focuses it.
- **Agent status.** Each tab and pane shows its state: `▲` blocked, `●` working, `✓` done, `○` idle.
  The goto picker (`ctrl+b g`) searches everything and filters with `b w i d a`, like herdr's.
- **Waiting for you.** An agent that finished its turn, got blocked or failed while you were looking elsewhere
  is highlighted (`your turn`, `needs you`), counted on its workspace and in the status line;
  `ctrl+b a` jumps to the next one. Focusing the pane clears it.
- **Predictive echo.** Printable keys show up immediately (underlined) and are reconciled with the
  program's real echo, mosh-style. Toggle with `ctrl+b e`, or start with `--no-predict`.
- **Mouse.** Click a tab or pane in the sidebar to go there, click a pane to focus it, drag a divider to
  resize, wheel to scroll a pane's history (typing jumps back down). Programs that use the mouse themselves
  (editors, htop) get the clicks and the wheel. `ctrl+b m` or `--no-mouse` turns it off so you can select
  text the usual way; in Windows Terminal, shift+drag selects even with the mouse on.
- **Copy exactly what Claude wrote.** Claude Code wraps long lines with real line breaks, so copying from the
  screen breaks code. `ctrl+b y` (or the `copy code` chip in the status line) lists the code blocks from
  Claude's last replies, read from its own transcript (`~/.claude/projects/...`, on whichever machine the
  pane runs), newest first with a preview; Enter or a click copies the exact text. `ctrl+b Y` copies the
  newest block at once. The clipboard is set with OSC 52, so it works over SSH too, and natively on Windows.
  Anywhere else, drag with the mouse to select inside one pane: it is copied on release, and lines the
  screen merely wrapped come out joined.
- **Themes and settings.** `ctrl+b ,` opens settings: theme (Gruvbox Dark, Monokai, Dracula, Tokyo Night,
  Catppuccin Mocha, One Dark, Nord, Solarized Dark, or Terminal for your terminal's own colours), the gap
  between sidebar and panes, mouse, predictive echo and sidebar width. Changes show at once and are saved
  to `%APPDATA%hc-tuisettings.json` (`~/.config/hc-tui/settings.json` elsewhere). Themes also recolour
  the default and 16 basic colours inside panes, like a terminal theme does.
- **Unplaced terminals.** Terminals started with `hc run` that no layout shows are listed at the bottom
  of the sidebar; Enter puts one into a new tab.

## Install

You need [herdr-core](https://github.com/herdrdev/herdr-core) (`hc`) installed and its core running.

```sh
cargo install --git https://github.com/BTL00/hc-tui
```

On Windows with the GNU toolchain, building needs `dlltool` on `PATH` (for example from
[llvm-mingw](https://github.com/mstorsjo/llvm-mingw)); the MSVC toolchain works as is.

## Use

```sh
hc-tui                      # sidebar + current tab
hc-tui --attach review      # open where that terminal is (or adopt it into a tab)
hc-tui --list               # print machines, workspaces and tabs, then exit
hc-tui --no-predict         # predictive echo off
hc-tui --no-mouse           # mouse off (select text natively)
```

`hc-tui` finds `hc` on `PATH`, then in `%LOCALAPPDATA%\Programs\herdr-core`; set `HC_TUI_HC` to override.

### Keys

| Keys | Action |
| --- | --- |
| `prefix c` | new tab (a shell in the focused pane's folder) |
| `prefix v` / `prefix -` | split right / down |
| `prefix h/j/k/l` | move between panes |
| `prefix H/J/K/L` | swap with the neighbouring pane |
| `prefix z` | zoom the focused pane |
| `prefix x` | close pane (ends its program) |
| `prefix r` | resize mode: `h/j/k/l`, any other key leaves |
| `prefix n` / `p` / `1..9` | next / previous / nth tab |
| `prefix T` / `X` | rename / close tab |
| `prefix N` / `W` / `D` | new / rename / close workspace |
| `prefix w` | workspace navigation: `j/k`, `←/→` between workspaces, Enter, Esc |
| `prefix g` | goto picker: `/` search, `b w i d a` filters, Enter |
| `prefix a` | jump to the next agent waiting for you |
| `prefix b` | toggle the sidebar |
| `prefix y` / `Y` | copy a code block Claude wrote: pick one / the newest |
| `prefix ,` | settings (theme, gap, mouse, echo, sidebar width) |
| `prefix m` | toggle the mouse |
| `prefix e` | toggle predictive echo |
| `prefix q` | detach; everything keeps running |
| `prefix ctrl+b` | send `ctrl+b` to the program |

## How it works

hc-tui runs `hc connect` twice: one framed connection for calls and terminal streams
(`terminal.attach`), one plain connection for `events.subscribe`. Each visible pane is attached with its
own size and rendered through a local [vt100](https://crates.io/crates/vt100) screen; the core answers the
programs' display queries. Layout changes go through `doc.get` / `doc.set` on the `herdr-app` document.

## Limitations

- No copy mode / scrollback yet, no mouse.
- Stacks and floating blocks from custom Herdr layouts are shown (the active child) but not editable.
- The vt100 emulator covers what shells and coding agents use; exotic sequences and graphics are ignored.

## License

Apache-2.0. Includes code adapted from herdr-core, herdr-app and herdr (Apache-2.0); see [NOTICE](NOTICE).
