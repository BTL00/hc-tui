# Demo GIF for the README, recorded with VHS (https://github.com/charmbracelet/vhs):
#   vhs docs/demo.tape      ->  docs/demo.gif
# Before recording: hc core running, two or three agents in a couple of
# workspaces, at least one of them done ("your turn") and one Claude pane
# with a code block in its last reply.

Output docs/demo.gif
Set Shell "pwsh"
Set FontSize 16
Set Width 1400
Set Height 760
Set Theme "Monokai Pro"
Set TypingSpeed 60ms

Type "hc-tui" Enter
Sleep 2.5s

# Who is waiting: jump to the next agent that needs you.
Ctrl+B Type "a"
Sleep 2s

# Copy a code block exactly as Claude wrote it.
Ctrl+B Type "y"
Sleep 2s
Down Sleep 700ms
Enter
Sleep 1.5s

# Themes from the settings menu.
Ctrl+B Type ","
Sleep 1s
Right Sleep 900ms
Right Sleep 900ms
Left Sleep 900ms
Escape
Sleep 1.5s

# Detach; everything keeps running.
Ctrl+B Type "q"
Sleep 1s
