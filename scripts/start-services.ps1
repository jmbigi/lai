<#
.SYNOPSIS
    Start the complete local lai stack: llama.cpp (8080) and gateway (8081).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/start-services.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/start-services.ps1 -Restart -BuildGateway
#>
[CmdletBinding()]
param(
    [switch]$Restart,
    [switch]$BuildGateway,
    [switch]$SkipLlama,
    [switch]$SkipGateway,
    [string]$ModelPath = "",
    [int]$Context = 32768
)

$ErrorActionPreference = "Stop"
$startLlama = Join-Path $PSScriptRoot "start-llama.ps1"
$startGateway = Join-Path $PSScriptRoot "start-gateway.ps1"

if (-not $SkipLlama) {
    $llamaArgs = @{ Context = $Context }
    if ($ModelPath) { $llamaArgs.ModelPath = $ModelPath }
    if ($Restart) { $llamaArgs.Restart = $true }
    & $startLlama @llamaArgs
    if ($LASTEXITCODE -ne 0) { throw "llama.cpp failed to start." }
}

if (-not $SkipGateway) {
    $gatewayArgs = @{}
    if ($BuildGateway) { $gatewayArgs.Build = $true }
    if ($Restart) { $gatewayArgs.Restart = $true }
    & $startGateway @gatewayArgs
    if ($LASTEXITCODE -ne 0) { throw "gateway failed to start." }
}

Write-Host ""
Write-Host "Local stack is ready:" -ForegroundColor Green
if (-not $SkipLlama) { Write-Host "  llama.cpp: http://127.0.0.1:8080" }
if (-not $SkipGateway) { Write-Host "  gateway:   http://127.0.0.1:8081" }
Write-Host "Stop it with: powershell -ExecutionPolicy Bypass -File scripts/stop-services.ps1"
