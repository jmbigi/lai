<#
.SYNOPSIS
    Stop the local llama.cpp and/or gateway processes started by lai scripts.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/stop-services.ps1
#>
[CmdletBinding()]
param(
    [ValidateSet("all", "llama", "gateway")]
    [string]$Service = "all"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$LogDir = Join-Path $RepoRoot "logs"

function Stop-FromPidFile {
    param(
        [string]$Name,
        [string]$PidFile,
        [string]$ProcessPattern
    )

    if (-not (Test-Path $PidFile)) {
        Write-Host "$Name is not running (no PID file)." -ForegroundColor DarkGray
        return
    }

    $rawPid = Get-Content $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $rawPid) {
        Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
        Write-Host "$Name is not running (empty PID file)." -ForegroundColor DarkGray
        return
    }

    $process = Get-Process -Id ([int]$rawPid) -ErrorAction SilentlyContinue
    if ($process -and $process.ProcessName -like $ProcessPattern) {
        Write-Host "Stopping $Name (PID $rawPid)..." -ForegroundColor Yellow
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        try { Wait-Process -Id $process.Id -Timeout 10 -ErrorAction SilentlyContinue } catch { }
        Write-Host "$Name stopped." -ForegroundColor Green
    } else {
        Write-Host "$Name is not running (stale PID file)." -ForegroundColor DarkGray
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
}

if ($Service -in @("all", "llama")) {
    Stop-FromPidFile -Name "llama.cpp" -PidFile (Join-Path $LogDir "llama.pid") -ProcessPattern "llama-server*"
}
if ($Service -in @("all", "gateway")) {
    Stop-FromPidFile -Name "gateway" -PidFile (Join-Path $LogDir "gateway.pid") -ProcessPattern "gateway*"
}
