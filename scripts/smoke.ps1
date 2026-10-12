#Requires -Version 5.1
# End-to-end checks for the runner against real binaries (docs/techspec.md 4).
# Run build.ps1 first: this reads its shim templates from bin/.
[CmdletBinding()]
param(
    [switch]$NoPause
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

trap {
    Write-Host ''
    Write-Host $_.Exception.Message -ForegroundColor Red
    Set-PauseIfDoubleClicked -Enabled (-not $NoPause)
    exit 1
}

$Root = Split-Path -Parent $PSScriptRoot
$Bin = Join-Path $Root 'bin'
$Work = Join-Path $env:TEMP 'shimbox-smoke'
# Named by path rather than by `start cmd` or `$env:ComSpec` on purpose: these
# checks assert what `cmd` answers to a shim - `echo`'s spacing, `more`'s stdin,
# `cd`'s output - and a machine whose command processor is pointed somewhere else
# would then fail for a reason that has nothing to do with the shim. The same path
# is also what the shim configs below are told to run.
$Cmd = 'C:\Windows\System32\cmd.exe'

$script:passed = 0
$script:failed = 0

function Check {
    param([string]$Name, [bool]$Condition, [string]$Detail)
    if ($Condition) {
        $script:passed++
        Write-Host ("  PASS  {0}" -f $Name) -ForegroundColor Green
    }
    else {
        $script:failed++
        Write-Host ("  FAIL  {0}: {1}" -f $Name, $Detail) -ForegroundColor Red
    }
}

function New-Shim {
    param([string]$Runner, [string]$Name, [string[]]$ConfigLines)
    $exe = Join-Path $Work "$Name.exe"
    Copy-Item $Runner $exe -Force
    $ConfigLines | Set-Content -Path $exe.Replace('.exe', '.shim') -Encoding ASCII
    return $exe
}

function Write-Config {
    param([string]$Name, [string[]]$Lines)
    Set-Content -Path (Join-Path $Work "$Name.shim") -Value $Lines -Encoding ASCII
}

Remove-Item $Work -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $Work | Out-Null

$cliRunner = Join-Path $Bin 'runner-cli.exe'
$guiRunner = Join-Path $Bin 'runner-gui.exe'
if (-not (Test-Path $cliRunner) -or -not (Test-Path $guiRunner)) {
    throw "build bin/runner-cli.exe and bin/runner-gui.exe first (scripts\build.cmd)"
}

Write-Host ''
Write-Host '== CLI variant' -ForegroundColor Cyan

$cli = New-Shim -Runner $cliRunner -Name 'echocli' -ConfigLines @(
    "path=$Cmd",
    'arg=/c'
)

# The caller passes one argument containing a double space; PowerShell quotes it,
# so the child must receive it unsplit and unmodified.
$output = & $cli echo 'first  second' | Out-String
Check 'arguments forwarded verbatim' ($output -match 'first\s\ssecond') "got: $($output.Trim())"

& $cli exit 7 | Out-Null
Check 'exit code propagated' ($LASTEXITCODE -eq 7) "got: $LASTEXITCODE"

# No workdir in the config: the child must land in the target's own directory.
$output = & $cli cd | Out-String
Check 'workdir defaults to target dir' ($output.Trim() -ieq 'C:\Windows\System32') "got: $($output.Trim())"

Write-Config -Name 'echocli' -Lines @(
    "path=$Cmd",
    'arg=/c',
    "workdir=$Work"
)
$output = & $cli cd | Out-String
Check 'explicit workdir honoured' ($output.Trim() -ieq $Work) "got: $($output.Trim())"

# Interactive stdin: cmd /c more consumes piped input through the shim.
$piped = 'from stdin' | & $cli more | Out-String
Check 'stdin inherited' ($piped -match 'from stdin') "got: $($piped.Trim())"

Write-Host ''
Write-Host '== error reporting' -ForegroundColor Cyan

$orphan = Join-Path $Work 'noconfig.exe'
Copy-Item $cliRunner $orphan -Force
$output = & $Cmd /c "`"$orphan`" 2>&1" | Out-String
Check 'missing config exits 101' ($LASTEXITCODE -eq 101) "got: $LASTEXITCODE"
Check 'missing config names the file' ($output -match 'noconfig\.shim') "got: $($output.Trim())"

Write-Config -Name 'echocli' -Lines @(
    'path=C:\nope\missing-target.exe',
    'arg=/c'
)
$output = & $Cmd /c "`"$cli`" 2>&1" | Out-String
Check 'spawn failure exits 103' ($LASTEXITCODE -eq 103) "got: $LASTEXITCODE, output: $($output.Trim())"
Check 'spawn failure names the target' ($output -match 'missing-target\.exe') "got: $($output.Trim())"

Write-Config -Name 'echocli' -Lines @(
    'arg=/c'
)
$output = & $Cmd /c "`"$cli`" 2>&1" | Out-String
Check 'config without path exits 102' ($LASTEXITCODE -eq 102) "got: $LASTEXITCODE"
Check 'path-less config explains itself' ($output -match 'no .path.') "got: $($output.Trim())"

Write-Host ''
Write-Host '== GUI variant' -ForegroundColor Cyan

$gui = New-Shim -Runner $guiRunner -Name 'echogui' -ConfigLines @(
    "path=$Cmd",
    'arg=/c'
)
& $gui exit 7 | Out-Null
Check 'gui variant propagates exit code' ($LASTEXITCODE -eq 7) "got: $LASTEXITCODE"

Write-Host ''
Write-Host '== manual checks left for a human' -ForegroundColor DarkGray
Write-Host '  - double-click a GUI shim from Explorer: no console window appears'
Write-Host '  - double-click a CLI shim of a console tool: console shows, output stays open'
Write-Host '  - break the config of a GUI shim: a message box appears, no silent failure'
Write-Host ''
Write-Host ("{0} passed, {1} failed" -f $script:passed, $script:failed) -ForegroundColor $(if ($script:failed) { 'Red' } else { 'Green' })

Set-PauseIfDoubleClicked -Enabled (-not $NoPause)

if ($script:failed) {
    exit 1
}

# Stated rather than left to fall out: the last command this script runs is a shim
# asked to exit with 7, and a caller that chains these - scripts\release.ps1 - reads
# that leftover as this script's own answer.
exit 0
