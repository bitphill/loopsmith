@echo off
rem {{purpose}}
rem
rem Paths are absolute because Task Scheduler does not inherit an interactive
rem shell's PATH. The POSIX `.sh` sibling of this file does the same job under
rem Git Bash, WSL, macOS, and Linux.
rem
rem There is exactly one `exit /b`, on the last line. `setlocal` saves the
rem errorlevel and the implicit `endlocal` restores it, so an early `exit /b`
rem reports 0 no matter what code it was given -- and `endlocal & exit /b` does
rem not help inside a nested block. Every path sets CODE and falls through.
setlocal enabledelayedexpansion
cd /d "%~dp0"
set "CODE=0"

set "LOOPSMITH={{binary}}"
if not exist "%LOOPSMITH%" (
  for /f "delims=" %%i in ('where loopsmith 2^>nul') do set "LOOPSMITH=%%i"
)
