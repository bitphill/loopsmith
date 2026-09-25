#!/usr/bin/env bash
# Regenerate the Markdown twin of every example, and copy the YAML into the
# web crate so `--web` can offer them.
#
# **One source.** Each example is authored once, as `<name>.yaml`. The
# `<name>.md` beside it is generated from it by `loopsmith loop convert`, and
# is not edited by hand. Before 1.0 both were hand-written, and they drifted:
# the same loop said two different things depending on which file you opened,
# and nothing anywhere noticed.
#
# **Why the copy into the crate.** The web UI's example library is compiled
# into the binary with `include_str!`, and `include_str!` can only reach files
# inside the package. `config/` lives above the crate root and is excluded from
# the published tarball, so an example served from there works in a checkout
# and 404s for every user who installed from crates.io, npm, pip, or brew.
#
# `loopsmith_web::examples::embedded_examples_match_the_source_of_truth` fails
# when this has not been run, so drift is caught by `cargo test` rather than by
# a user.
#
#   ./tools/sync-examples.sh              # regenerate and sync
#   ./tools/sync-examples.sh --check      # fail if anything is out of date
#
# `config/examples/legacy/` is not touched. Those are the 0.3 originals, kept
# as the corpus that proves the relocation table still works on real configs;
# they are compared against their 1.0 twins by
# `every_legacy_example_migrates_to_the_one_beside_it`.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$root/config/examples"
dst="$root/runtime/crates/loopsmith-web/templates/examples"

CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

# The binary, not `cargo run`: this is also run from CI after a release build,
# and a debug rebuild there costs minutes for no reason.
bin="$root/runtime/target/release/loopsmith"
[ -x "$bin" ] || bin="$root/runtime/target/debug/loopsmith"
[ -x "$bin" ] || {
  echo "no loopsmith binary; build one first:" >&2
  echo "  (cd runtime && cargo build --release --bin loopsmith)" >&2
  exit 2
}

stale=0

# ── the Markdown twins ─────────────────────────────────────────────────────
for f in "$src"/*.yaml; do
  md="${f%.yaml}.md"
  generated="$("$bin" loop convert "$f")"
  if [ "$CHECK" -eq 1 ]; then
    if [ ! -f "$md" ] || [ "$generated" != "$(cat "$md")" ]; then
      echo "stale: $(basename "$md")" >&2
      stale=$((stale + 1))
    fi
  else
    printf '%s\n' "$generated" > "$md"
  fi
done

# A `.md` whose `.yaml` is gone is an example nobody can regenerate.
for f in "$src"/*.md; do
  [ -e "$f" ] || continue
  [ -e "${f%.md}.yaml" ] && continue
  if [ "$CHECK" -eq 1 ]; then
    echo "orphan: $(basename "$f") has no .yaml" >&2
    stale=$((stale + 1))
  else
    echo "removing orphan $(basename "$f")"
    rm "$f"
  fi
done

# ── the copies compiled into the web binary ────────────────────────────────
mkdir -p "$dst"
for f in "$dst"/*.yaml; do
  [ -e "$f" ] || continue
  base="$(basename "$f")"
  [ -e "$src/$base" ] && continue
  if [ "$CHECK" -eq 1 ]; then
    echo "stale copy: $base is in the crate and not in config/examples" >&2
    stale=$((stale + 1))
  else
    echo "removing stale $base"
    rm "$f"
  fi
done

count=0
for f in "$src"/*.yaml; do
  base="$(basename "$f")"
  if [ "$CHECK" -eq 1 ]; then
    if ! cmp -s "$f" "$dst/$base"; then
      echo "stale copy: $base" >&2
      stale=$((stale + 1))
    fi
  else
    cp "$f" "$dst/"
  fi
  count=$((count + 1))
done

if [ "$CHECK" -eq 1 ]; then
  if [ "$stale" -gt 0 ]; then
    echo "$stale file(s) out of date — run ./tools/sync-examples.sh" >&2
    exit 1
  fi
  echo "$count example(s) checked, all current"
else
  echo "$count example(s): Markdown regenerated, YAML synced into templates/examples/"
fi
