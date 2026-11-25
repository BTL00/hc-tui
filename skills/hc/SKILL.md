---
name: hc
description: "Control herdr-core (hc): persistent terminals for programs and coding agents, shown in hc-tui and the Herdr apps. Use only when the user explicitly mentions hc, herdr-core, hc-tui or Herdr, or asks to start, read, message or wait on another terminal or agent there. Do not use merely because a task could benefit from a background terminal or delegation. Requires HC_TERMINAL_ID."
---

# hc (herdr-core)

`hc` runs programs and coding agents in terminals that stay alive. hc-tui and
the Herdr desktop and phone apps show those terminals. It is not `herdr`, the
older terminal multiplexer: there is no `herdr` command and no `HERDR_ENV`.

Before any control command, check that you run inside an hc terminal:

```bash
test -n "${HC_TERMINAL_ID:-}"
```

PowerShell: `if ($env:HC_TERMINAL_ID) { ... }`. If it is empty, say that you are
not running inside hc and stop.

## Read the manual of the installed version

The installed binary is the authority. Read its manual before acting:

```bash
hc --skill          # terminals, agents, messages, waiting, other machines, files
hc --help           # the command list; hc <command> --help for exact arguments
```

Follow that manual; it replaces anything you remember about `herdr`.

To change what the person sees (spaces, tabs, splits), read
`herdr-app --skill` when `herdr-app` is installed. `hc` itself does not arrange
windows; hc-tui and the apps share one layout.

## Rules that hold in every version

- `$HC_TERMINAL_ID` is your own address. Terminals you start are your children.
- Start agents with `hc run --bg --name <name> ...`; wait with `hc wait`, never
  with sleep or polling.
- Only kill or remove terminals you started. `hc ls` also lists the user's
  other work.
- Never run `hc server stop` or `hc server restart` unless the user asks: it
  affects every terminal. Experiments use their own `HC_SESSION`.
