#!/usr/bin/env bash
# loopsmith universal installer — Linux, macOS, BSD.
#
# Detects the host, installs what is missing via the native package manager,
# builds the release binary, and puts it somewhere on PATH. Re-runnable and
# idempotent: running it twice is how you upgrade.
#
# On Windows, run install.bat instead. This script needs bash; the *generated*
# loop scripts are POSIX sh, which is a different requirement — an installer runs
# once on a machine you are sitting at, and a loop script runs unattended on a
# machine nobody chose.
set -euo pipefail

REPO_URL="${LOOPSMITH_REPO_URL:-https://github.com/bitphill/loopsmith.git}"
INSTALL_DIR="${LOOPSMITH_HOME:-$HOME/.loopsmith}"
BIN_LINK_DIR="${LOOPSMITH_BIN_DIR:-/usr/local/bin}"
BRANCH="${LOOPSMITH_BRANCH:-main}"
LOG="$INSTALL_DIR/install.log"

log()  { printf '\033[1;36m[loopsmith]\033[0m %s\n' "$*" | tee -a "$LOG" >&2; }
warn() { printf '\033[1;33m[loopsmith]\033[0m %s\n' "$*" | tee -a "$LOG" >&2; }
err()  { printf '\033[1;31m[loopsmith]\033[0m %s\n' "$*" | tee -a "$LOG" >&2; exit 1; }

detect_os() {
  case "$(uname -s | tr '[:upper:]' '[:lower:]')" in
    linux*)                    echo linux ;;
    darwin*)                   echo macos ;;
    freebsd*|openbsd*|netbsd*) echo bsd ;;
    msys*|cygwin*|mingw*)      echo windows ;;
    *)                         echo unknown ;;
  esac
}

mkdir -p "$INSTALL_DIR"
: > "$LOG"

OS="$(detect_os)"
log "host: $OS $(uname -m)"
[ "$OS" = unknown ] && err "unsupported host: $(uname -s)"
if [ "$OS" = windows ]; then
  warn "this looks like an MSYS/Cygwin shell. install.bat is the native path."
  warn "continuing, since a POSIX layer is present."
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPS_SCRIPT="$SCRIPT_DIR/installers/deps.sh"

if [ -f "$DEPS_SCRIPT" ]; then
  log "resolving dependencies"
  bash "$DEPS_SCRIPT" 2>&1 | tee -a "$LOG"
else
  warn "installers/deps.sh not found; falling back to an inline rustup install"
  if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
  fi
fi

# rustup writes its PATH line to ~/.profile, which zsh never reads, so a shell
# that has cargo installed can still not see it. Add it for this process.
[ -d "$HOME/.cargo/bin" ] && export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null 2>&1 || err "cargo is still not on PATH after dependency install"
command -v git   >/dev/null 2>&1 || err "git is required"

# A checkout beside this script is the source of truth when there is one; that is
# what makes `git clone && ./install.sh` install the code you just cloned rather
# than whatever main happens to be.
if [ -d "$SCRIPT_DIR/runtime" ]; then
  log "building from the checkout at $SCRIPT_DIR"
  SRC_DIR="$SCRIPT_DIR"
else
  log "cloning $REPO_URL ($BRANCH)"
  rm -rf "$INSTALL_DIR/src"
  git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$INSTALL_DIR/src" 2>&1 | tee -a "$LOG"
  SRC_DIR="$INSTALL_DIR/src"
fi

log "building release binary — a few minutes on a cold cache"
( cd "$SRC_DIR/runtime" && cargo build --release --bin loopsmith ) 2>&1 | tee -a "$LOG"

BIN_SRC="$SRC_DIR/runtime/target/release/loopsmith"
BIN_DST="$INSTALL_DIR/bin/loopsmith"
[ -x "$BIN_SRC" ] || err "the build reported success but $BIN_SRC is not there"
mkdir -p "$INSTALL_DIR/bin"
install -m 0755 "$BIN_SRC" "$BIN_DST"
log "installed $BIN_DST"

if [ -w "$BIN_LINK_DIR" ] || [ "$(id -u)" -eq 0 ]; then
  ln -sf "$BIN_DST" "$BIN_LINK_DIR/loopsmith"
  log "linked $BIN_LINK_DIR/loopsmith"
else
  warn "cannot write to $BIN_LINK_DIR, so nothing was linked."
  warn "add this to your shell profile:"
  warn "  export PATH=\"$INSTALL_DIR/bin:\$PATH\""
fi

log "done. next:"
log "  loopsmith doctor          # what this machine is, and what that stops you doing"
log "  loopsmith new --path ~/loops/my-loop --purpose \"...\""
