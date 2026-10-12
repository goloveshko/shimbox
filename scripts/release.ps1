#Requires -Version 5.1
# One click for a whole release: the three masters scripts\ is made of, in the
# order that makes each one's output the next one's input. Nothing is re-implemented
# here - a step answers with its own exit code and the next is not started, because
# the mistake this file exists to prevent is packaging a bin the build never filled.
[CmdletBinding()]
param(
    [ValidateSet('release', 'debug')]
    [string]$Profile = 'release',
    [switch]$NoPause
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

# The arguments go as a hashtable rather than a list because `@list` would hand the
# masters '-Profile' as a positional value, and `release` would arrive as the text
# of a switch.
function Invoke-Step {
    param([string]$Name, [hashtable]$Arguments)

    Write-Section "release: $Name"
    & (Join-Path $PSScriptRoot "$Name.ps1") @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

$exitCode = 0

try {
    # Each master is called with its own pause off: this script owns the window's
    # last key press, and a release that stopped for one per step is three clicks
    # wearing one. The size target of 2.2 is a requirement of a release rather than
    # a report, so the click that makes one asks the build what CI asks it - while a
    # debug profile stays a working build, left to read its own sizes.
    Invoke-Step 'build' @{
        Profile    = $Profile
        NoPause    = $true
        StrictSize = ($Profile -eq 'release')
    }
    Invoke-Step 'smoke' @{ NoPause = $true }
    Invoke-Step 'package' @{ NoPause = $true }

    Write-Section 'release: done'
}
catch {
    Write-Host ''
    Write-Host $_.Exception.Message -ForegroundColor Red
    $exitCode = 1
}
finally {
    Set-PauseIfDoubleClicked -Enabled (-not $NoPause)
}

exit $exitCode
