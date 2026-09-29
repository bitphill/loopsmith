#!/bin/sh
# {{purpose}}
#
# POSIX sh on purpose: macOS ships bash 3.2, so anything needing bash 4 syntax
# would fail there. Paths are absolute because cron and launchd do not inherit
# your shell's PATH.
set -eu
cd "$(dirname "$0")"

LOOPSMITH="{{binary}}"
if [ ! -x "$LOOPSMITH" ]; then
  if command -v loopsmith >/dev/null 2>&1; then
    LOOPSMITH=$(command -v loopsmith)
  else
    echo "loopsmith is not at $LOOPSMITH and not on PATH" >&2
    echo "This loop was created against a binary that has since moved." >&2
    echo "Re-point it by editing this script, or put loopsmith on PATH." >&2
    exit 127
  fi
fi
