#!/usr/bin/env bash
# Publish the project site and the generated code wiki to the places that
# serve them.
#
#   ./tools/publish-wiki.sh              # publish to GitHub Pages and the Wiki tab
#   ./tools/publish-wiki.sh --dry-run    # show what would change, push nothing
#   ./tools/publish-wiki.sh --pages-only # only the gh-pages branch
#   ./tools/publish-wiki.sh --wiki-only  # only the repository Wiki tab
#
# There are two sources. `gitnexus wiki loops` writes `.gitnexus/wiki/` — an
# HTML viewer plus one Markdown page per subsystem, derived from the code and
# regenerated wholesale. `wiki/` in this repository holds the pages a generator
# cannot write: the architecture rationale, the command surface, the migration
# guide. Both are published together, and a name collision between them is an
# error rather than a silent overwrite.
#
# `.gitnexus/` is gitignored, so publishing is a separate step, and it has to
# happen twice because the two surfaces want the same content in different
# shapes:
#
#   gh-pages branch          -> https://bitphill.github.io/loopsmith/wiki/
#   repository Wiki tab      -> https://github.com/bitphill/loopsmith/wiki
#
# The gh-pages root gets the landing page too, assembled by
# `tools/build-site.sh` out of `site/` and `assets/`.
#
# The Pages copy is verbatim: the viewer is a single self-contained HTML file.
#
# The Wiki tab is not. A GitHub wiki addresses pages by name, not by filename,
# so `gating-success-criteria.md` has to become the page `Gating-Success-Criteria`
# and every `](gating-success-criteria.md)` in every page has to be rewritten to
# match. Copy the files across unchanged and each page renders fine while every
# cross-reference in it 404s — which is the failure mode worth automating away,
# because it looks like success until someone clicks.
#
# The project's front door is the site, not the Wiki tab. GitHub offers no way
# to point that tab elsewhere — no setting, and a wiki page cannot redirect,
# because the renderer strips scripts and meta tags. So the tab stays, as a
# signpost: every page opens with a banner to the site, and so does Home.
# The banner is written into each page rather than into `_Header.md`: GitHub
# renders a wiki's `_Sidebar.md` and `_Footer.md` but silently ignores a header. The pages themselves stay too, because links to them already
# exist in the READMEs, the crates, and the release notes.
#
# Neither surface is versioned with the release tags. Both always show the
# newest content. That is why the READMEs link to them rather than duplicating
# them: a README is pinned per version and must not disagree with itself.
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$PWD"
SRC="$ROOT/.gitnexus/wiki"
HAND="$ROOT/wiki"
SITE="https://bitphill.github.io/loopsmith/"
VIEWER="${SITE}wiki/"
WIKI_REMOTE="https://github.com/bitphill/loopsmith.wiki.git"

DRY=0; DO_PAGES=1; DO_WIKI=1
for arg in "$@"; do
  case "$arg" in
    --dry-run)    DRY=1 ;;
    --pages-only) DO_WIKI=0 ;;
    --wiki-only)  DO_PAGES=0 ;;
    -h|--help)    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

log() { printf '\033[1;36m[wiki]\033[0m %s\n' "$*" >&2; }
err() { printf '\033[1;31m[wiki]\033[0m %s\n' "$*" >&2; exit 1; }

[ -d "$SRC" ] || err "no wiki at $SRC — generate it first:
  gitnexus wiki loops
Answer 'n' to the gist prompt; this script publishes it instead."
[ -f "$SRC/index.html" ] || err "$SRC has no index.html — the generation did not finish"

TMP="$(mktemp -d)"
cleanup() {
  # The worktree has to be unregistered, not just deleted, or `git worktree
  # list` keeps reporting a directory that is gone.
  git -C "$ROOT" worktree remove --force "$TMP/pages" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT

# ── GitHub Pages ───────────────────────────────────────────────────────────
if [ "$DO_PAGES" -eq 1 ]; then
  log "gh-pages: preparing"
  git -C "$ROOT" fetch -q origin gh-pages
  git -C "$ROOT" worktree add -q --detach "$TMP/pages" origin/gh-pages
  git -C "$TMP/pages" checkout -q -B gh-pages origin/gh-pages

  rm -rf "$TMP/pages/wiki"
  mkdir -p "$TMP/pages/wiki"
  cp -R "$SRC/." "$TMP/pages/wiki/"

  # The hand-written pages go beside the generated ones rather than into them:
  # the viewer compiles its content into `index.html`, so there is no way to
  # add a page to it without regenerating it. They are served as Markdown and
  # linked from the landing page and from the wiki tab, which is where prose
  # gets read anyway.
  for f in "$HAND"/*.md; do
    [ -e "$f" ] || continue
    base="$(basename "$f")"
    # `[ … ] && err` would be a failing command under `set -e` on every run
    # where there is no collision, which is every run.
    if [ -e "$SRC/$base" ]; then
      err "wiki/$base and the generated wiki both claim that name"
    fi
    cp "$f" "$TMP/pages/wiki/$base"
  done

  # The landing page, beside the wiki rather than instead of it. The root of
  # gh-pages used to be a meta-refresh into `wiki/`, which meant the project's
  # own address was a redirect to its generated API documentation — the least
  # useful page it has for anyone arriving without context.
  "$ROOT/tools/build-site.sh" "$TMP/site" >/dev/null
  cp -R "$TMP/site/." "$TMP/pages/"

  if [ -z "$(git -C "$TMP/pages" status --porcelain)" ]; then
    log "gh-pages: already current"
  elif [ "$DRY" -eq 1 ]; then
    log "gh-pages: would publish"
    git -C "$TMP/pages" status --short | sed 's/^/    /'
  else
    git -C "$TMP/pages" add -A
    git -C "$TMP/pages" commit -q -m "Regenerate the site and the code wiki

The landing page from site/, the wiki from \`gitnexus wiki\`. Published by
tools/publish-wiki.sh."
    git -C "$TMP/pages" push -q origin gh-pages
    log "gh-pages: published -> $VIEWER"
  fi
fi

# ── Repository Wiki tab ────────────────────────────────────────────────────
if [ "$DO_WIKI" -eq 1 ]; then
  log "wiki tab: cloning"
  if ! git clone -q "$WIKI_REMOTE" "$TMP/wiki" 2>/dev/null; then
    err "cannot clone $WIKI_REMOTE

A repository's wiki does not exist until its first page is saved through the
web UI, and GitHub exposes no API for that step. Open
  https://github.com/bitphill/loopsmith/wiki
click 'Create the first page', save anything at all, then re-run this script —
it overwrites that page on the first push."
  fi

  find "$TMP/wiki" -maxdepth 1 -name '*.md' -delete

  SRC="$SRC" HAND="$HAND" DST="$TMP/wiki" VIEWER="$VIEWER" SITE="$SITE" python3 - <<'PY'
import os, re

src, dst, viewer = os.environ['SRC'], os.environ['DST'], os.environ['VIEWER']
site = os.environ['SITE']
banner = (f"> 📖 **loopsmith's documentation lives at "
          f"[{site.split('//')[1].rstrip('/')}]({site}) →** "
          "This wiki mirrors it so existing links keep working.\n")
hand = os.environ['HAND']

# Two sources, one set of pages. A generated page is named after the subsystem
# it describes (`gating-success-criteria.md`); a hand-written one is already
# named as a wiki page (`Migration-0-3-To-1-0.md`), because that is the name it
# is cross-linked by from the READMEs and from the other pages.
gen = [f for f in sorted(os.listdir(src)) if f.endswith('.md')]
own = ([f for f in sorted(os.listdir(hand)) if f.endswith('.md')]
       if os.path.isdir(hand) else [])
mds = gen + own
where = {f: src for f in gen}
where.update({f: hand for f in own})

def page_name(stem):
    # `overview` is the entry point, and a GitHub wiki's entry point is `Home`.
    if stem == 'overview':
        return 'Home'
    return '-'.join(w.capitalize() if w.islower() else w for w in stem.split('-'))

def page_of(f):
    # A hand-written file is already named as its page; leaving it alone is
    # what keeps `](Migration-0-3-To-1-0)` resolving from outside the wiki.
    return f[:-3] if where[f] is hand else page_name(f[:-3])

stem2page = {f[:-3]: page_of(f) for f in mds}

def heading(page, text):
    m = re.search(r'^#\s+(.+)$', text, re.M)
    return m.group(1).strip() if m else page.replace('-', ' ')

titles = {}
for f in mds:
    stem = f[:-3]
    page = stem2page[stem]
    text = open(os.path.join(where[f], f)).read()
    # `](foo-bar.md)` addresses a file. A wiki addresses a page. The character
    # class has to include capitals: the generated pages are lower-case, but the
    # hand-written ones in `wiki/` are named the way they are titled, and a link
    # this misses is not an error anywhere — it is a dead link on a published
    # page, which is the kind of mistake that survives.
    text = re.sub(r'\]\(([A-Za-z0-9._-]+)\.md\)',
                  lambda m: '](%s)' % stem2page.get(m.group(1), m.group(1)), text)
    if stem == 'overview':
        # The generator's own banner heading duplicates the project title below it.
        text = re.sub(r'^# loops — Wiki\n+', '', text, count=1)
    titles[page] = heading(page, text)
    if stem != 'overview':
        text = (banner + "\n"
                f"[← Wiki home](Home) · [Rendered viewer with search]({viewer})\n\n---\n\n" + text)
    open(os.path.join(dst, page + '.md'), 'w').write(text)

pages = [p for p in sorted(stem2page.values()) if p != 'Home']
index = "\n".join(f"- [{titles[p]}]({p})" for p in pages)

home = os.path.join(dst, 'Home.md')
body = open(home).read()
open(home, 'w').write(
    f"# [loopsmith's documentation lives at {site.split('//')[1].rstrip('/')} →]({site})\n\n"
    f"**[Open the site]({site})** for the introduction, installation, and the\n"
    f"[code wiki with search]({viewer}). The pages below are the same content, kept\n"
    "here so that links already pointing at them still work.\n\n"
    "## All pages\n\n" + index + "\n\n---\n\n" + body + "\n")

open(os.path.join(dst, '_Sidebar.md'), 'w').write(
    f"**[loopsmith]({site})**\n\n"
    f"[Documentation site]({site}) · [Code wiki]({viewer}) · "
    "[Repository](https://github.com/bitphill/loopsmith)\n\n"
    "---\n\n[Home](Home)\n\n" + index + "\n")

open(os.path.join(dst, '_Footer.md'), 'w').write(
    "Generated by `gitnexus wiki` from "
    "[bitphill/loopsmith](https://github.com/bitphill/loopsmith). "
    "Do not edit by hand — regenerated on each run.\n")

# A link to a page that does not exist renders as ordinary text on GitHub, so
# nothing complains at push time. Catch it here instead.
broken = sorted({t for t in re.findall(r'\]\(([A-Z][A-Za-z0-9-]*)\)',
                                       "".join(open(os.path.join(dst, f)).read()
                                               for f in os.listdir(dst) if f.endswith('.md')))
                 if not os.path.exists(os.path.join(dst, t + '.md'))})
if broken:
    raise SystemExit("cross-links with no page: " + ", ".join(broken))

print(f"    {len(gen)} generated + {len(own)} hand-written pages, "
      f"{len(pages)} cross-link targets, all resolved")
PY

  if [ -z "$(git -C "$TMP/wiki" status --porcelain)" ]; then
    log "wiki tab: already current"
  elif [ "$DRY" -eq 1 ]; then
    log "wiki tab: would publish"
    git -C "$TMP/wiki" status --short | sed 's/^/    /'
  else
    git -C "$TMP/wiki" add -A
    git -C "$TMP/wiki" -c user.email=bitphill@users.noreply.github.com \
        -c user.name=bitphill commit -q -m "Regenerate the code wiki

Output of \`gitnexus wiki\`, reshaped for wiki page addressing by
tools/publish-wiki.sh."
    git -C "$TMP/wiki" push -q origin HEAD
    log "wiki tab: published -> https://github.com/bitphill/loopsmith/wiki"
  fi
fi

[ "$DRY" -eq 1 ] && log "dry run: nothing was pushed"
exit 0
