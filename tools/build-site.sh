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

# Where the site will be served from, with no trailing slash.
#
# This is the whole of "CNAME-ready". The pages carry `__SITE_URL__` rather
# than an address, because a canonical URL, two OpenGraph tags, a Twitter card,
# the robots Sitemap line and two sitemap entries all have to say the same
# thing — and a move that updates five of those six leaves every crawler
# pointed at the address the site just left.
#
# To move: add `site/CNAME` with the domain, and set SITE_URL to match.
SITE_URL="${SITE_URL:-https://bitphill.github.io/loopsmith}"

rm -rf "$OUT"
mkdir -p "$OUT"

for f in index.html robots.txt sitemap.xml; do
  # `|` as the delimiter, because the replacement is a URL full of slashes.
  sed "s|__SITE_URL__|$SITE_URL|g" "$ROOT/site/$f" > "$OUT/$f"
done

# A placeholder that survives into the output is one nobody substituted, and
# it fails silently: the page renders, and every absolute link on it 404s.
if grep -rq "__SITE_URL__" "$OUT"; then
  echo "build-site: __SITE_URL__ was not substituted" >&2
  exit 1
fi

# The mark, at the two sizes the page asks for: the favicon and touch icon at
# 256, and the OpenGraph card at 512.
cp "$ROOT/assets/loopsmith-logo-256.png" "$OUT/"
cp "$ROOT/assets/loopsmith-logo-512.png" "$OUT/"

# GitHub Pages runs Jekyll over a branch unless told not to, and Jekyll hides
# anything beginning with an underscore.
: > "$OUT/.nojekyll"

# A custom domain is one file and it belongs to the person who owns the
# domain, so it is not generated here. `site/CNAME` is copied when it exists,
# which is the other half of the switch SITE_URL above is the first half of.
if [ -f "$ROOT/site/CNAME" ]; then
  cp "$ROOT/site/CNAME" "$OUT/"
fi

echo "site assembled -> $OUT"
