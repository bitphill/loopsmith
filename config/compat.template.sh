#!/bin/sh
# Portable helpers for this loop's detector scripts.
#
#   . ./scripts/compat.sh
#
# Detectors run with no shell: `command` is argv[0] and `args` are literal, so a
# detector is a real file with a real shebang. This is what it should source.
#
# Everything below detects at run time rather than being written out by the
# machine that generated it. A loop directory gets copied to a build box, a
# container, or a colleague's laptop, and a baked-in answer would be wrong on
# arrival with no sign that anything had changed.
#
# The three differences that actually break scripts:
#
#   sed -i       GNU takes no argument, BSD requires one. Getting it wrong on
#                BSD consumes the next argument as a backup suffix, which is how
#                a script ends up editing a file called `-e`.
#   stat         `-c%s` on GNU, `-f%z` on BSD.
#   readlink -f  Absent from BSD readlink before macOS 12.
#
# And the one that breaks them before they start: macOS ships bash 3.2.57,
# because 4.0 changed licence. Associative arrays, ${x,,}, mapfile, &>>, and **
# all arrived in 4.0. Write to POSIX sh unless `need_bash 4` says otherwise.

# ── what is this machine ────────────────────────────────────────────────────

LOOPSMITH_OS=$(uname -s 2>/dev/null || echo unknown)
export LOOPSMITH_OS

if sed --version >/dev/null 2>&1; then
  LOOPSMITH_USERLAND=gnu
else
  LOOPSMITH_USERLAND=bsd
fi
export LOOPSMITH_USERLAND

# Major version of the bash on PATH, or 0 when there is none. Deliberately not
# `bash --version | ...` inside a subshell that could fail silently.
LOOPSMITH_BASH_MAJOR=0
if command -v bash >/dev/null 2>&1; then
  LOOPSMITH_BASH_MAJOR=$(
    bash --version 2>/dev/null | head -1 |
      sed -e 's/.*version //' -e 's/[^0-9].*//'
  )
  [ -n "$LOOPSMITH_BASH_MAJOR" ] || LOOPSMITH_BASH_MAJOR=0
fi
export LOOPSMITH_BASH_MAJOR

# ── helpers ─────────────────────────────────────────────────────────────────

# sed_i <expr> <file>...  — edit in place, either userland.
sed_i() {
  _expr=$1
  shift
  if [ "$LOOPSMITH_USERLAND" = gnu ]; then
    sed -i -e "$_expr" "$@"
  else
    sed -i '' -e "$_expr" "$@"
  fi
}

# stat_size <file> — size in bytes.
stat_size() {
  if [ "$LOOPSMITH_USERLAND" = gnu ]; then
    stat -c%s "$1"
  else
    stat -f%z "$1"
  fi
}

# stat_mtime <file> — modification time as a unix timestamp.
stat_mtime() {
  if [ "$LOOPSMITH_USERLAND" = gnu ]; then
    stat -c%Y "$1"
  else
    stat -f%m "$1"
  fi
}

# readlink_f <path> — absolute, symlink-resolved path. Falls back to a cd/pwd
# walk where `readlink -f` is missing, which is every BSD before macOS 12.
readlink_f() {
  if readlink -f . >/dev/null 2>&1; then
    readlink -f "$1"
    return
  fi
  _target=$1
  _dir=$(dirname "$_target")
  _base=$(basename "$_target")
  ( cd "$_dir" 2>/dev/null && printf '%s/%s\n' "$(pwd -P)" "$_base" )
}

# sha256 <file> — hex digest, whichever tool is installed.
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    echo "no sha256 tool on PATH (looked for sha256sum, shasum)" >&2
    return 127
  fi
}

# need_bash <major> — exit 2 with a real explanation when the shell is too old.
#
# Exiting 2 rather than 1 matters: a detector's exit code is its verdict, and
# "this machine cannot run the check" is a different fact from "the check
# failed". A gate that cannot tell them apart reports missing tooling as
# unfinished work.
need_bash() {
  _want=${1:-4}
  if [ "$LOOPSMITH_BASH_MAJOR" -lt "$_want" ]; then
    echo "this script needs bash $_want or newer; found ${LOOPSMITH_BASH_MAJOR:-none} on $LOOPSMITH_OS" >&2
    if [ "$LOOPSMITH_OS" = Darwin ]; then
      echo "macOS ships bash 3.2 for licence reasons. Install a newer one with:" >&2
      echo "  brew install bash" >&2
      echo "…or rewrite the script to POSIX sh, which is what the rest of this loop uses." >&2
    fi
    exit 2
  fi
}

# require <command>... — exit 2 when a tool the detector needs is missing.
require() {
  for _cmd in "$@"; do
    command -v "$_cmd" >/dev/null 2>&1 && continue
    echo "required command \`$_cmd\` is not on PATH" >&2
    exit 2
  done
}

# compat_report — one line, for a log or a bug report.
compat_report() {
  printf 'os=%s userland=%s bash=%s sh=%s\n' \
    "$LOOPSMITH_OS" "$LOOPSMITH_USERLAND" "$LOOPSMITH_BASH_MAJOR" \
    "$(command -v sh)"
}
