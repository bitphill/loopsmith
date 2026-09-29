#!/usr/bin/env bash
# Build each registry's README from one body and one header.
#
#   ./tools/sync-package-readmes.sh           # write them
#   ./tools/sync-package-readmes.sh --check   # fail if any is out of date
#
# npm, PyPI and crates.io each want a README, and all three say the same thing
# about the same tool. Before 1.0 they were three ~320-line files maintained by
# hand, which is what a near-copy always becomes: the npm one still described a
# six-step web form that had been rebuilt, the PyPI one still had the 0.3
# command names, and nothing could tell you which was right.
#
# What genuinely differs per registry is the header — the badge, the install
# line, and the one paragraph explaining what this package is in a place that
# usually holds a library. That is `packaging/<target>.head.md`. Everything
# after it is `packaging/README.body.md`, shared.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# target:destination
TARGETS="
npm:npm/README.md
pypi:pypi/README.md
crates:runtime/crates/loopsmith-cli/README.md
"

CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

body="$root/packaging/README.body.md"
[ -f "$body" ] || { echo "no $body" >&2; exit 2; }

stale=0
count=0
for entry in $TARGETS; do
  target="${entry%%:*}"
  dest="$root/${entry#*:}"
  head="$root/packaging/$target.head.md"
  [ -f "$head" ] || { echo "no $head" >&2; exit 2; }

  rendered="$(cat "$head"; echo; cat "$body")"
  count=$((count + 1))

  if [ "$CHECK" -eq 1 ]; then
    if [ ! -f "$dest" ] || [ "$rendered" != "$(cat "$dest")" ]; then
      echo "stale: ${entry#*:} — run ./tools/sync-package-readmes.sh" >&2
      stale=$((stale + 1))
    fi
  else
    printf '%s\n' "$rendered" > "$dest"
    echo "wrote ${entry#*:}"
  fi
done

if [ "$CHECK" -eq 1 ]; then
  [ "$stale" -eq 0 ] || exit 1
  echo "$count package README(s) checked, all current"
fi
