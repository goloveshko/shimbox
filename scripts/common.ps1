#Requires -Version 5.1
# Shared helpers for the scripts\*.ps1 masters. Dot-sourced, no side effects.

function Test-DoubleClicked {
    # The double-click chain is explorer -> cmd -> powershell, so walk a few
    # generations up; anything else means a shell or IDE task, which must not
    # wait for a key press.
    $id = $PID
    for ($depth = 0; $depth -le 3; $depth++) {
        $process = Get-CimInstance Win32_Process -Filter "ProcessId=$id" -ErrorAction SilentlyContinue
        if (-not $process) { return $false }
        if ($process.Name -eq 'explorer.exe') { return $true }
        $id = [int]$process.ParentProcessId
    }
    return $false
}

function Set-PauseIfDoubleClicked {
    param([bool]$Enabled = $true)

    if (-not $Enabled -or $env:SHIMBOX_NO_PAUSE -or -not (Test-DoubleClicked)) {
        return
    }

    Write-Host ''
    Write-Host 'Press any key to close this window...' -ForegroundColor DarkGray
    try {
        $null = [Console]::ReadKey($true)
    }
    catch {
        $null = Read-Host
    }
}
