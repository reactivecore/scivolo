#!/usr/bin/env bash
# The typer report's outcome counts and the files that changed outcome against the baseline.
set -euo pipefail
results=tmp/typer/3.9.0/results.tsv
baseline=tmp/typer/baseline.tsv
[ -f "$baseline" ] || { echo "no baseline: run just typer-baseline" >&2; exit 1; }
of() { awk -F'\t' -v o="$2" '$2==o{print $1}' "$1" | sort; }
row() { while read -r f; do grep -F "$f	" "$results" | cut -c1-300 || echo "$f	missing"; done; }
awk -F'\t' '{print $2}' "$results" | sort | uniq -c
echo "== lost passes"
comm -23 <(of "$baseline" passed) <(of "$results" passed) | row
for o in accepted differ "warnings differ" crashed; do
    echo "== new $o"
    comm -13 <(of "$baseline" "$o") <(of "$results" "$o") | row
done
echo "== new passes: $(comm -13 <(of "$baseline" passed) <(of "$results" passed) | wc -l)"
