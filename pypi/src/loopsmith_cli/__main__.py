"""``loopsmith`` console entry point."""

from __future__ import annotations

import os
import subprocess
import sys

from . import ResolveError, ensure_binary


def main() -> int:
    try:
        exe = ensure_binary()
    except ResolveError as e:
        print(f"[loopsmith] {e}", file=sys.stderr)
        return 127
    except OSError as e:
        print(f"[loopsmith] could not fetch the binary: {e}", file=sys.stderr)
        print("[loopsmith] build from source instead:  cargo install loopsmith", file=sys.stderr)
        return 127

    argv = [str(exe), *sys.argv[1:]]

    # On POSIX, replace this process rather than wrapping it. loopsmith is
    # long-running and interactive, so a Python parent adds a process that has to
    # forward signals correctly and gets it wrong at least once. `execv` makes the
    # shell's Ctrl-C reach the real binary, and its exit code arrive unchanged.
    if os.name != "nt":
        os.execv(str(exe), argv)  # noqa: S606 — never returns

    # Windows has no exec that replaces the process, so wrap and propagate.
    try:
        return subprocess.call(argv)
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
