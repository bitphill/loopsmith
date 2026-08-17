@echo off
rem loopsmith installer for Windows.
rem
rem This exists so the install is one word rather than an execution-policy
rem incantation. It delegates to installers\install.ps1 with the policy scoped to
rem this one process, which changes nothing about the machine.
setlocal
cd /d "%~dp0"

where powershell >nul 2>&1
if errorlevel 1 (
  echo powershell was not found on PATH. 1>&2
  echo loopsmith needs it to install; Windows 7 and later ship with it. 1>&2
  exit /b 127
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0installers\install.ps1" %*
endlocal & exit /b %errorlevel%
