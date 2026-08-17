# loopsmith installer for Windows.
#
# Invoked by ..\install.bat, which exists so a user can double-click or type one
# word rather than remembering an execution-policy incantation. Re-runnable and
# idempotent: running it twice is how you upgrade.
$ErrorActionPreference = 'Stop'

$RepoUrl    = if ($env:LOOPSMITH_REPO_URL) { $env:LOOPSMITH_REPO_URL } else { 'https://github.com/bitphill/loopsmith.git' }
$Branch     = if ($env:LOOPSMITH_BRANCH)   { $env:LOOPSMITH_BRANCH }   else { 'main' }
$InstallDir = if ($env:LOOPSMITH_HOME)     { $env:LOOPSMITH_HOME }     else { Join-Path $env:USERPROFILE '.loopsmith' }
$BinDir     = Join-Path $InstallDir 'bin'
$LogFile    = Join-Path $InstallDir 'install.log'

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Set-Content -Path $LogFile -Value '' -Encoding utf8

function Write-Log  { param($m) Write-Host "[loopsmith] $m" -ForegroundColor Cyan;   Add-Content $LogFile "[loopsmith] $m" }
function Write-Warn { param($m) Write-Host "[loopsmith] $m" -ForegroundColor Yellow; Add-Content $LogFile "[warn] $m" }
function Die        { param($m) Write-Host "[loopsmith] $m" -ForegroundColor Red;    Add-Content $LogFile "[error] $m"; exit 1 }

Write-Log "host: windows $env:PROCESSOR_ARCHITECTURE"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot  = Split-Path -Parent $ScriptDir
$Deps      = Join-Path $ScriptDir 'deps.ps1'

if (Test-Path $Deps) {
    Write-Log 'resolving dependencies'
    & $Deps
} else {
    Write-Warn 'installers\deps.ps1 not found; assuming cargo and git are already present'
}

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Die 'cargo is not on PATH after the dependency step' }
if (-not (Get-Command git   -ErrorAction SilentlyContinue)) { Die 'git is required' }

# A checkout beside this script wins when there is one, so `git clone && install`
# installs the code just cloned rather than whatever main happens to be.
if (Test-Path (Join-Path $RepoRoot 'runtime')) {
    Write-Log "building from the checkout at $RepoRoot"
    $SrcDir = $RepoRoot
} else {
    $SrcDir = Join-Path $InstallDir 'src'
    Write-Log "cloning $RepoUrl ($Branch)"
    if (Test-Path $SrcDir) { Remove-Item $SrcDir -Recurse -Force }
    git clone --depth 1 --branch $Branch $RepoUrl $SrcDir 2>&1 | Tee-Object -Append -FilePath $LogFile
}

Write-Log 'building release binary — a few minutes on a cold cache'
Push-Location (Join-Path $SrcDir 'runtime')
try {
    cargo build --release --bin loopsmith 2>&1 | Tee-Object -Append -FilePath $LogFile
    if ($LASTEXITCODE -ne 0) { Die "cargo build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

$BinSrc = Join-Path $SrcDir 'runtime\target\release\loopsmith.exe'
$BinDst = Join-Path $BinDir 'loopsmith.exe'
if (-not (Test-Path $BinSrc)) { Die "the build reported success but $BinSrc is not there" }
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item $BinSrc $BinDst -Force
Write-Log "installed $BinDst"

# The user PATH, not the machine PATH: this needs no elevation, and a tool
# installed into a home directory has no business editing a system-wide setting.
$userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
if ($userPath -notlike "*$BinDir*") {
    [Environment]::SetEnvironmentVariable('PATH', "$BinDir;$userPath", 'User')
    Write-Log "added $BinDir to your user PATH — open a new shell for it to take effect"
} else {
    Write-Log "$BinDir is already on your user PATH"
}
$env:PATH = "$BinDir;$env:PATH"

Write-Log 'done. next:'
Write-Log '  loopsmith doctor          # what this machine is, and what that stops you doing'
Write-Log '  loopsmith new --path %USERPROFILE%\loops\my-loop --purpose "..."'
