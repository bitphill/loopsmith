if not exist "%LOOPSMITH%" (
  echo loopsmith is not at %LOOPSMITH% and not on PATH 1>&2
  echo This loop was created against a binary that has since moved. 1>&2
  echo Re-point it by editing this script, or put loopsmith on PATH. 1>&2
  set "CODE=127"
  goto :loopsmith_done
)
