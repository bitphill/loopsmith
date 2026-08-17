#!/usr/bin/env bash
# Propagate the workspace version to every place that has to repeat it.
#
#   ./tools/sync-version.sh           # rewrite everything to match runtime/Cargo.toml
#   ./tools/sync-version.sh --check   # fail if anything is out of step (CI uses this)
#
# `runtime/Cargo.toml`'s `[workspace.package] version` is the single source of
# truth. Everything else — the npm package, the PyPI distribution, its
# `__version__`, the Homebrew formula's tag, and the tag-pinned logo URL in each
# published README — is derived. Hand-editing eight files per release is how one
# gets missed, and the one that gets missed is usually a URL, which fails silently
# as a broken image on a registry page nobody looks at twice.
#
# Not in `scripts/`: that name is reserved for a loop's own detector scripts, and
# the repository deliberately ships none so the `pre_execution` refusal keeps its
# teaching value.
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

VERSION="$(
  awk '/^\[workspace\.package\]/{f=1} f && /^version = /{gsub(/[",]/,"",$3); print $3; exit}' runtime/Cargo.toml
)"
[ -n "$VERSION" ] || { echo "could not read the version from runtime/Cargo.toml" >&2; exit 1; }

drift=0

# Report or rewrite, depending on mode. `sed -i` spelling differs between GNU and
# BSD, so this writes to a temp file and moves it — the same reason a generated
# detector sources compat.sh instead of branching.
apply() {
  local file="$1" pattern="$2" replacement="$3" label="$4"
  [ -f "$file" ] || { echo "  missing: $file" >&2; drift=$((drift+1)); return; }
  local tmp
  tmp="$(mktemp)"
  sed -E "s|$pattern|$replacement|g" "$file" > "$tmp"
  if cmp -s "$file" "$tmp"; then
    rm -f "$tmp"
    return
  fi
  if [ "$CHECK" -eq 1 ]; then
    echo "  out of step: $file ($label)"
    drift=$((drift+1))
    rm -f "$tmp"
  else
    mv "$tmp" "$file"
    echo "  updated: $file ($label)"
  fi
}

echo "workspace version: $VERSION"

# Every crate manifest inherits the version, so only the workspace's path deps
# repeat it.
apply runtime/Cargo.toml \
  '(path = "crates/loopsmith-[a-z]+", version = ")[0-9]+\.[0-9]+\.[0-9]+(")' \
  "\\1$VERSION\\2" 'workspace path deps'

apply npm/package.json \
  '("version": ")[0-9]+\.[0-9]+\.[0-9]+(")' \
  "\\1$VERSION\\2" 'npm package'

apply pypi/pyproject.toml \
  '^(version = ")[0-9]+\.[0-9]+\.[0-9]+(")' \
  "\\1$VERSION\\2" 'PyPI distribution'

apply pypi/src/loopsmith_cli/__init__.py \
  '^(__version__ = ")[0-9]+\.[0-9]+\.[0-9]+(")' \
  "\\1$VERSION\\2" 'PyPI __version__'

# The formula's `url` names the tag. Its `sha256` cannot be derived from anything
# local — it is the digest of a tarball GitHub generates — so it stays manual and
# the release checklist covers it.
apply Formula/loopsmith.rb \
  '(archive/refs/tags/v)[0-9]+\.[0-9]+\.[0-9]+(\.tar\.gz)' \
  "\\1$VERSION\\2" 'Homebrew tag'

# The logo in every published README is pinned to the release tag rather than
# `main`, because a published README is immutable and an image URL that can move
# underneath it will eventually be wrong.
for readme in npm/README.md pypi/README.md runtime/crates/*/README.md; do
  apply "$readme" \
    '(raw\.githubusercontent\.com/bitphill/loopsmith/v)[0-9]+\.[0-9]+\.[0-9]+(/assets)' \
    "\\1$VERSION\\2" 'tag-pinned logo'
done

if [ "$CHECK" -eq 1 ]; then
  if [ "$drift" -gt 0 ]; then
    echo
    echo "$drift file(s) do not match runtime/Cargo.toml ($VERSION)." >&2
    echo "Run ./tools/sync-version.sh to fix." >&2
    exit 1
  fi
  echo "everything matches $VERSION"
else
  echo "done. Remember the formula sha256 once the tag exists:"
  echo "  curl -sL https://github.com/bitphill/loopsmith/archive/refs/tags/v$VERSION.tar.gz -o t.tar.gz"
  echo "  gzip -t t.tar.gz && wc -c < t.tar.gz && shasum -a 256 t.tar.gz"
fi
