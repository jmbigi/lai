<#
.SYNOPSIS
    Start a local llama.cpp server for lai.

.DESCRIPTION
    Resolves a GGUF model (explicit -ModelPath, LAI_MODEL_PATH, models/*.gguf,
    or an Ollama model blob), starts llama-server on 127.0.0.1:8080, writes a
    PID file under logs/ and waits for /health.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/start-llama.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/start-llama.ps1 -ModelPath C:\models\qwen3.gguf -Context 16384
#>
[CmdletBinding()]
param(
    [string]$ModelPath = "",
    [string]$OllamaModel = "qwen3:8b-nothink",
    [string]$ListenHost = "127.0.0.1",
    [int]$Port = 8080,
    [int]$Context = 32768,
    [int]$GpuLayers = 99,
    [string]$Alias = "qwen3-8b",
    [switch]$Restart
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$LogDir = Join-Path $RepoRoot "logs"
$PidFile = Join-Path $LogDir "llama.pid"
$OutLog = Join-Path $LogDir "llama.out.log"
$ErrLog = Join-Path $LogDir "llama.err.log"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Test-Health {
    param([int]$HealthPort)
    try {
        $response = Invoke-WebRequest -Uri "http://127.0.0.1:$HealthPort/health" -UseBasicParsing -TimeoutSec 2
        return ($response.StatusCode -eq 200)
    } catch {
        return $false
    }
}

function Stop-ExistingLlama {
    if (-not (Test-Path $PidFile)) { return }
    $oldPid = (Get-Content $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1)
    if (-not $oldPid) {
        Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
        return
    }
    $process = Get-Process -Id ([int]$oldPid) -ErrorAction SilentlyContinue
    if ($process -and $process.ProcessName -like "llama-server*") {
        Write-Host "Stopping existing llama-server (PID $oldPid)..." -ForegroundColor Yellow
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        try { Wait-Process -Id $process.Id -Timeout 10 -ErrorAction SilentlyContinue } catch { }
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
}

function Resolve-ModelPath {
    param([string]$Explicit)

    if ($Explicit) {
        if (-not (Test-Path -LiteralPath $Explicit)) { throw "Model not found: $Explicit" }
        return (Resolve-Path -LiteralPath $Explicit).Path
    }
    if ($env:LAI_MODEL_PATH) {
        if (-not (Test-Path -LiteralPath $env:LAI_MODEL_PATH)) {
            throw "LAI_MODEL_PATH points to a missing file: $env:LAI_MODEL_PATH"
        }
        return (Resolve-Path -LiteralPath $env:LAI_MODEL_PATH).Path
    }

    $modelDir = Join-Path $RepoRoot "models"
    if (Test-Path $modelDir) {
        $candidate = Get-ChildItem $modelDir -Filter *.gguf -File -ErrorAction SilentlyContinue |
            Sort-Object Length -Descending |
            Select-Object -First 1
        if ($candidate) { return $candidate.FullName }
    }

    $ollama = Get-Command ollama -ErrorAction SilentlyContinue
    if ($ollama) {
        $names = @($OllamaModel, "qwen2.5-coder:7b", "qwen3:8b-nothink", "qwen2.5-coder:3b") |
            Where-Object { $_ } |
            Select-Object -Unique
        foreach ($name in $names) {
            $modelfile = & ollama show --modelfile $name 2>$null
            if ($LASTEXITCODE -ne 0 -or -not $modelfile) { continue }
            $from = $modelfile | Where-Object { $_ -match '^\s*FROM\s+(.+?)\s*$' } | Select-Object -First 1
            if (-not $from) { continue }
            $null = $from -match '^\s*FROM\s+(.+?)\s*$'
            $path = $Matches[1].Trim().Trim('"')
            if (Test-Path -LiteralPath $path) {
                Write-Host "Using Ollama model blob for '$name'." -ForegroundColor DarkGray
                return (Resolve-Path -LiteralPath $path).Path
            }
        }
    }

    throw @"
No GGUF model found.
  - Put a .gguf file in $modelDir, or
  - set LAI_MODEL_PATH, or
  - pass -ModelPath C:\path\to\model.gguf, or
  - pull an Ollama model: ollama pull qwen2.5-coder:7b
"@
}

function Resolve-LlamaServer {
    $command = Get-Command llama-server -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }

    $patterns = @(
        "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\ggml.llamacpp*\llama-server.exe",
        "$env:ProgramFiles\llama.cpp\llama-server.exe",
        "$env:ProgramFiles\llama.cpp\bin\llama-server.exe"
    )
    foreach ($pattern in $patterns) {
        $found = Get-ChildItem -Path $pattern -File -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($found) { return $found.FullName }
    }
    throw "llama-server not found. Install it with: winget install ggml.llamacpp"
}

if (Test-Health -HealthPort $Port) {
    if (-not $Restart) {
        Write-Host "llama.cpp is already healthy on http://127.0.0.1:$Port" -ForegroundColor Green
        exit 0
    }
}

Stop-ExistingLlama

$listener = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue | Select-Object -First 1
if ($listener) {
    $owner = Get-Process -Id $listener.OwningProcess -ErrorAction SilentlyContinue
    $ownerName = if ($owner) { "$($owner.ProcessName) (PID $($owner.Id))" } else { "PID $($listener.OwningProcess)" }
    throw "Port $Port is already in use by $ownerName. Stop that process or use -Port <other>."
}

$resolvedModel = Resolve-ModelPath -Explicit $ModelPath
$llamaServer = Resolve-LlamaServer

function Quote-Argument {
    param([string]$Value)
    if ($Value -match '[\s"]') {
        return '"' + ($Value -replace '"', '\"') + '"'
    }
    return $Value
}

function Start-LlamaProcess {
    param([string[]]$ExtraArguments = @())

    $arguments = @(
        "-m", $resolvedModel,
        "--host", $ListenHost,
        "--port", "$Port",
        "-c", "$Context",
        "-ngl", "$GpuLayers",
        "--alias", $Alias,
        "--cont-batching",
        "--flash-attn", "auto"
    )
    if ($env:LAI_LLAMA_EXTRA_ARGS) {
        $arguments += ($env:LAI_LLAMA_EXTRA_ARGS -split '\s+')
    }
    if ($ExtraArguments.Count -gt 0) {
        $arguments += $ExtraArguments
    }

    $argumentLine = ($arguments | ForEach-Object { Quote-Argument $_ }) -join " "
    Remove-Item $OutLog, $ErrLog -Force -ErrorAction SilentlyContinue

    $process = Start-Process -FilePath $llamaServer `
        -ArgumentList $argumentLine `
        -RedirectStandardOutput $OutLog `
        -RedirectStandardError $ErrLog `
        -WindowStyle Hidden `
        -PassThru
    $process.Id | Set-Content -Path $PidFile -Encoding ASCII

    for ($i = 1; $i -le 120; $i++) {
        Start-Sleep -Seconds 1
        if (Test-Health -HealthPort $Port) {
            return [pscustomobject]@{ Healthy = $true; Process = $process; Stderr = ""; Seconds = $i }
        }
        if ($process.HasExited) { break }
    }

    $stderr = ""
    if (Test-Path $ErrLog) {
        $stderr = Get-Content $ErrLog -Raw -ErrorAction SilentlyContinue
    }
    return [pscustomobject]@{ Healthy = $false; Process = $process; Stderr = $stderr; Seconds = $i }
}

Write-Host "Starting llama.cpp" -ForegroundColor Cyan
Write-Host "  server: $llamaServer"
Write-Host "  model:  $resolvedModel"
Write-Host "  url:    http://${ListenHost}:$Port"

$attempt = Start-LlamaProcess
if (-not $attempt.Healthy) {
    $memoryError = $attempt.Stderr -match 'failed to allocate buffer for kv cache|ErrorOutOfDeviceMemory|out of memory|failed to initialize the context'
    $noKvOffload = $env:LAI_LLAMA_EXTRA_ARGS -match '(-nkvo|--no-kv-offload)'
    if ($memoryError -and -not $noKvOffload) {
        Write-Host "GPU memory insufficient for the KV cache; retrying with KV cache on CPU (-nkvo)..." -ForegroundColor Yellow
        Stop-Process -Id $attempt.Process.Id -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 1
        $attempt = Start-LlamaProcess -ExtraArguments @("-nkvo")
    }
}

if ($attempt.Healthy) {
    Write-Host "llama.cpp healthy after $($attempt.Seconds)s (PID $($attempt.Process.Id))." -ForegroundColor Green
    Write-Host "Logs: $OutLog / $ErrLog"
    exit 0
}

Write-Host "llama.cpp did not become healthy." -ForegroundColor Red
if (Test-Path $ErrLog) {
    Write-Host "--- stderr (tail) ---" -ForegroundColor DarkGray
    Get-Content $ErrLog -Tail 40 -ErrorAction SilentlyContinue
}
if (Test-Path $OutLog) {
    Write-Host "--- stdout (tail) ---" -ForegroundColor DarkGray
    Get-Content $OutLog -Tail 20 -ErrorAction SilentlyContinue
}
Stop-Process -Id $attempt.Process.Id -Force -ErrorAction SilentlyContinue
Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
exit 1
