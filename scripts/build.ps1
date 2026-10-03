#Requires -Version 5.1
# Master build script. Thin .cmd wrappers call this; all logic lives here.
[CmdletBinding()]
param(
    [ValidateSet('release', 'debug')]
    [string]$Profile = 'release',
    [switch]$TestOnly,
    [switch]$NoPause
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$Root = Split-Path -Parent $PSScriptRoot
$BinDir = Join-Path $Root 'bin'
$SizeTargetKb = 400

function Write-Section {
    param([string]$Text)
    Write-Host ''
    Write-Host "== $Text" -ForegroundColor Cyan
}

function Invoke-Cargo {
    param([string[]]$Arguments)
    Write-Host ("cargo {0}" -f ($Arguments -join ' ')) -ForegroundColor DarkGray
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments[0]) failed with exit code $LASTEXITCODE"
    }
}

function Write-SizeRow {
    param([string]$Label, [string]$Path)
    if (-not (Test-Path $Path)) {
        Write-Host ("  {0,-14} missing" -f $Label) -ForegroundColor Red
        return
    }
    $bytes = (Get-Item $Path).Length
    $kb = [math]::Round($bytes / 1KB, 1)
    if ($bytes -le $SizeTargetKb * 1KB) {
        $verdict = 'within target'
        $color = 'Green'
    }
    else {
        $verdict = 'OVER target'
        $color = 'Yellow'
    }
    Write-Host ("  {0,-14} {1,8} KB  {2} {3} KB" -f $Label, $kb, $verdict, $SizeTargetKb) -ForegroundColor $color
}

$profileArgs = @()
if ($Profile -eq 'release') { $profileArgs = @('--release') }
$artifactDir = Join-Path (Join-Path $Root 'target') $Profile
$runner = Join-Path $artifactDir 'shim-runner.exe'

$exitCode = 0

try {
    Push-Location $Root

    Write-Section 'tests'
    Invoke-Cargo (@('test', '-p', 'runner') + $profileArgs)

    if (-not $TestOnly) {
        Write-Section 'runner: CLI variant (console subsystem)'
        Invoke-Cargo ((@('build', '-p', 'runner') + $profileArgs))
        if (-not (Test-Path $BinDir)) {
            New-Item -ItemType Directory -Path $BinDir | Out-Null
        }
        $cli = Join-Path $BinDir 'runner-cli.exe'
        Copy-Item $runner $cli -Force

        Write-Section 'runner: GUI variant (windows subsystem)'
        Invoke-Cargo ((@('build', '-p', 'runner', '--features', 'gui') + $profileArgs))
        $gui = Join-Path $BinDir 'runner-gui.exe'
        Copy-Item $runner $gui -Force

        Write-Section 'artifacts'
        Write-SizeRow 'runner-cli' $cli
        Write-SizeRow 'runner-gui' $gui
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
