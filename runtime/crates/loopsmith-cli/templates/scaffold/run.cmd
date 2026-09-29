{{header}}{{missing_binary}}"%LOOPSMITH%" run start "{{config_file}}" %*
set "CODE=!ERRORLEVEL!"
:loopsmith_done
endlocal & exit /b %CODE%
