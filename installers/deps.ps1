# Dependencies for building loopsmith on Windows.
#
# Called by ..\install.bat, and safe to run on its own. Installs nothing already
# present, and never installs a package manager.
$ErrorActionPreference = 'Stop'

function Write-Log  { param($m) Write-Host "[deps] $m" -ForegroundColor Cyan }
function Write-Warn { param($m) Write-Host "[deps] $m" -ForegroundColor Yellow }
function Die        { param($m) Write-Host "[deps] $m" -ForegroundColor Red; exit 1 }

function Test-Have { param($c) $null -ne (Get-Command $c -ErrorAction SilentlyContinue) }

# winget ships with Windows 11 and recent 10; choco is the common alternative.
# Neither is installed here — a host with no package manager is a decision
# somebody made, and this reports it instead of working around it.
function Install-Pkg {
    param([string]$WingetId, [string]$ChocoId, [string]$Label)
    if (Test-Have winget) {
        Write-Log "installing $Label via winget"
        winget install --id $WingetId --accept-source-agreements --accept-package-agreements -h
    } elseif (Test-Have choco) {
        Write-Log "installing $Label via choco"
        choco install $ChocoId -y
    } else {
        Die "$Label is missing and neither winget nor choco is available. Install $Label by hand."
    }
}

if (Test-Have git) { Write-Log 'git is present' } else { Install-Pkg 'Git.Git' 'git' 'git' }

if (Test-Have cargo) {
    Write-Log "cargo $((cargo --version).Split(' ')[1]) is present"
} else {
    Write-Log 'installing the Rust toolchain'
    if (Test-Have winget) {
        # Rustup.Rustup pulls the MSVC toolchain, which needs the Visual Studio
        # C++ build tools for the linker. Cargo says so clearly if they are
        # absent, which is a better failure than a silent link error.
        winget install --id Rustup.Rustup --accept-source-agreements --accept-package-agreements -h
    } else {
        $exe = Join-Path $env:TEMP 'rustup-init.exe'
        Write-Log 'downloading rustup-init.exe'
        Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $exe -UseBasicParsing
        & $exe -y --profile minimal --default-toolchain stable
        Remove-Item $exe -Force -ErrorAction SilentlyContinue
    }
}

# A fresh rustup install is not on this process's PATH yet.
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }

if (-not (Test-Have cargo)) {
    Die "cargo is not on PATH even after installing rustup. Open a new shell and re-run, or add $cargoBin to PATH."
}

# 1.75 is the declared rust-version. An older toolchain fails deep inside a
# dependency with a message about a syntax feature, which is not a useful clue.
$ver = (cargo --version).Split(' ')[1]
$parts = $ver.Split('.')
if ([int]$parts[0] -eq 1 -and [int]$parts[1] -lt 75) {
    Write-Warn "cargo $ver is older than the 1.75 loopsmith declares. Run: rustup update stable"
}

if (-not (Test-Have link.exe)) {
    Write-Warn 'link.exe was not found. The MSVC toolchain needs the Visual Studio C++ build tools:'
    Write-Warn '  winget install --id Microsoft.VisualStudio.2022.BuildTools'
    Write-Warn 'Select "Desktop development with C++" in the installer.'
}

Write-Log 'dependencies satisfied'
