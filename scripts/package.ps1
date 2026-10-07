#Requires -Version 5.1
# Wraps what build.ps1 staged in bin/ into the one archive a release carries.
# Run build.ps1 first: this packages bytes, it does not produce them.
[CmdletBinding()]
param(
    [switch]$NoPause
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$Root = Split-Path -Parent $PSScriptRoot
$Bin = Join-Path $Root 'bin'

$exitCode = 0

try {
    # The version lives in the workspace manifest, so the archive is named from
    # there rather than from a second copy someone has to remember.
    $manifest = Get-Content -Raw (Join-Path $Root 'Cargo.toml')
    if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw 'no version found in the workspace Cargo.toml'
    }
    $version = $Matches[1]

    # A release tag is the number the app's own update card compares against, so
    # a tag that disagrees with what was built is the same drift apps\gui's
    # build.rs already refuses to let through.
    $tag = $env:GITHUB_REF_NAME
    if ($tag -and $tag -like 'v*' -and $tag -ne "v$version") {
        throw "tag $tag is not the built version v$version"
    }

    # The manager is the artifact: both runner variants are already inside it,
    # and the license has to travel with the binary it covers.
    $files = @(
        (Join-Path $Bin 'shimbox.exe'),
        (Join-Path $Root 'README.md'),
        (Join-Path $Root 'LICENSE')
    )
    foreach ($file in $files) {
        if (-not (Test-Path $file)) {
            throw "missing $file - run scripts\build.cmd first"
        }
    }

    $zip = Join-Path $Bin "shimbox-$version-windows-x64.zip"
    Write-Section 'package'
    Remove-Item $zip -Force -ErrorAction SilentlyContinue
    Compress-Archive -Path $files -DestinationPath $zip
    $kb = [math]::Round((Get-Item $zip).Length / 1KB, 1)
    Write-Host ("  {0}  {1} KB" -f (Split-Path -Leaf $zip), $kb)
    Write-Section 'done'
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
