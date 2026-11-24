<#
.SYNOPSIS
    Run the local development checks for lai.

.DESCRIPTION
    Default: cargo check, cargo test, go build and pytest.
    -Full adds cargo fmt --check and cargo clippy.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/dev-check.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/dev-check.ps1 -Full
#>
[CmdletBinding()]
param(
    [switch]$Full,
    [switch]$SkipRust,
    [switch]$SkipGo,
    [switch]$SkipPython
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$failures = New-Object System.Collections.Generic.List[string]

function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @($machine, $user, $env:Path) -split ";" |
        Where-Object { $_ -and $_.Trim() } |
        Select-Object -Unique
    $env:Path = $parts -join ";"
}

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Action
    )
    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    # Native tools such as cargo write progress to stderr. Windows PowerShell
    # 5.1 turns that into a terminating error when ErrorActionPreference is
    # Stop, so relax it only while the native command runs.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $Action
        $succeeded = $?
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
    if (-not $succeeded -or $exitCode -ne 0) {
        Write-Host "FAILED: $Name (exit $exitCode)" -ForegroundColor Red
        $failures.Add($Name)
    } else {
        Write-Host "OK: $Name" -ForegroundColor Green
    }
}

Update-SessionPath

if (-not $SkipRust) {
    Push-Location (Join-Path $RepoRoot "lai_rust")
    try {
        if ($Full) {
            Invoke-Step "cargo fmt --check" { & cargo fmt --all -- --check }
            Invoke-Step "cargo clippy" { & cargo clippy --all-targets }
        }
        Invoke-Step "cargo check --all-targets" { & cargo check --all-targets }
        Invoke-Step "cargo test" { & cargo test --no-fail-fast }
    } finally {
        Pop-Location
    }
}

if (-not $SkipGo) {
    Push-Location (Join-Path $RepoRoot "gateway-go")
    try {
        Invoke-Step "go build gateway" { & go build -o gateway.exe . }
    } finally {
        Pop-Location
    }
}

if (-not $SkipPython) {
    $venvPython = Join-Path $RepoRoot ".venv\Scripts\python.exe"
    if (-not (Test-Path $venvPython)) {
        Write-Host "FAILED: Python venv missing. Run scripts/setup-dev.ps1." -ForegroundColor Red
        $failures.Add("python venv")
    } else {
        Push-Location $RepoRoot
        try {
            Invoke-Step "pytest" { & $venvPython -m pytest -q }
        } finally {
            Pop-Location
        }
    }
}

Write-Host ""
if ($failures.Count -eq 0) {
    Write-Host "All checks passed." -ForegroundColor Green
    exit 0
}

Write-Host "Failed checks:" -ForegroundColor Red
$failures | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
exit 1
