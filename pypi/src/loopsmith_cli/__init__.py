"""Launcher for the prebuilt ``loopsmith`` binary.

The binary is fetched on first run rather than at install time. A wheel that
downloads during ``pip install`` breaks in every environment that installs with
no network and then runs with one — CI images, Docker build stages, and locked-down
build hosts — and the failure surfaces as an install error for a package the user
has not tried to use yet.

Every download is verified against the release's published ``SHA256SUMS`` before
it is executed. Fetching a binary and running it unverified is a supply-chain
hole with a progress bar.
"""

from __future__ import annotations

import hashlib
import os
import platform
import shutil
import stat
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path

__version__ = "0.2.2"

REPO = "bitphill/loopsmith"
_RELEASE_BASE = f"https://github.com/{REPO}/releases/download/v{__version__}"


class ResolveError(RuntimeError):
    """This host has no prebuilt binary, or one could not be verified."""


def _target() -> str:
    """The Rust target triple for this interpreter's host.

    ``platform.machine()`` rather than ``platform.processor()``: the latter is
    empty on most Linux distributions and returns marketing strings on Windows.
    """
    system = platform.system().lower()
    machine = platform.machine().lower()
    arm = machine in ("arm64", "aarch64")
    intel = machine in ("x86_64", "amd64")

    if system == "linux":
        if intel:
            # musl and glibc need different binaries, and a glibc build on musl
            # dies with a bare "not found" that names no missing library.
            libc, _ = platform.libc_ver()
            flavour = "gnu" if libc else "musl"
            return f"x86_64-unknown-linux-{flavour}"
        if arm:
            return "aarch64-unknown-linux-gnu"
    elif system == "darwin":
        if arm:
            return "aarch64-apple-darwin"
        if intel:
            return "x86_64-apple-darwin"
    elif system == "windows" and intel:
        return "x86_64-pc-windows-msvc"

    raise ResolveError(
        f"no prebuilt loopsmith for {system}/{machine} at v{__version__}.\n"
        "Build from source instead:  cargo install loopsmith"
    )


def cache_dir() -> Path:
    """Where the downloaded binary is kept, versioned so an upgrade re-fetches."""
    env = os.environ.get("LOOPSMITH_HOME")
    root = Path(env) if env else Path.home() / ".loopsmith"
    return root / "bin" / __version__


def _read(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=120) as response:  # noqa: S310
        return response.read()


def _expected_digest(asset: str) -> str:
    sums = _read(f"{_RELEASE_BASE}/SHA256SUMS").decode("utf-8")
    for line in sums.splitlines():
        line = line.strip()
        if line.endswith(asset):
            return line.split()[0]
    raise ResolveError(f"{asset} is not listed in the release SHA256SUMS")


def _extract(archive: Path, into: Path, member: str) -> Path:
    into.mkdir(parents=True, exist_ok=True)
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as zf:
            zf.extract(member, path=into)
    else:
        with tarfile.open(archive, "r:gz") as tf:
            # One named member, not extractall: an archive is untrusted input
            # even when its checksum matched, and a path-traversing entry only
            # needs one careless extraction.
            info = tf.getmember(member)
            if info.name != member or info.issym() or info.islnk():
                raise ResolveError(f"unexpected entry {info.name!r} in {archive.name}")
            tf.extract(info, path=into)
    return into / member


def ensure_binary() -> Path:
    """Path to a verified ``loopsmith``, downloading it once if needed."""
    windows = platform.system().lower() == "windows"
    name = "loopsmith.exe" if windows else "loopsmith"
    destination = cache_dir() / name
    if destination.exists():
        return destination

    # There is deliberately no "reuse a `loopsmith` already on PATH" shortcut
    # here. pip installs *this* package's console script as `loopsmith` on PATH,
    # so `shutil.which("loopsmith")` finds the very script that is running and
    # `execv`ing it re-enters this function — an infinite exec loop that presents
    # as the command hanging with no output. Distinguishing our own script from a
    # cargo-installed binary means comparing against argv[0], the interpreter's
    # script directory, and every symlink in between, to save one download that
    # happens once per version. Not worth the failure mode.
    target = _target()
    asset = f"loopsmith-v{__version__}-{target}.{'zip' if windows else 'tar.gz'}"
    want = _expected_digest(asset)
    payload = _read(f"{_RELEASE_BASE}/{asset}")

    got = hashlib.sha256(payload).hexdigest()
    if got != want:
        raise ResolveError(
            f"checksum mismatch for {asset}: expected {want}, got {got}"
        )

    with tempfile.TemporaryDirectory() as tmp:
        archive = Path(tmp) / asset
        archive.write_bytes(payload)
        extracted = _extract(archive, Path(tmp) / "unpacked", name)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(extracted), str(destination))

    destination.chmod(destination.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)
    print(f"[loopsmith] installed {destination}", file=sys.stderr)
    return destination
