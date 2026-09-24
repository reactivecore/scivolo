#!/usr/bin/env bash
# Reproduces the baseline measurements of spikes/00-baseline.
#
#   ./run.sh              # everything (≈ 15 min on an M4 Pro)
#   ./run.sh env          # environment only
#   ./run.sh bench        # timing only
#   ./run.sh artifacts    # TASTy / class / NIR / stdlib jar sizes only
#
# Env knobs: RUNS (default 10 for fast commands), SLOW_RUNS (default 5 for
# anything that compiles), SCALA (default 3.9.0), SN (Scala Native, default 0.5.12).
#
# Output: results/env.txt, results/bench.csv, results/bench.txt, results/artifacts.txt
# Scratch: work/ (gitignored; safe to delete).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SCALA="${SCALA:-3.9.0}"
SN="${SN:-0.5.12}"
RUNS="${RUNS:-10}"
SLOW_RUNS="${SLOW_RUNS:-5}"
W="$HERE/work"
R="$HERE/results"
BENCH="python3 $HERE/bench.py"
CSV="$R/bench.csv"
mkdir -p "$W/bin" "$R"

# Every script is copied into its own directory so each is a separate scala-cli project.
setup_work() {
  for s in hello moderate cpu; do
    mkdir -p "$W/$s"
    cp "$HERE/scripts/$s.scala" "$W/$s/$s.scala"
  done
}
# Appending a unique comment changes the source hash -> scala-cli/Bloop/zinc must recompile.
mutate() { echo "echo \"// \$(uuidgen)\" >> $W/$1/$1.scala"; }

b() { # b NAME RUNS WARMUP [--prepare CMD] -- CMD...
  local name="$1" runs="$2" warm="$3"; shift 3
  $BENCH --name "$name" --runs "$runs" --warmup "$warm" --csv "$CSV" "$@" | tee -a "$R/bench.txt"
}

env_info() {
  {
    echo "date: $(date -u +%FT%TZ)"
    echo "uname: $(uname -srm)"
    echo "cpu: $(sysctl -n machdep.cpu.brand_string) ($(sysctl -n hw.ncpu) cores, $(( $(sysctl -n hw.memsize) / 1073741824 )) GiB)"
    echo "macOS: $(sw_vers -productVersion)"
    echo "scala-cli: $(scala-cli version --offline | head -1)"
    echo "scala (runner): $(scala version --offline 2>/dev/null | head -1 || true)"
    echo "Scala used: $SCALA; Scala Native: $SN"
    echo "java: $(java -version 2>&1 | head -1)"
    echo "python3: $(python3 --version)"
    echo "rustc: $(rustc --version)"
    echo "clang: $(clang --version | head -1)"
  } | tee "$R/env.txt"
}

bench() {
  : > "$CSV"; : > "$R/bench.txt"
  echo "name,runs,mean_ms,sd_ms,min_ms,median_ms,max_ms" > "$CSV"
  setup_work

  # ---- prime caches (dependency download, Bloop start) -------------------------
  for s in hello moderate cpu; do
    scala-cli run "$W/$s/$s.scala" >/dev/null 2>&1
  done

  # ---- scala-cli launcher alone (GraalVM native image) ------------------------
  b "scala-cli version --offline (launcher only)" "$RUNS" 2 -- scala-cli version --offline

  for s in hello moderate cpu; do
    F="$W/$s/$s.scala"
    # (b) cached compilation, warm Bloop
    b "[$s] (b) scala-cli run, cached, warm Bloop" "$RUNS" 2 -- scala-cli run "$F"
    # (a) source changed -> incremental recompile on warm Bloop
    b "[$s] (a) scala-cli run, changed src, warm Bloop" "$SLOW_RUNS" 1 --prepare "$(mutate $s)" -- scala-cli run "$F"
    # compile only (to separate compile vs run)
    b "[$s]     scala-cli compile, cached, warm Bloop" "$RUNS" 2 -- scala-cli compile "$F"
    b "[$s]     scala-cli compile, changed src, warm Bloop" "$SLOW_RUNS" 1 --prepare "$(mutate $s)" -- scala-cli compile "$F"
    # (c) no build server: scala-cli forks a fresh JVM running dotty.tools.dotc.Main every time
    #     (it does NOT cache in this mode, so unchanged == changed)
    b "[$s] (c) scala-cli run --server=false" "$SLOW_RUNS" 1 -- scala-cli run --server=false "$F"
  done

  # (a0) truly cold: Bloop daemon stopped before each run (daemon JVM start + cold compiler)
  b "[hello] (a0) scala-cli run, Bloop stopped" 3 0 \
    --prepare "scala-cli --power bloop exit; $(mutate hello)" -- scala-cli run "$W/hello/hello.scala"
  scala-cli run "$W/hello/hello.scala" >/dev/null 2>&1   # restart Bloop

  # ---- (d) plain JVM on the compiled classes ----------------------------------
  for s in hello moderate cpu; do
    CP0="$(scala-cli --power compile --print-class-path "$W/$s/$s.scala" 2>/dev/null)"
    # CDS only accepts jars on the class path -> jar up the classes dir
    rm -f "$W/$s.jar"; jar cf "$W/$s.jar" -C "${CP0%%:*}" .
    CP="$W/$s.jar:${CP0#*:}"
    echo "$CP" > "$W/$s.classpath"
    b "[$s] (d) java -cp <app.jar+stdlib> $s" "$RUNS" 2 -- java -cp "$CP" "$s"
    # AppCDS: archive of loaded classes created on the first (warmup) run
    rm -f "$W/$s.jsa"
    b "[$s] (d') java + AppCDS (-XX:+AutoCreateSharedArchive)" "$RUNS" 2 -- \
      java -XX:+AutoCreateSharedArchive -XX:SharedArchiveFile="$W/$s.jsa" -cp "$CP" "$s"
  done

  # ---- (e) Scala Native --------------------------------------------------------
  for s in hello moderate cpu; do
    F="$W/$s/$s.scala"
    for mode in debug release-fast; do
      OUT="$W/bin/$s-native-$mode"
      b "[$s] (e) native package ($mode), changed src" "$SLOW_RUNS" 1 --prepare "$(mutate $s)" -- \
        scala-cli --power package --native --native-version "$SN" --native-mode "$mode" "$F" -o "$OUT" -f
      b "[$s] (e) native binary run ($mode)" "$RUNS" 2 -- "$OUT"
    done
    b "[$s] (e) scala-cli run --native (debug), cached" "$SLOW_RUNS" 1 -- \
      scala-cli --power run --native --native-version "$SN" "$F"
  done

  # ---- (f) reference points ----------------------------------------------------
  b "[hello] (f) python3 -c print" "$RUNS" 2 -- python3 -c "'print(\"Hello, world!\")'"
  b "[moderate] (f) python3 moderate.py" "$RUNS" 2 -- python3 "$HERE/scripts/moderate.py"
  b "[cpu] (f) python3 cpu.py" "$SLOW_RUNS" 1 -- python3 "$HERE/scripts/cpu.py"
  rustc -O "$HERE/scripts/hello.rs" -o "$W/bin/hello-rs"
  rustc -O "$HERE/scripts/cpu.rs" -o "$W/bin/cpu-rs"
  b "[hello] (f) rust binary (rustc -O)" "$RUNS" 2 -- "$W/bin/hello-rs"
  b "[cpu] (f) rust binary (rustc -O)" "$RUNS" 2 -- "$W/bin/cpu-rs"
  mkdir -p "$W/jh"
  printf 'public class H { public static void main(String[] a){ System.out.println("Hello, world!"); } }' > "$W/jh/H.java"
  javac -d "$W/jh" "$W/jh/H.java"
  b "[hello] (f) plain Java hello (java -cp)" "$RUNS" 2 -- java -cp "$W/jh" H
  b "[hello] (f) /usr/bin/true (process spawn floor)" "$RUNS" 2 -- /usr/bin/true
}

# ---- artifacts: sizes of TASTy / class / NIR and stdlib jars ------------------
sum_sizes() { # sum_sizes DIR EXT -> "count bytes"
  find "$1" -name "*.$2" -type f -print0 | xargs -0 stat -f %z 2>/dev/null |
    awk '{n++; s+=$1} END {printf "%d files, %d bytes", n, s}'
}
jar_stats() { # jar_stats JAR -> per-extension count / uncompressed bytes
  local jar="$1"
  echo "$(basename "$jar"): $(stat -f %z "$jar") bytes on disk"
  unzip -l "$jar" | awk 'NF==4 && $4 ~ /\./ {
      n=split($4, p, "."); ext=p[n]; c[ext]++; s[ext]+=$1 }
    END { for (e in c) if (e=="tasty"||e=="class"||e=="nir"||e=="sjsir") printf "    .%-6s %6d files %10d bytes uncompressed\n", e, c[e], s[e] }'
}

artifacts() {
  setup_work
  local out="$R/artifacts.txt"
  local A="$HERE/artifacts"
  mkdir -p "$A"
  CP="$(scala-cli --power compile --print-class-path "$W/moderate/moderate.scala" 2>/dev/null)"
  CLS="${CP%%:*}"
  NCP="$(scala-cli --power compile --native --native-version "$SN" --print-class-path "$W/moderate/moderate.scala" 2>/dev/null)"
  NCLS="${NCP%%:*}"
  {
    echo "== moderate.scala (Scala $SCALA, JVM) =="
    echo "source: $(stat -f %z "$HERE/scripts/moderate.scala") bytes"
    echo ".tasty: $(sum_sizes "$CLS" tasty)"
    echo ".class: $(sum_sizes "$CLS" class)"
    echo
    echo "== moderate.scala (Scala $SCALA, Scala Native $SN) =="
    echo ".tasty: $(sum_sizes "$NCLS" tasty)"
    echo ".class: $(sum_sizes "$NCLS" class)"
    echo ".nir:   $(sum_sizes "$NCLS" nir)"
    echo
    echo "== per-file (JVM classes dir) =="
    (cd "$CLS" && ls -l *.tasty | awk '{printf "  %7d  %s\n", $5, $9}')
    echo
    echo "== stdlib / runtime jars (JVM) =="
    echo "$CP" | tr ':' '\n' | grep '\.jar$' | while read -r j; do jar_stats "$j"; done
    SL="$(echo "$CP" | tr ':' '\n' | grep 'scala-library-' | head -1)"
    echo "  samples from $(basename "$SL"):"
    unzip -l "$SL" | awk '$4 ~ /^scala\/(Predef|Option|collection\/immutable\/List|collection\/immutable\/Vector|collection\/IterableOps)\.(tasty|class)$/ {printf "    %8d  %s\n", $1, $4}'
    echo
    echo "== Scala Native runtime jars (NIR) =="
    echo "$NCP" | tr ':' '\n' | grep '\.jar$' | while read -r j; do jar_stats "$j"; done
  } | tee "$out"

  # keep one small .tasty + its source (and the NIR of the same class) for later spikes
  cp "$HERE/scripts/moderate.scala" "$A/moderate.scala"
  cp "$CLS/Employee.tasty" "$A/Employee.tasty"
  cp "$CLS/moderate\$package.tasty" "$A/moderate\$package.tasty"
  cp "$NCLS/Employee.nir" "$A/Employee.nir"
  ls -l "$A"
}

case "${1:-all}" in
  env) env_info ;;
  bench) env_info; bench ;;
  artifacts) artifacts ;;
  all) env_info; bench; artifacts ;;
  *) echo "usage: $0 [all|env|bench|artifacts]"; exit 1 ;;
esac
