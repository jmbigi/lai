<#
.SYNOPSIS
    Build and start the Go gateway on 127.0.0.1:8081.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/start-gateway.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/start-gateway.ps1 -Build -Restart
#>
[CmdletBinding()]
param(
    [int]$Port = 8081,
    [switch]$Build,
    [switch]$Restart
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$GatewayDir = Join-Path $RepoRoot "gateway-go"
$Config = Join-Path $GatewayDir "config.yaml"
$Executable = Join-Path $GatewayDir "gateway.exe"
$LogDir = Join-Path $RepoRoot "logs"
$PidFile = Join-Path $LogDir "gateway.pid"
$OutLog = Join-Path $LogDir "gateway.out.log"
$ErrLog = Join-Path $LogDir "gateway.err.log"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @($machine, $user, $env:Path) -split ";" |
        Where-Object { $_ -and $_.Trim() } |
        Select-Object -Unique
    $env:Path = $parts -join ";"
}

function Test-Health {
    param([int]$HealthPort)
    try {
        $response = Invoke-WebRequest -Uri "http://127.0.0.1:$HealthPort/health" -UseBasicParsing -TimeoutSec 2
        return ($response.StatusCode -eq 200)
    } catch {
        return $false
    }
}

function Stop-ExistingGateway {
    if (-not (Test-Path $PidFile)) { return }
    $oldPid = (Get-Content $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1)
    if (-not $oldPid) {
        Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
        return
    }
    $process = Get-Process -Id ([int]$oldPid) -ErrorAction SilentlyContinue
    if ($process -and $process.ProcessName -like "gateway*") {
        Write-Host "Stopping existing gateway (PID $oldPid)..." -ForegroundColor Yellow
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        try { Wait-Process -Id $process.Id -Timeout 10 -ErrorAction SilentlyContinue } catch { }
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
}

Update-SessionPath

if (Test-Health -HealthPort $Port) {
    if (-not $Restart) {
        Write-Host "Gateway is already healthy on http://127.0.0.1:$Port" -ForegroundColor Green
        exit 0
    }
}

Stop-ExistingGateway

$listener = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue | Select-Object -First 1
if ($listener) {
    $owner = Get-Process -Id $listener.OwningProcess -ErrorAction SilentlyContinue
    $ownerName = if ($owner) { "$($owner.ProcessName) (PID $($owner.Id))" } else { "PID $($listener.OwningProcess)" }
    throw "Port $Port is already in use by $ownerName. Stop that process or use -Port <other>."
}

if ($Build -or -not (Test-Path $Executable)) {
    if (-not (Get-Command go -ErrorAction SilentlyContinue)) {
        throw "go not found. Run scripts/setup-dev.ps1 or install Go first."
    }
    Write-Host "Building gateway..." -ForegroundColor Cyan
    Push-Location $GatewayDir
    try {
        & go build -o gateway.exe .
        if ($LASTEXITCODE -ne 0) { throw "go build failed." }
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path $Config)) { throw "Gateway config not found: $Config" }

Write-Host "Starting gateway on http://127.0.0.1:$Port" -ForegroundColor Cyan
Remove-Item $OutLog, $ErrLog -Force -ErrorAction SilentlyContinue

$process = Start-Process -FilePath $Executable `
    -ArgumentList "`"$Config`"" `
    -WorkingDirectory $GatewayDir `
    -RedirectStandardOutput $OutLog `
    -RedirectStandardError $ErrLog `
    -WindowStyle Hidden `
    -PassThru
$process.Id | Set-Content -Path $PidFile -Encoding ASCII

for ($i = 1; $i -le 30; $i++) {
    Start-Sleep -Seconds 1
    if (Test-Health -HealthPort $Port) {
        Write-Host "Gateway healthy after ${i}s (PID $($process.Id))." -ForegroundColor Green
        Write-Host "Logs: $OutLog / $ErrLog"
        exit 0
    }
    if ($process.HasExited) { break }
}

Write-Host "Gateway did not become healthy." -ForegroundColor Red
if (Test-Path $ErrLog) {
    Write-Host "--- stderr (tail) ---" -ForegroundColor DarkGray
    Get-Content $ErrLog -Tail 40 -ErrorAction SilentlyContinue
}
if (Test-Path $OutLog) {
    Write-Host "--- stdout (tail) ---" -ForegroundColor DarkGray
    Get-Content $OutLog -Tail 20 -ErrorAction SilentlyContinue
}
Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
exit 1
