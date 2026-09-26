#!/usr/bin/env sh
# Read installers/manifest.json without jq.
#
# An installer runs on a machine that has just been told it needs cargo and
# git; it is in no position to also need a JSON parser. So the manifest is kept
# flat — strings and arrays of strings, one per line — and this reads it with
# sed, which every POSIX host has.
#
# Both readers exit non-zero on a key that is not there. A missing key in an
# installer is a URL that silently becomes empty, and `git clone ""` fails
# somewhere much further along with a much worse message.
#
#   . installers/manifest.sh            # sets MANIFEST to the file
#   m_str repo_url                      # one string
#   m_list build_args                   # one per line

MANIFEST="${MANIFEST:-$(dirname "$0")/manifest.json}"
[ -f "$MANIFEST" ] || MANIFEST="$(dirname "$0")/installers/manifest.json"

m_str() {
  value=$(sed -n "s/^[[:space:]]*\"$1\"[[:space:]]*:[[:space:]]*\"\\(.*\\)\",\\{0,1\\}$/\\1/p" \
          "$MANIFEST" | head -1 | sed 's/",*$//')
  if [ -z "$value" ]; then
    echo "manifest: no key '$1' in $MANIFEST" >&2
    return 1
  fi
  printf '%s\n' "$value"
}

# Arrays are written one element per line in the manifest, which is what makes
# this possible at all. Keep it that way.
m_list() {
  awk -v key="\"$1\"" '
    index($0, key) && index($0, "[") { inside = 1; sub(/.*\[/, ""); }
    inside {
      line = $0
      if (index(line, "]")) { sub(/\].*/, "", line); inside = 0; done = 1 }
      while (match(line, /"([^"\\]|\\.)*"/)) {
        item = substr(line, RSTART + 1, RLENGTH - 2)
        gsub(/\\"/, "\"", item)
        print item
        line = substr(line, RSTART + RLENGTH)
      }
      if (done) exit
    }
  ' "$MANIFEST"
}
