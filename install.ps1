# loopsmith installer for Windows.
#
# Run it directly (`.\install.ps1`) or through install.bat, which exists so a
# user can double-click or type one word rather than remembering an
# execution-policy incantation. Re-runnable and idempotent: running it twice is
# how you upgrade.
#
# This sits at the repository root beside install.sh, and for the same reason:
# an installer is the first file anyone looks for, and burying it one directory
# down makes a reader wonder whether they found the right one. Its dependency
# half lives in installers\deps.ps1, mirroring installers/deps.sh.
$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot  = $ScriptDir

# Every fact this shares with install.sh comes from one file, so moving the
# repository or changing the build command is one edit rather than three.
# `-Encoding UTF8` because Windows PowerShell 5.1, which install.bat runs, reads
# a file without a byte-order mark in the ANSI code page - the manifest is UTF-8
# with no BOM, so any non-ASCII character in it would print as mojibake.
$Manifest   = Get-Content (Join-Path $ScriptDir 'installers\manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json

$RepoUrl    = if ($env:LOOPSMITH_REPO_URL) { $env:LOOPSMITH_REPO_URL } else { $Manifest.repo_url }
$Branch     = if ($env:LOOPSMITH_BRANCH)   { $env:LOOPSMITH_BRANCH }   else { $Manifest.branch }
$InstallDir = if ($env:LOOPSMITH_HOME)     { $env:LOOPSMITH_HOME }     else { Join-Path $env:USERPROFILE $Manifest.install_dir_name }
$BinDir     = Join-Path $InstallDir $Manifest.bin_subdir
$LogFile    = Join-Path $InstallDir 'install.log'

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Set-Content -Path $LogFile -Value '' -Encoding utf8

function Write-Log  { param($m) Write-Host "[loopsmith] $m" -ForegroundColor Cyan;   Add-Content $LogFile "[loopsmith] $m" }
function Write-Warn { param($m) Write-Host "[loopsmith] $m" -ForegroundColor Yellow; Add-Content $LogFile "[warn] $m" }
function Die        { param($m) Write-Host "[loopsmith] $m" -ForegroundColor Red;    Add-Content $LogFile "[error] $m"; exit 1 }

# Run a native command with everything it prints copied to the log, and die if
# it exits non-zero. The exit code is the verdict; stderr is just more output.
#
# Windows PowerShell 5.1 turns each line a native command writes to stderr into
# an error record once stderr is redirected, and under 'Stop' the first of those
# ends the script. cargo and git both report progress on stderr, so a build that
# had succeeded died on cargo's own "Finished" line. 'Continue' for the length
# of the call, and each record back to plain text so the log reads as output
# rather than as a wall of NativeCommandError.
function Invoke-Logged {
    param([string]$What, [string]$Exe, [string[]]$Arguments)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Exe @Arguments 2>&1 | ForEach-Object { "$_" } | Tee-Object -Append -FilePath $LogFile
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
    if ($code -ne 0) { Die "$What failed with exit code $code" }
}

Write-Log "host: windows $env:PROCESSOR_ARCHITECTURE"

$Deps = Join-Path $ScriptDir 'installers\deps.ps1'

if (Test-Path $Deps) {
    Write-Log 'resolving dependencies'
    & $Deps
} else {
    Write-Warn 'installers\deps.ps1 not found; assuming cargo and git are already present'
}

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
foreach ($tool in $Manifest.requires) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Die "$tool is required and is not on PATH after the dependency step"
    }
}

# A checkout beside this script wins when there is one, so `git clone && install`
# installs the code just cloned rather than whatever main happens to be.
if (Test-Path (Join-Path $RepoRoot $Manifest.build_dir)) {
    Write-Log "building from the checkout at $RepoRoot"
    $SrcDir = $RepoRoot
} else {
    $SrcDir = Join-Path $InstallDir 'src'
    Write-Log "cloning $RepoUrl ($Branch)"
    if (Test-Path $SrcDir) { Remove-Item $SrcDir -Recurse -Force }
    Invoke-Logged -What 'git clone' -Exe 'git' -Arguments @('clone', '--depth', '1', '--branch', $Branch, $RepoUrl, $SrcDir)
}

Write-Log 'building release binary - a few minutes on a cold cache'
Push-Location (Join-Path $SrcDir $Manifest.build_dir)
try {
    Invoke-Logged -What 'cargo build' -Exe 'cargo' -Arguments $Manifest.build_args
} finally {
    Pop-Location
}

$BinSrc = Join-Path $SrcDir ($Manifest.built_windows -replace '/', '\')
$BinDst = Join-Path $BinDir ($Manifest.binary + '.exe')
if (-not (Test-Path $BinSrc)) { Die "the build reported success but $BinSrc is not there" }
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item $BinSrc $BinDst -Force
Write-Log "installed $BinDst"

# The user PATH, not the machine PATH: this needs no elevation, and a tool
# installed into a home directory has no business editing a system-wide setting.
$userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
if ($userPath -notlike "*$BinDir*") {
    [Environment]::SetEnvironmentVariable('PATH', "$BinDir;$userPath", 'User')
    Write-Log "added $BinDir to your user PATH - open a new shell for it to take effect"
} else {
    Write-Log "$BinDir is already on your user PATH"
}
$env:PATH = "$BinDir;$env:PATH"

Write-Log 'done. next:'
foreach ($step in $Manifest.next_steps) { Write-Log "  $step" }
