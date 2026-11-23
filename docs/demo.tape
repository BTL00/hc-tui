# Demo GIF for the README, recorded with VHS (https://github.com/charmbracelet/vhs),
# which drives a real terminal (ttyd + xterm.js) in a headless browser.
#
#   pwsh docs/demo-setup.ps1        # throwaway "demo" core with demo sessions
#   vhs docs/demo.tape              # -> docs/demo.gif
#   pwsh docs/demo-setup.ps1 -Stop
#
# Run from the repo root after `cargo build --release`. The copy step puts
# the demo's code block on your clipboard.

Output docs/demo.gif
Set Shell "pwsh"
Set FontSize 15
Set Width 1440
Set Height 800
Set Padding 12
Set TypingSpeed 50ms
Set Framerate 24

# Hidden: talk to the demo core, not your usual one.
Hide
Type "$env:HC_SESSION = 'demo'; Remove-Item env:HC_TERMINAL_*, env:HC_SCOPE -ErrorAction SilentlyContinue; $env:PATH = (Resolve-Path target/release).Path + ';' + $env:PATH; Clear-Host" Enter
Sleep 1.5s
Show

Type "hc-tui" Sleep 400ms Enter
Sleep 3.5s

# Two agents are waiting: jump to the next one, then the other.
Ctrl+B Type "a"
Sleep 3s
Ctrl+B Type "a"
Sleep 3s

# Copy a code block exactly as Claude wrote it, without the screen's line wraps.
Ctrl+B Type "y"
Sleep 3s
Down Sleep 1.5s
Enter
Sleep 2.5s

# Themes from the settings menu, applied live.
Ctrl+B Type ","
Sleep 1.5s
Right Sleep 1.2s
Right Sleep 1.2s
Right Sleep 1.2s
Left Sleep 1.2s
Left Sleep 1.2s
Left Sleep 1s
Escape
Sleep 1.5s

# Detach; everything keeps running.
Ctrl+B Type "q"
Sleep 1.5s
