<#
.SYNOPSIS
    Install and prepare the lai development environment on Windows.

.DESCRIPTION
    Idempotent setup: installs Git, Rust, Go, Python, CMake, LLVM, llama.cpp
    and Ollama through winget when missing, creates the Python virtual
    environment, builds the Go gateway and the Rust CLI.

    Run it from an elevated PowerShell if you want machine-wide installs:
        powershell -ExecutionPolicy Bypass -File scripts/setup-dev.ps1 -Elevate

.PARAMETER Elevate
    Relaunch the script as Administrator (UAC prompt) before installing.

.PARAMETER RunTests
    Run cargo test and pytest after building.

.PARAMETER SkipBuild
    Only install tools and Python dependencies; do not build.

.PARAMETER SkipModels
    Do not inspect/download local models.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/setup-dev.ps1 -Elevate -RunTests
#>
[CmdletBinding()]
param(
    [switch]$Elevate,
    [switch]$RunTests,
    [switch]$SkipBuild,
    [switch]$SkipModels
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @($machine, $user, $env:Path) -split ";" |
        Where-Object { $_ -and $_.Trim() } |
        Select-Object -Unique
    $env:Path = $parts -join ";"
}

# Native tools (cargo, go, pip, winget) write progress to stderr. Windows
# PowerShell 5.1 turns that into a terminating error when
# ErrorActionPreference is Stop, so relax it only for the native call.
function Invoke-Native {
    param([Parameter(Mandatory = $true)][scriptblock]$Action)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        # Out-Host keeps the tool output visible without mixing it into the
        # function's return value (the exit code).
        & $Action | Out-Host
        return $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
}

function Install-WingetPackage {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [string]$Command = ""
    )

    if ($Command -and (Get-Command $Command -ErrorAction SilentlyContinue)) {
        Write-Host "[ok]      $Id ($Command already available)" -ForegroundColor DarkGray
        return
    }

    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Write-Warning "winget is not available. Install '$Id' manually, then rerun this script."
        return
    }

    Write-Host "[install] $Id" -ForegroundColor Cyan
    $code = Invoke-Native {
        & winget install --id $Id -e --silent --accept-package-agreements --accept-source-agreements --disable-interactivity
    }
    if ($code -ne 0) {
        Write-Warning "winget returned $code for $Id (it may already be installed)."
    }
    Update-SessionPath
}

if ($Elevate -and -not (Test-Admin)) {
    Write-Host "Relaunching as Administrator..." -ForegroundColor Yellow
    $forward = @()
    if ($RunTests) { $forward += "-RunTests" }
    if ($SkipBuild) { $forward += "-SkipBuild" }
    if ($SkipModels) { $forward += "-SkipModels" }
    $arguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"") + $forward
    Start-Process -FilePath "powershell.exe" -Verb RunAs -ArgumentList $arguments
    exit 0
}

Write-Host ""
Write-Host "lai development setup" -ForegroundColor Green
Write-Host "Repository: $RepoRoot"
Write-Host "Administrator: $(Test-Admin)"
Write-Host ""

Update-SessionPath

# ---------------------------------------------------------------------------
# Toolchain
# ---------------------------------------------------------------------------
Install-WingetPackage -Id "Git.Git" -Command "git"
Install-WingetPackage -Id "Rustlang.Rustup" -Command "cargo"
Install-WingetPackage -Id "GoLang.Go" -Command "go"
Install-WingetPackage -Id "Python.Python.3.12" -Command "python"
Install-WingetPackage -Id "Kitware.CMake" -Command "cmake"
Install-WingetPackage -Id "LLVM.LLVM" -Command "clang"
Install-WingetPackage -Id "ggml.llamacpp" -Command "llama-server"
Install-WingetPackage -Id "Ollama.Ollama" -Command "ollama"

# Make is optional for the Rust/Go/Python workflow. Reuse MSYS2 if present.
if (-not (Get-Command make -ErrorAction SilentlyContinue)) {
    $msysMake = "C:\msys64\usr\bin\make.exe"
    if (Test-Path $msysMake) {
        $env:Path = "C:\msys64\usr\bin;$env:Path"
        Write-Host "[ok]      make (MSYS2)" -ForegroundColor DarkGray
    } else {
        Write-Warning "make not found. It is only needed for the legacy Makefile targets; install MSYS2 if you need it."
    }
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo is still unavailable. Close this window, open a new one and rerun the script."
}
if ((Get-Command rustup -ErrorAction SilentlyContinue) -and -not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Invoke-Native { & rustup default stable } | Out-Null
    Update-SessionPath
}

# ---------------------------------------------------------------------------
# Python virtual environment
# ---------------------------------------------------------------------------
$venvPython = Join-Path $RepoRoot ".venv\Scripts\python.exe"
if (-not (Test-Path $venvPython)) {
    Write-Host "[create]  .venv" -ForegroundColor Cyan
    Push-Location $RepoRoot
    try {
        $code = Invoke-Native { & python -m venv .venv }
        if ($code -ne 0) { throw "python -m venv failed." }
    } finally {
        Pop-Location
    }
}

Write-Host "[install] Python dependencies" -ForegroundColor Cyan
$code = Invoke-Native { & $venvPython -m pip install --upgrade pip setuptools wheel }
if ($code -ne 0) { throw "pip upgrade failed." }
$code = Invoke-Native { & $venvPython -m pip install -r (Join-Path $RepoRoot "requirements-dev.txt") }
if ($code -ne 0) { throw "Python dependency installation failed." }

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
if (-not $SkipBuild) {
    Write-Host "[build]   Go gateway" -ForegroundColor Cyan
    Push-Location (Join-Path $RepoRoot "gateway-go")
    try {
        $code = Invoke-Native { & go build -o gateway.exe . }
        if ($code -ne 0) { throw "go build failed." }
    } finally {
        Pop-Location
    }

    Write-Host "[build]   Rust CLI" -ForegroundColor Cyan
    Push-Location (Join-Path $RepoRoot "lai_rust")
    try {
        $code = Invoke-Native { & cargo build }
        if ($code -ne 0) { throw "cargo build failed." }
    } finally {
        Pop-Location
    }
}

# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------
if ($RunTests) {
    Write-Host "[test]    Rust" -ForegroundColor Cyan
    Push-Location (Join-Path $RepoRoot "lai_rust")
    try {
        $code = Invoke-Native { & cargo test --no-fail-fast }
        if ($code -ne 0) { throw "cargo test failed." }
    } finally {
        Pop-Location
    }

    Write-Host "[test]    Python (legacy)" -ForegroundColor Cyan
    Push-Location $RepoRoot
    try {
        $code = Invoke-Native { & $venvPython -m pytest -q }
        if ($code -ne 0) { throw "pytest failed." }
    } finally {
        Pop-Location
    }
}

# ---------------------------------------------------------------------------
# Local models
# ---------------------------------------------------------------------------
if (-not $SkipModels) {
    $modelDir = Join-Path $RepoRoot "models"
    $gguf = @()
    if (Test-Path $modelDir) {
        $gguf = @(Get-ChildItem $modelDir -Filter *.gguf -File -ErrorAction SilentlyContinue)
    }
    if ($gguf.Count -gt 0) {
        Write-Host "[ok]      GGUF model: $($gguf[0].FullName)" -ForegroundColor DarkGray
    } elseif (Get-Command ollama -ErrorAction SilentlyContinue) {
        Write-Host "[info]    No models/*.gguf found. Ollama models can be reused:" -ForegroundColor Yellow
        Write-Host "          ollama list"
        Write-Host "          powershell -File scripts/start-llama.ps1"
    } else {
        Write-Warning "No local model found. Place a .gguf in models/ or install Ollama and pull a model."
    }
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "Setup complete." -ForegroundColor Green
Write-Host "  cargo:  $(& cargo --version 2>$null)"
Write-Host "  go:     $(& go version 2>$null)"
Write-Host "  python: $(& $venvPython --version 2>$null)"
Write-Host ""
Write-Host "Start the local stack:" -ForegroundColor Yellow
Write-Host "  powershell -ExecutionPolicy Bypass -File scripts/start-services.ps1"
Write-Host "Check everything:" -ForegroundColor Yellow
Write-Host "  powershell -ExecutionPolicy Bypass -File scripts/dev-check.ps1"
