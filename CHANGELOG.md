# Changelog

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
