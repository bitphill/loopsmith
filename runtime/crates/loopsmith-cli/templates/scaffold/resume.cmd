{{header}}{{missing_binary}}if "%~1"=="" (
  echo usage: resume.cmd ^<run-id^> 1>&2
  echo The run id is printed at the end of every run and names the file in logs\. 1>&2
  echo recent runs: 1>&2
  for /f "delims=" %%f in ('dir /b /o-d logs\*.log 2^>nul') do @echo   %%~nf 1>&2
  set "CODE=2"
  goto :loopsmith_done
)
"%LOOPSMITH%" run resume "{{config_file}}" "%~1"
set "CODE=!ERRORLEVEL!"
:loopsmith_done
endlocal & exit /b %CODE%
