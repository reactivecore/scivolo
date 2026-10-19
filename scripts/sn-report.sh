#!/usr/bin/env bash
# Runs the JUnit tests built by `scripts/sn-tests.sh all` in groups and prints a Markdown table
# of passed, failed and skipped tests per group, also to tmp/sn-report/report.md; per-group
# output lands in tmp/sn-report/. SCIVOLO_TIMEOUT (default 60 s) bounds each group. Exits 1 if
# a group did not complete or passed no test.
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
out=$root/tmp/sn-report
mkdir -p "$out"
groups=(
  "language:scala.:-org.scalanative:-scala.scalanative"
  "scala.scalanative:scala.scalanative."
  "java.lang:testsuite.javalib.lang."
  "java.io:testsuite.javalib.io."
  "java.net:testsuite.javalib.net."
  "java.math:testsuite.javalib.math."
  "java.nio:testsuite.javalib.nio."
  "java.util:testsuite.javalib.util.:-concurrent"
  "java.util.concurrent:testsuite.javalib.util.concurrent."
  "niobuffer:testsuite.niobuffer."
  "posixlib:testsuite.posixlib."
  "runtime:testsuite.runtime."
)
report() {
  local incomplete=0 total_p=0 total_f=0 total_s=0
  echo "| Group | Passed | Failed | Skipped | Completed |"
  echo "|---|---:|---:|---:|---|"
  for g in "${groups[@]}"; do
    IFS=: read -r name filters <<<"$g"
    IFS=: read -ra args <<<"$filters"
    SCIVOLO_TIMEOUT=${SCIVOLO_TIMEOUT:-60} "$root/scripts/sn-run.sh" "${args[@]}" >"$out/$name.txt" 2>&1
    p=$(grep -c '^PASS' "$out/$name.txt"); f=$(grep -c '^FAIL' "$out/$name.txt"); s=$(grep -c '^SKIP' "$out/$name.txt")
    done=$(grep -q '^passed ' "$out/$name.txt" && echo yes || tail -1 "$out/$name.txt" | cut -c1-60)
    [ "$done" = yes ] && [ "$p" -gt 0 ] || incomplete=1
    total_p=$((total_p + p)); total_f=$((total_f + f)); total_s=$((total_s + s))
    echo "| $name | $p | $f | $s | $done |"
  done
  echo "| **total** | **$total_p** | **$total_f** | **$total_s** | |"
  return $incomplete
}
report | tee "$out/report.md"
