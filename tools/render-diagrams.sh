#!/usr/bin/env bash
# Render assets/architecture.mmd into the two forms everything else uses.
#
#   ./tools/render-diagrams.sh            # render the PNG and the text
#   ./tools/render-diagrams.sh --check    # fail if either is out of date
#   ./tools/render-diagrams.sh --text     # text only, no Node required
#
# The `.mmd` is the source. Before 1.0 the PNG was drawn once and the same
# diagram was typed by hand into three documents, which is four copies and no
# way to tell which was current.
#
# **The text form is the one CI checks**, because it is deterministic: the same
# input gives the same bytes on every machine. A PNG is not — mermaid-cli
# renders through a headless browser, and font hinting differs between hosts,
# so comparing image bytes would fail for reasons that have nothing to do with
# the diagram. Instead the render writes `assets/architecture.sha256`, holding
# the hash of the `.mmd` the PNG was made from, and the check compares that.
# It cannot prove the PNG is correct; it proves nobody edited the source and
# forgot the picture, which is the failure that actually happens.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/assets/architecture.mmd"
PNG="$ROOT/assets/architecture.png"
TXT="$ROOT/assets/architecture.txt"
SHA="$ROOT/assets/architecture.sha256"

MODE=render
case "${1:-}" in
  --check) MODE=check ;;
  --text)  MODE=text ;;
  "")      ;;
  *) echo "unknown option: $1" >&2; exit 2 ;;
esac

[ -f "$SRC" ] || { echo "no $SRC" >&2; exit 2; }

# `shasum -a 256` on macOS, `sha256sum` on Linux. The installers' compat helper
# is not available here, and this is the only place that needs it.
hash_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

render_text() {
  SRC="$SRC" python3 - <<'PY'
import os, re, sys

src = open(os.environ['SRC'], encoding='utf-8').read()

# Mermaid entities, because a label cannot carry a raw angle bracket.
def unescape(s):
    for a, b in (('&lt;', '<'), ('&gt;', '>'), ('&amp;', '&'), ('&quot;', '"')):
        s = s.replace(a, b)
    return s

planes, current = [], None
for line in src.split('\n'):
    line = line.strip()
    if line.startswith('%%'):
        continue
    m = re.match(r'subgraph\s+\w+\["(.+?)"\]', line)
    if m:
        current = {'title': unescape(m.group(1)), 'nodes': []}
        planes.append(current)
        continue
    if line == 'end':
        current = None
        continue
    m = re.match(r'\w+\["(.+?)"\]', line)
    if m and current is not None:
        label = unescape(m.group(1))
        name, _, what = label.partition(':')
        current['nodes'].append((name.strip(), what.strip()))

if not planes:
    sys.exit('architecture.mmd: no subgraphs found; the text renderer reads those')

# Two columns, so the names line up and the descriptions read as prose rather
# than as ragged tails. The width is per plane: one plane names eleven crates
# and another names six model vendors on one line, and sharing a width between
# them pushes the crate descriptions forty columns to the right.
out = []
for i, plane in enumerate(planes):
    if i:
        out.append('       │')
    out.append(plane['title'])
    width = max(len(n) for n, _ in plane['nodes'])
    last = len(plane['nodes']) - 1
    for j, (name, what) in enumerate(plane['nodes']):
        stem = '  ' if last == 0 else ('└─' if j == last else '├─')
        out.append(f"  {stem} {name.ljust(width)}  {what}".rstrip())

sys.stdout.write('\n'.join(out) + '\n')
PY
}

case "$MODE" in
  text)
    render_text > "$TXT"
    echo "wrote $TXT"
    ;;

  check)
    stale=0
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    render_text > "$tmp"
    if ! cmp -s "$tmp" "$TXT"; then
      echo "::error file=assets/architecture.txt::out of date — run ./tools/render-diagrams.sh" >&2
      diff -u "$TXT" "$tmp" | head -40 >&2 || true
      stale=1
    fi
    if [ ! -f "$SHA" ] || [ "$(cat "$SHA")" != "$(hash_of "$SRC")" ]; then
      echo "::error file=assets/architecture.png::the diagram source changed and the PNG was not re-rendered — run ./tools/render-diagrams.sh" >&2
      stale=1
    fi
    [ "$stale" -eq 0 ] && echo "diagrams are current"
    exit "$stale"
    ;;

  render)
    render_text > "$TXT"
    echo "wrote $TXT"

    # mermaid-cli renders through puppeteer, which wants its own copy of
    # headless Chrome. This machine already has one — Playwright's, fetched for
    # the web e2e tests — and downloading a second 150 MB browser to draw one
    # picture is not a reasonable thing to ask of anyone. Any of these will do;
    # the first that exists wins.
    if [ -z "${PUPPETEER_EXECUTABLE_PATH:-}" ]; then
      for candidate in \
        "$HOME"/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-*/chrome-headless-shell \
        "$HOME"/.cache/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-*/chrome-headless-shell \
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
        /usr/bin/google-chrome \
        /usr/bin/chromium \
        /usr/bin/chromium-browser
      do
        if [ -x "$candidate" ]; then
          export PUPPETEER_EXECUTABLE_PATH="$candidate"
          break
        fi
      done
    fi
    [ -n "${PUPPETEER_EXECUTABLE_PATH:-}" ] && \
      echo "rendering with $PUPPETEER_EXECUTABLE_PATH"

    # `npx -y` so this needs nothing installed globally, and a clear message
    # rather than a stack trace when there is no network to fetch it with.
    if ! npx -y @mermaid-js/mermaid-cli@11 -i "$SRC" -o "$PNG" \
         --backgroundColor white --width 1600 >/dev/null 2>&1; then
      echo "could not render the PNG." >&2
      echo "mermaid-cli needs Node, a network to fetch it, and a Chrome to draw" >&2
      echo "with. Set PUPPETEER_EXECUTABLE_PATH if you have one somewhere else." >&2
      echo "The text form is written; re-run this when the rest is available." >&2
      exit 1
    fi
    echo "wrote $PNG"

    hash_of "$SRC" > "$SHA"
    echo "wrote $SHA"
    ;;
esac
