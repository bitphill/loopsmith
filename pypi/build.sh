#!/usr/bin/env bash
# Build (and optionally upload) the loopsmith-cli distribution.
#
#   ./build.sh              # build sdist + wheel into dist/, then twine check
#   ./build.sh --upload     # ...and upload to PyPI, using PYPI_TOKEN
#
# Why this exists rather than a bare `python3 -m build`: the interpreter first on
# PATH is not a stable fact. Installing anything through Homebrew can put a new
# `python3` ahead of the one whose user site-packages holds `build` and `twine`,
# and the failure reads as "No module named build" on a machine where build is
# very much installed. Homebrew's Python is also PEP 668 externally-managed, so
# `pip install --user` into it is refused outright.
#
# A throwaway venv sidesteps all of it: the tools are installed where this script
# can see them, pinned, and nothing on the host is touched.
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

VENV=".venv-build"
UPLOAD=0
[ "${1:-}" = "--upload" ] && UPLOAD=1

log() { printf '\033[1;36m[pypi]\033[0m %s\n' "$*" >&2; }
err() { printf '\033[1;31m[pypi]\033[0m %s\n' "$*" >&2; exit 1; }

# Any Python that can create a venv will do; 3.8 is the floor the package
# declares. Preferring an explicit list over bare `python3` keeps the choice from
# depending on PATH order.
pick_python() {
  for candidate in python3.13 python3.12 python3.11 python3.10 python3.9 python3 /usr/bin/python3; do
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import venv' >/dev/null 2>&1; then
      echo "$candidate"
      return 0
    fi
  done
  return 1
}

PY="$(pick_python)" || err "no Python with the venv module was found"
log "using $PY ($("$PY" --version 2>&1))"

if [ ! -x "$VENV/bin/python" ]; then
  log "creating $VENV"
  "$PY" -m venv "$VENV"
fi

log "installing build tooling into $VENV"
"$VENV/bin/python" -m pip install --quiet --upgrade pip
"$VENV/bin/python" -m pip install --quiet --upgrade build twine

rm -rf dist build src/*.egg-info
log "building sdist and wheel"
"$VENV/bin/python" -m build

log "checking metadata renders on PyPI"
"$VENV/bin/python" -m twine check dist/*

ls -la dist/

if [ "$UPLOAD" -eq 1 ]; then
  [ -n "${PYPI_TOKEN:-}" ] || err "PYPI_TOKEN is not set (it lives in ~/.profile, which zsh does not read — \`. ~/.profile\` first)"
  log "uploading to PyPI"
  TWINE_USERNAME=__token__ TWINE_PASSWORD="$PYPI_TOKEN" \
    "$VENV/bin/python" -m twine upload dist/*
  log "done"
else
  log "not uploaded. To publish: ./build.sh --upload"
fi
