#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

out="${1:-debug/comments_audit_latest.txt}"
mkdir -p "$(dirname "$out")"

tmp_raw="$(mktemp)"
tmp_flagged="$(mktemp)"
trap 'rm -f "$tmp_raw" "$tmp_flagged"' EXIT

files=$( { git ls-files; git ls-files --others --exclude-standard; } | sort -u )

for f in $files; do
  case "$f" in
    *.rs|*.ts|*.tsx|*.js|*.jsx|*.svelte|*.c|*.cc|*.cpp|*.h|*.hpp|*.go|*.py|*.java|*.sh|*.toml|*.yml|*.yaml|*.conf|*.ini|Dockerfile|.dockerignore|.gitignore)
      [ -f "$f" ] || continue
      awk -v file="$f" '
        function emit(ln, txt) {
          gsub(/^[[:space:]]+|[[:space:]]+$/, "", txt)
          if (txt == "") return
          print file ":" ln ":" txt
        }
        BEGIN { in_block = 0 }
        {
          line = $0
          ln = FNR

          if (in_block) {
            emit(ln, line)
            if (line ~ /\*\//) in_block = 0
            next
          }

          # shell/config comments
          if (file ~ /(Dockerfile|\.dockerignore|\.gitignore|\.toml|\.ya?ml|\.conf|\.ini|\.sh)$/) {
            if (line ~ /^[[:space:]]*#/) emit(ln, line)
            next
          }

          # rust doc + line comments
          if (line ~ /^[[:space:]]*(\/\/\/|\/\/!|\/\/)/) emit(ln, line)

          # inline // (skip obvious URLs)
          i = index(line, "//")
          if (i > 0) {
            prefix = substr(line, 1, i - 1)
            next_ch = substr(line, i + 2, 1)
            if (!(prefix ~ /https?:$/) && next_ch != "/") emit(ln, line)
          }

          # block comments
          if (line ~ /\/\*/) {
            emit(ln, line)
            if (line !~ /\*\//) in_block = 1
          }
        }
      ' "$f" >> "$tmp_raw"
      ;;
  esac
done

sort -u "$tmp_raw" > "$out"

# heuristic "likely noisy" report
awk -F: '
  {
    file=$1; line=$2; txt=substr($0, index($0,$3))
    c=txt
    i=index(c, "//")
    if (i > 0) c=substr(c, i + 2)
    sub(/^[[:space:]]*\/{2,3}[! ]?/, "", c)
    sub(/^[[:space:]]*\/\*+[[:space:]]?/, "", c)
    sub(/[[:space:]]*\*\/[[:space:]]*$/, "", c)
    sub(/^[[:space:]]*#\s?/, "", c)
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", c)

    if (c == "") next
    if (c ~ /^#\[|^#!?\[/) next
    if (c ~ /^(derive|serde|cfg|test|allow)\b/) next
    if (c ~ /^(hero|scan|form|verify|connack|publish|sidebar|content|footer|modal|graph|toggle|section|thumbnail)\b$/) print $0
    n=split(c, w, /[^A-Za-z0-9_.`-]+/)
    if (n >= 12) print $0
  }
' "$out" | sort -u > "$tmp_flagged"

{
  echo "# comments audit"
  echo "# raw_count: $(wc -l < "$out")"
  echo "# noisy_candidates: $(wc -l < "$tmp_flagged")"
  echo
  echo "## noisy_candidates"
  cat "$tmp_flagged"
} >> "$out"

echo "wrote $out"
