#!/usr/bin/env bash
# Copy config/examples/*.yaml into the CLI crate so `--web` can offer them.
#
# The web UI's example library is compiled into the binary with `include_str!`,
# and `include_str!` can only reach files inside the package. `config/` lives
# above the crate root and is excluded from the published tarball, so an
# example served from there works in a checkout and 404s for every user who
# installed from crates.io, npm, pip, or brew.
#
# `web::examples::embedded_examples_match_the_source_of_truth` fails when this
# has not been run, so drift is caught by `cargo test` rather than by a user.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$root/config/examples"
dst="$root/runtime/crates/loopsmith-cli/templates/examples"

mkdir -p "$dst"
# Remove copies whose source is gone, so a deleted example does not linger in
# the binary forever.
for f in "$dst"/*.yaml; do
  [ -e "$f" ] || continue
  base="$(basename "$f")"
  [ -e "$src/$base" ] || { echo "removing stale $base"; rm "$f"; }
done

count=0
for f in "$src"/*.yaml; do
  cp "$f" "$dst/"
  count=$((count + 1))
done
echo "synced $count example(s) into templates/examples/"
