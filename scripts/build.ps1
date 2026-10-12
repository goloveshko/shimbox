#Requires -Version 5.1
# Master build script. Thin .cmd wrappers call this; all logic lives here.
[CmdletBinding()]
param(
    [ValidateSet('release', 'debug')]
    [string]$Profile = 'release',
    [switch]$TestOnly,
    [switch]$NoPause,
    # For CI: the size target of 2.2 is a requirement, not a report, so a build
    # that misses it is a failed one. A double-clicked build keeps printing the
    # line and leaves the deciding to whoever reads it.
    [switch]$StrictSize
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$Root = Split-Path -Parent $PSScriptRoot
$BinDir = Join-Path $Root 'bin'
$SizeTargetKb = 400

function Invoke-Cargo {
    param([string[]]$Arguments)
    Write-Host ("cargo {0}" -f ($Arguments -join ' ')) -ForegroundColor DarkGray
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments[0]) failed with exit code $LASTEXITCODE"
    }
}

function Write-SizeRow {
    param([string]$Label, [string]$Path, [int]$TargetKb)
    if (-not (Test-Path $Path)) {
        Write-Host ("  {0,-14} missing" -f $Label) -ForegroundColor Red
        return
    }
    $bytes = (Get-Item $Path).Length
    $kb = [math]::Round($bytes / 1KB, 1)
    if ($TargetKb -eq 0) {
        Write-Host ("  {0,-14} {1,8} KB" -f $Label, $kb)
        return
    }
    if ($bytes -le $TargetKb * 1KB) {
        $verdict = 'within target'
        $color = 'Green'
    }
    else {
        $verdict = 'OVER target'
        $color = 'Yellow'
        if ($StrictSize) {
            throw "$Label is $([math]::Round($bytes / 1KB, 1)) KB, over the $TargetKb KB target"
        }
    }
    Write-Host ("  {0,-14} {1,8} KB  {2} {3} KB" -f $Label, $kb, $verdict, $TargetKb) -ForegroundColor $color
}

# The runner has its own size profile; the Tauri app builds with the plain
# release profile. Tests always run unoptimized.
if ($Profile -eq 'release') {
    $runnerArgs = @('--profile', 'dist')
    $guiArgs = @('--release')
    $runnerDir = Join-Path $Root 'target/dist'
    $guiDir = Join-Path $Root 'target/release'
}
else {
    $runnerArgs = @()
    $guiArgs = @()
    $runnerDir = Join-Path $Root 'target/debug'
    $guiDir = Join-Path $Root 'target/debug'
}

$exitCode = 0

try {
    Push-Location $Root

    # Checked before anything is compiled, because a build that spends a minute on
    # the runners only to say that a line is wrapped differently is a minute nobody
    # needed. Unlike the size target this is a gate for everyone, not one for CI: the
    # tree is either formatted or it is not, and the answer to 'not' is a command
    # rather than a judgement, so there is nothing to decide and no reason to let a
    # month of small drift accumulate before someone reads a report.
    Write-Section 'format'
    Write-Host 'cargo fmt --all -- --check' -ForegroundColor DarkGray
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) {
        throw 'the Rust tree is not formatted; run `cargo fmt --all`, read what it changed, and build again'
    }

    # The same argument for the UI, which no compiler ever reads: a file that left
    # Prettier's style is invisible until someone opens the window. What is checked
    # is apps\gui's own `check` script rather than a glob repeated here, so the
    # question 'which files are the UI' has one answer in one file; Prettier is not
    # something cargo brings, so a missing install is still named as the setup step
    # it is instead of being skipped - a check that passes silently when its tool is
    # absent is the drift arriving back.
    Write-Section 'ui format'
    $prettier = Join-Path $Root 'apps\gui\node_modules\.bin\prettier.cmd'
    if (-not (Test-Path $prettier)) {
        throw 'Prettier is not installed; run `npm install` in apps\gui, then build again'
    }
    Push-Location (Join-Path $Root 'apps\gui')
    try {
        Write-Host 'npm run check' -ForegroundColor DarkGray
        npm run --silent check
    }
    finally {
        Pop-Location
    }
    if ($LASTEXITCODE -ne 0) {
        throw 'the UI is not formatted; run `npm run format` in apps\gui, read what it changed, and build again'
    }

    if (-not (Test-Path $BinDir)) {
        New-Item -ItemType Directory -Path $BinDir | Out-Null
    }

    # The runners are staged before anything is tested or built: the GUI embeds
    # those bytes, so both later steps need them to exist. One cargo call builds
    # both, since they are two crates over one library and the second used to
    # rebuild the first's bytes under one name that staging had already copied.
    Write-Section 'runner: CLI and GUI variants'
    Invoke-Cargo ((@('build', '-p', 'runner-cli', '-p', 'runner-gui') + $runnerArgs))
    $cli = Join-Path $BinDir 'runner-cli.exe'
    $gui = Join-Path $BinDir 'runner-gui.exe'
    foreach ($staged in @($cli, $gui)) {
        Copy-Item (Join-Path $runnerDir (Split-Path -Leaf $staged)) $staged -Force
    }

    Write-Section 'tests'
    Invoke-Cargo @('test', '--workspace')

    if (-not $TestOnly) {
        $manager = Join-Path $BinDir 'shimbox.exe'
        $managerArtifact = Join-Path $guiDir 'gui.exe'

        Write-Section 'gui: Tauri manager'
        Invoke-Cargo ((@('build', '-p', 'gui') + $guiArgs))

        # The manager is staged as a hard link rather than a copy, and only after a
        # build that compiled, so a failed one leaves bin holding what it held before.
        # A link needs the staged name gone first, which Windows allows while nothing
        # runs from it and refuses once this rebuild took the two `target` names away
        # from the bytes an open window runs. A running manager is therefore reported
        # rather than fought over: it keeps its bytes, bin keeps its name, and the next
        # build with the window closed links the new ones.
        $running = Get-Process -Name 'shimbox' -ErrorAction SilentlyContinue
        if ($running) {
            Write-Host '  a shimbox window is open: this build is not staged' -ForegroundColor Yellow
        }
        else {
            if (Test-Path $manager) {
                Remove-Item $manager -Force
            }
            New-Item -ItemType HardLink -Path $manager -Target $managerArtifact | Out-Null
        }

        Write-Section 'artifacts'
        Write-SizeRow 'runner-cli' $cli $SizeTargetKb
        Write-SizeRow 'runner-gui' $gui $SizeTargetKb
        Write-SizeRow 'shimbox' $manager 0
        Write-Host "  $BinDir" -ForegroundColor DarkGray
    }

    Write-Section 'done'
}
catch {
    Write-Host ''
    Write-Host $_.Exception.Message -ForegroundColor Red
    $exitCode = 1
}
finally {
    Pop-Location

    Set-PauseIfDoubleClicked -Enabled (-not $NoPause)
}

exit $exitCode
