# Sets up a throwaway herdr-core instance for recording docs/demo.gif, so the
# GIF shows demo sessions only, never your real workspaces.
#
#   pwsh docs/demo-setup.ps1          # start the "demo" core and its sessions
#   vhs docs/demo.tape                # record (the tape sets HC_SESSION=demo)
#   pwsh docs/demo-setup.ps1 -Stop    # end the demo sessions
#
# Needs: hc, claude (Claude Code), cargo, and the hc-tui release build.
param([switch]$Stop)
$ErrorActionPreference = 'Stop'

# A separate core: everything below talks to it, not to your usual one.
$env:HC_SESSION = 'demo'
Remove-Item env:HC_TERMINAL_SOCKET, env:HC_TERMINAL_ID, env:HC_SCOPE -ErrorAction SilentlyContinue

$repo = Split-Path $PSScriptRoot -Parent
$shop = Join-Path $HOME 'demo\shop'

if ($Stop) {
    hc ls --json | ConvertFrom-Json | ForEach-Object { $_.terminals } | ForEach-Object {
        hc kill $_.address | Out-Null; hc rm $_.address 2>$null | Out-Null }
    return
}

New-Item -ItemType Directory -Force $shop | Out-Null
hc server start | Out-Null

# One JSON-RPC connection to the demo core. hc connect answers only while its
# input is open, so it stays open until the end.
$hc = [Diagnostics.Process]::Start([Diagnostics.ProcessStartInfo]@{
    FileName = (Get-Command hc).Source; Arguments = 'connect'
    RedirectStandardInput = $true; RedirectStandardOutput = $true; UseShellExecute = $false })
$script:id = 0
function Invoke-Hc([string]$method, $params) {
    $script:id++
    $hc.StandardInput.WriteLine((@{ jsonrpc = '2.0'; id = $script:id; method = $method; params = $params } | ConvertTo-Json -Depth 20 -Compress))
    $r = $hc.StandardOutput.ReadLine() | ConvertFrom-Json
    if ($r.error) { throw "$method failed: $($r.error.message)" }
    $r.result
}
function Start-Term([string]$cwd, [string[]]$argv) {
    $r = Invoke-Hc 'terminal.run' @{
        argv = $argv; cwd = $cwd; no_parent = $true; outlive_parent = $true
        env = @{
            TERM = 'xterm-256color'; COLORTERM = 'truecolor'
            # A Claude started from inside another Claude would not save its
            # transcript, and the copy picker reads the transcript.
            CLAUDE_CODE_FORCE_SESSION_PERSISTENCE = '1'; CLAUDE_CODE_CHILD_SESSION = ''
        }
    }
    if ($r.address) { $r.address } else { $r.terminal.address }
}
# Only the project's settings and default permissions: no personal status line,
# plugins or auto mode, so the shop Claude stops to ask before writing a file.
$claudeArgs = @('claude', '--setting-sources', 'project,local', '--permission-mode', 'default')

# hc-tui workspace: Claude writes a code block (finishes: "your turn"), a shell runs the tests.
$claude = Start-Term $repo ($claudeArgs + 'Write a small Rust function that formats a number of seconds as "1h 02m 03s", with a unit test. Only answer in chat, do not edit any files.')
$tests = Start-Term $repo @('pwsh', '-NoLogo', '-NoExit', '-Command', '$Host.UI.RawUI.WindowTitle = "tests"; cargo test')
# shop workspace: Claude wants to write a file and waits for permission ("needs you").
$shopClaude = Start-Term $shop ($claudeArgs + 'Create checkout.js with a validateCheckout(form) function that checks email, card number and postcode.')
$shell = Start-Term $shop @('pwsh', '-NoLogo', '-NoExit', '-Command', '$Host.UI.RawUI.WindowTitle = "shell"; Get-ChildItem')

# The first tab is where hc-tui opens, so it is a plain shell: both agents
# stay marked as waiting until the recording jumps to them.
function Block([string]$t) { @{ id = "p$t"; type = 'block'; content = @{ terminal = $t } } }
$doc = @{
    spaces = @(
        @{ id = 'sp2'; name = 'shop'; folder = $shop; tabs = @(
            @{ id = 'tb3'; name = 'shell'; root = @{ id = 'tb3'; type = 'row'; children = @((Block $shell)) } },
            @{ id = 'tb2'; name = 'checkout'; root = @{ id = 'tb2'; type = 'row'; children = @((Block $shopClaude)) } }
        ) },
        @{ id = 'sp1'; name = 'hc-tui'; folder = $repo; tabs = @(
            @{ id = 'tb1'; name = 'review'; root = @{ id = 'tb1'; type = 'row'; children = @((Block $claude), (Block $tests)) } }
        ) }
    )
}
$current = Invoke-Hc 'doc.get' @{ name = 'herdr-app' }
Invoke-Hc 'doc.set' @{ name = 'herdr-app'; value = $doc; version = $current.version } | Out-Null
$hc.StandardInput.Close(); $hc.WaitForExit(5000) | Out-Null
"waiting for both Claudes to stop working ($claude, $shopClaude)…"
# Claude takes a few seconds to start; before that, wait returns at once.
foreach ($t in $claude, $shopClaude) {
    $deadline = (Get-Date).AddMinutes(4)
    do {
        Start-Sleep -Seconds 5
        $s = (hc ls --json | ConvertFrom-Json).terminals | Where-Object address -eq $t
    } until ($s.status -in 'done', 'blocked' -or (Get-Date) -gt $deadline)
}
hc ls
"demo ready: record with  vhs docs/demo.tape"
