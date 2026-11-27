# Changelog

## Unreleased

- **Choose the machine for a new workspace.** `prefix N` shows the machine the workspace will be made on:
  the one you are on by default, Tab / Shift+Tab (or ↓ / ↑) picks another connected machine.
- **Calmer sidebar.** An empty line between workspaces, a thin line over each machine, and no `hc-tui`
  header.
- The sidebar keeps the tab you are in on screen when the tree is taller than the window.
- The help popup (`prefix ?`) is wide enough for its longest line; some lines were cut off.

## 1.0.2

- **No more blinking cursor in Claude Code panes.** While Claude works, it redraws its screen about ten
  times a second, hiding the cursor and moving it around each time, and hc-tui showed those half-done
  redraws. hc-tui now honours synchronized output (DEC mode 2026): a redraw the program marks is shown
  only once it is complete (or after 150 ms), and hc-tui's own frames reach your terminal the same way.
  Setting: "sync redraws".
- **Steady cursor.** The cursor moves only once it has stayed put for 60 ms (at once while you type).
  Setting "cursor": `steady`, `instant` (the old behaviour) or `off in agents` (hidden in agent panes,
  which draw their own caret).
- **No differently coloured strip around hc-tui.** The terminal's padding, scrollbar and leftover part of a
  row or column kept the terminal's own background. They now get the theme's (OSC 11) while hc-tui runs.
  Setting: "paint edges".
- **Ctrl+Enter and Shift+Enter reach Claude Code.** The Windows console hands programs Ctrl+Enter as a plain
  line feed and Shift+Enter as a plain Enter, so Claude only added a line or sent. hc-tui now notices which
  key it was and sends it the way the pane's program asked for (kitty keyboard protocol), so Ctrl+Enter
  sends your message while Claude works and Shift+Enter starts a new line. Other programs get what they
  always got.
- **An `hc` skill for agents** in `skills/hc`: agents in hc terminals have `HC_TERMINAL_ID`, not herdr's
  `HERDR_ENV`, and should read `hc --skill`. It replaces the old herdr skill.
- **Paste images into Claude on another machine.** Claude Code reads images from the clipboard of the
  machine it runs on, not yours. In a Claude pane on another machine, alt+v (or ctrl+v) now takes the
  image from your clipboard, puts it in a file beside that terminal (`terminal.put`) and pastes its path, which
  Claude attaches as `[Image #1]`. Without an image on the clipboard the key goes through as before.
  `ctrl+b i` does it in any pane. Setting: "paste images".

## 1.0.1

- **Copying inside Claude Code reaches your clipboard.** Claude Code takes the mouse, so a drag in its pane
  is Claude's own selection, and it copied to the clipboard of the machine it runs on. hc-tui now passes a
  program's clipboard request (OSC 52) on to the computer you are sitting at, like tmux's
  `set-clipboard on`. A setting, "copy from apps", turns it off. Requests replayed with a pane's
  history on attach are ignored, and programs can never read your clipboard.
- **Alt+drag** selects with hc-tui even in panes whose program uses the mouse.
- **`ctrl+b x` on a finished pane** (you typed `exit`) closes it at once, without the y/n question.

## 1.0.0

The first release. hc-tui is a terminal UI for [herdr-core](https://github.com/herdrdev/herdr-core):
herdr's sidebar and keys on top of the hc runtime, sharing one layout with the Herdr desktop and
iPhone apps.

- **Shared layout.** Workspaces, tabs and splits are the Herdr app's own layout document: change them
  here and the desktop and phone apps follow, and the other way round.
- **Every machine.** Machines added with `hc machine add` show up in the tree (💻) with their workspaces,
  tabs and panes.
- **Agents waiting for you.** An agent that finished its turn or got blocked while you looked elsewhere
  is highlighted (`your turn`, `needs you`), counted per workspace and in the status line;
  `ctrl+b a` jumps to the next one.
- **Copy exactly what Claude wrote.** `ctrl+b y` (or the `copy code` chip) lists the code blocks from
  Claude's last replies, read from its transcript instead of the screen, so wrapped lines don't break
  your code. Works for panes on other machines; OSC 52 clipboard, so it works over SSH too.
- **Mouse.** Click tabs and panes, drag dividers, wheel through a pane's history, drag to select text
  (wrapped lines come out joined). `ctrl+b m` turns it off.
- **Themes.** Gruvbox Dark, Monokai, Dracula, Tokyo Night, Catppuccin Mocha, One Dark, Nord, Solarized
  Dark and Terminal, in a settings menu (`ctrl+b ,`) that saves your choices.
- **Predictive echo.** Keys show up at once on slow links and are reconciled with the real echo,
  mosh-style (`ctrl+b e`).
- **herdr keys.** Prefix `ctrl+b`, herdr's defaults; `ctrl+b ?` lists them.
