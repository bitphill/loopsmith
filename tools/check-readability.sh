#!/usr/bin/env bash
# Is README-FOR-DUMMIES.md still readable by the person it is named for?
#
#   ./tools/check-readability.sh                 # the default document and grade
#   ./tools/check-readability.sh FILE [GRADE]    # any document, any ceiling
#   ./tools/check-readability.sh --report FILE   # the worst sentences, no verdict
#
# The grade is Flesch-Kincaid: 0.39·(words/sentence) + 11.8·(syllables/word) −
# 15.59, which answers "how many years of schooling does reading this assume".
# It is a crude instrument — it cannot tell prose from a shipping manifest, and
# it rewards short words regardless of whether they mean anything. It is used
# here anyway, because the failure it catches is the one this document actually
# has: sentences that grew a clause at a time until nobody could follow them.
#
# What is measured is prose. Code blocks, tables, link targets, inline code and
# headings are skipped — a `no_progress_iterations` in a config listing is not
# a word the reader has to parse, and counting it would push the score up
# without making anything harder to read.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

REPORT=0
if [ "${1:-}" = "--report" ]; then REPORT=1; shift; fi
DOC="${1:-$ROOT/README-FOR-DUMMIES.md}"
MAX="${2:-6.0}"

[ -f "$DOC" ] || { echo "no such document: $DOC" >&2; exit 2; }

DOC="$DOC" MAX="$MAX" REPORT="$REPORT" python3 - <<'PY'
import os, re, sys

doc, ceiling, report = os.environ['DOC'], float(os.environ['MAX']), os.environ['REPORT'] == '1'
text = open(doc, encoding='utf-8').read()

# Code fences first: everything inside one is a listing, not a sentence.
text = re.sub(r'^```.*?^```', '', text, flags=re.M | re.S)
lines = []
for line in text.split('\n'):
    s = line.strip()
    if s.startswith('#'):            # headings are labels
        continue
    if s.startswith('|') or set(s) <= set('|-: '):   # tables
        continue
    if s.startswith('    ') or line.startswith('\t'):  # indented listings
        continue
    if s.startswith('<'):            # raw HTML, which the README opens with
        continue
    lines.append(line)
text = '\n'.join(lines)

text = re.sub(r'`[^`]*`', ' code ', text)            # inline code is one token
text = re.sub(r'\[([^\]]*)\]\([^)]*\)', r'\1', text)  # keep link text, drop the URL
text = re.sub(r'https?://\S+', ' link ', text)
text = re.sub(r'[*_>]', ' ', text)

# Markdown wraps paragraphs across lines, so a newline is usually not the end
# of anything. Rejoin each paragraph and each list item into one line, then end
# it with a full stop if the author did not — otherwise a run of six bullets is
# counted as one forty-word sentence and every per-sentence figure is nonsense.
units, current = [], []
def flush():
    if current:
        units.append(' '.join(w.strip() for w in current))
        current.clear()

for line in text.split('\n'):
    if not line.strip():
        flush()
        continue
    item = re.match(r'^\s*(?:[-*+]|\d+\.)\s+(.*)', line)
    if item:
        flush()
        current.append(item.group(1))
    else:
        current.append(line)
flush()

text = '\n'.join(u + '.' if u and u[-1] not in '.!?' else u for u in units)

VOWELS = 'aeiouy'

def syllables(word):
    word = word.lower().strip("'")
    if not word:
        return 0
    count, prev_vowel = 0, False
    for ch in word:
        vowel = ch in VOWELS
        if vowel and not prev_vowel:
            count += 1
        prev_vowel = vowel
    # A trailing silent `e` is the single biggest source of over-counting.
    if word.endswith('e') and not word.endswith(('le', 'ee')) and count > 1:
        count -= 1
    return max(count, 1)

def measure(chunk):
    sentences = [s for s in re.split(r'[.!?]+(?:\s|$)', chunk) if s.strip()]
    words = re.findall(r"[A-Za-z][A-Za-z'-]*", chunk)
    if not sentences or not words:
        return None
    syl = sum(syllables(w) for w in words)
    grade = (0.39 * len(words) / len(sentences)
             + 11.8 * syl / len(words) - 15.59)
    return grade, len(words), len(sentences), syl

whole = measure(text)
if whole is None:
    sys.exit(f"{doc}: nothing to measure")
grade, words, sentences, syl = whole

if report:
    worst = []
    for s in re.split(r'(?<=[.!?])\s+', text):
        s = ' '.join(s.split())
        m = measure(s)
        if m and len(re.findall(r"[A-Za-z][A-Za-z'-]*", s)) >= 6:
            worst.append((m[0], s))
    worst.sort(reverse=True)
    for g, s in worst[:25]:
        print(f"{g:5.1f}  {s[:150]}")
    print(f"\n{doc}: grade {grade:.2f} over {sentences} sentences")
    sys.exit(0)

print(f"{os.path.relpath(doc)}: Flesch-Kincaid grade {grade:.2f} "
      f"({words} words, {sentences} sentences, {syl/words:.2f} syllables/word)")
if grade > ceiling:
    print(f"::error file={os.path.relpath(doc)}::grade {grade:.2f} is above the "
          f"ceiling of {ceiling:.1f}; run "
          f"./tools/check-readability.sh --report to see the worst sentences")
    sys.exit(1)
PY
