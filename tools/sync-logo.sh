#!/usr/bin/env bash
# Regenerate the web UI's copy of the logo from assets/loopsmith-logo-256.png.
#
# Two reasons this is a copy rather than a reference:
#
#  1. `include_bytes!` cannot reach above the package root, and `assets/` is
#     excluded from the published tarball. A mark served from there works in a
#     checkout and 404s for everyone who installed from a registry.
#  2. The published logo is RGB on a flat near-white field. That is right for a
#     README rendered on GitHub and wrong for a dark UI, where it becomes a
#     white tile. The copy has that field keyed to alpha.
#
# Pure stdlib on purpose: this machine has neither Pillow nor ImageMagick, and
# adding a build-time image dependency to a Rust CLI to move one file is a bad
# trade. `sips` cannot key alpha.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 tools/key-logo-alpha.py
