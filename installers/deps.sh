#!/usr/bin/env bash
# Dependencies for building loopsmith on Linux, macOS, or BSD.
#
# Called by ../install.sh, and safe to run on its own. Installs nothing that is
# already present, and never installs a package manager — if a host has none,
# that is a decision someone made and this script says so rather than working
# around it.
set -euo pipefail

log()  { printf '\033[1;36m[deps]\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[1;33m[deps]\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[1;31m[deps]\033[0m %s\n' "$*" >&2; exit 1; }

have() { command -v "$1" >/dev/null 2>&1; }

# The package manager, not the operating system. A Debian container may have apt
# and no sudo; an Alpine one has apk; a Mac may have brew or nothing at all.
pkg_install() {
  local pkgs=("$@")
  [ ${#pkgs[@]} -eq 0 ] && return 0
  local sudo=""
  [ "$(id -u)" -ne 0 ] && have sudo && sudo="sudo"

  if   have apt-get; then $sudo apt-get update -qq && $sudo apt-get install -y "${pkgs[@]}"
  elif have dnf;     then $sudo dnf install -y "${pkgs[@]}"
  elif have yum;     then $sudo yum install -y "${pkgs[@]}"
  elif have pacman;  then $sudo pacman -Sy --noconfirm "${pkgs[@]}"
  elif have apk;     then $sudo apk add --no-cache "${pkgs[@]}"
  elif have zypper;  then $sudo zypper --non-interactive install "${pkgs[@]}"
  elif have brew;    then brew install "${pkgs[@]}"
  elif have pkg;     then $sudo pkg install -y "${pkgs[@]}"
  else
    warn "no package manager found; install these by hand: ${pkgs[*]}"
    return 1
  fi
}

missing=()
for tool in git curl; do
  have "$tool" || missing+=("$tool")
done

if [ ${#missing[@]} -gt 0 ]; then
  log "installing: ${missing[*]}"
  pkg_install "${missing[@]}" || err "could not install: ${missing[*]}"
else
  log "git and curl are present"
fi

# loopsmith links no C libraries — TLS is not used from the process at all, since
# providers are external commands — so pkg-config and openssl headers are not
# needed. Nothing is installed for them on purpose.

if have cargo; then
  log "cargo $(cargo --version | awk '{print $2}') is present"
else
  log "installing rustup with the minimal profile"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal --default-toolchain stable
fi

# Export for the caller's process, and tell the user why their shell may disagree.
if [ -d "$HOME/.cargo/bin" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
  case ":$PATH:" in
    *":$HOME/.cargo/bin:"*) ;;
    *) warn "add \$HOME/.cargo/bin to PATH in your shell profile" ;;
  esac
fi

have cargo || err "cargo is not on PATH even after installing rustup"

# 1.75 is the declared rust-version. An older toolchain fails deep in a
# dependency with a message about a syntax feature, which is not a useful clue.
ver="$(cargo --version | awk '{print $2}')"
major="${ver%%.*}"; rest="${ver#*.}"; minor="${rest%%.*}"
if [ "$major" -eq 1 ] && [ "$minor" -lt 75 ]; then
  warn "cargo $ver is older than the 1.75 loopsmith declares."
  warn "run: rustup update stable"
fi

log "dependencies satisfied"
