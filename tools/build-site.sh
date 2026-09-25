#!/usr/bin/env bash
# Assemble the project site into one directory.
#
# `site/` holds the pages and `assets/` holds the artwork, and the site needs
# both. Copying the logos into `site/` and committing them there would be a
# second copy of a 300 KB binary that nothing keeps in step with the first, so
# the join happens here instead — once, used by both the publisher
# (`publish-wiki.sh`) and the Lighthouse check in CI.
#
# What this does NOT do is touch `wiki/`. That directory is generated from the
# code index and published beside this; the two are assembled separately and
# land in the same place.
#
#   ./tools/build-site.sh [outdir]       # default: build/site
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/build/site}"

rm -rf "$OUT"
mkdir -p "$OUT"

cp "$ROOT/site/index.html" "$OUT/"
cp "$ROOT/site/robots.txt" "$OUT/"
cp "$ROOT/site/sitemap.xml" "$OUT/"

# The mark, at the two sizes the page asks for: the favicon and touch icon at
# 256, and the OpenGraph card at 512.
cp "$ROOT/assets/loopsmith-logo-256.png" "$OUT/"
cp "$ROOT/assets/loopsmith-logo-512.png" "$OUT/"

# GitHub Pages runs Jekyll over a branch unless told not to, and Jekyll hides
# anything beginning with an underscore.
: > "$OUT/.nojekyll"

# A custom domain is one file and it belongs to the person who owns the
# domain, so it is not generated here. `site/CNAME` is copied when it exists,
# which is how the switch is made: add that file, and change the absolute URLs
# in `index.html`, `robots.txt` and `sitemap.xml` to match.
if [ -f "$ROOT/site/CNAME" ]; then
  cp "$ROOT/site/CNAME" "$OUT/"
fi

echo "site assembled -> $OUT"
