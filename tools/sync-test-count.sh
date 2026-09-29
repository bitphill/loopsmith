#!/usr/bin/env bash
# Keep the README's tests badge equal to the number of tests the suite has.
#
#   ./tools/sync-test-count.sh           # rewrite the badge to the real count
#   ./tools/sync-test-count.sh --check   # fail if the badge disagrees (CI uses this)
#
# Every other number in the README's badge row is read live by shields.io from
# the registry or the repository. This one is baked into the URL, which is how
# it came to say 415 for a suite that had grown to 620 with nothing noticing.
#
# The count comes from `cargo test --workspace -- --list`, which enumerates every
# test without running one. It depends on the platform: one test exists only on
# macOS (it writes a launch agent) and one only on Unix, so the same suite lists
# 620 on macOS, 619 on Linux, and fewer on Windows. macOS is the superset —
# every test in the codebase runs there — so the badge counts that, and CI checks
# it on the macOS leg alone. Run anywhere else, `--check` would report a drift
# that is really a platform difference, so it refuses rather than mislead.
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

if [ "$(uname -s)" != "Darwin" ]; then
  echo "the badge counts the macOS suite, which is the superset; run this on macOS" >&2
  exit 2
fi

# `grep -c` prints 0 and exits 1 on no matches, which under `pipefail` would end
# the script here with no message. `|| true` keeps the 0, and the next line says
# what it means.
count="$(cd runtime && cargo test --workspace -q -- --list 2>/dev/null | grep -c ': test$' || true)"
[ "${count:-0}" -gt 0 ] || { echo "cargo listed no tests — the build probably failed" >&2; exit 1; }

# A capture group, not a second `grep -o '[0-9]+'`: the `%20` in the URL is a
# number too, and pulling every digit run out of the match yields "415\n20".
current="$(sed -nE 's|.*badge/tests-([0-9]+)%20passing.*|\1|p' README.md | head -1)"
[ -n "$current" ] || { echo "README.md has no tests badge to check" >&2; exit 1; }

if [ "$current" = "$count" ]; then
  echo "tests badge says $count, and the suite has $count"
  exit 0
fi

if [ "$CHECK" -eq 1 ]; then
  echo "::error file=README.md::the tests badge says $current but the suite has $count — run ./tools/sync-test-count.sh"
  exit 1
fi

# Written through `cat` rather than `mv`, so README.md keeps its own permissions
# instead of inheriting mktemp's 0600.
tmp="$(mktemp)"
sed -E "s|badge/tests-[0-9]+%20passing|badge/tests-${count}%20passing|" README.md > "$tmp"
cat "$tmp" > README.md
rm -f "$tmp"
echo "tests badge: $current -> $count"
