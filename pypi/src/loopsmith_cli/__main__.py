"""``loopsmith`` console entry point."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

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

    # Never exec ourselves. This is belt-and-braces against the one failure mode
    # that has no useful symptom: an exec loop looks exactly like a hang, with no
    # output, no error, and no traceback to read.
    try:
        if Path(exe).resolve() == Path(sys.argv[0]).resolve():
            print(
                f"[loopsmith] refusing to run {exe}: that is this launcher, not the "
                "loopsmith binary.\n"
                "[loopsmith] build from source instead:  cargo install loopsmith",
                file=sys.stderr,
            )
            return 127
    except OSError:
        pass  # An unresolvable argv[0] is not a reason to refuse to run.

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
