#!/usr/bin/env bash
# Measures scalino (github.com/lolgab/scalino) as a JVM-free Scala 3.9 frontend.
#
#   ./run.sh            # fetch + bench + rss + macros
#   ./run.sh fetch      # download and unpack the release into work/
#   ./run.sh bench      # timings (results/bench.txt, results/bench.csv)
#   ./run.sh rss        # peak RSS per path (results/rss.txt)
#   ./run.sh macros     # macro checks (results/macros.txt)
#
# Env knobs: VERSION (default v0.0.13), RUNS (default 10), SLOW_RUNS (default 5).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
BASE="$HERE/../00-baseline"
VERSION="${VERSION:-v0.0.13}"
RUNS="${RUNS:-10}"
SLOW_RUNS="${SLOW_RUNS:-5}"
W="$HERE/work"
R="$HERE/results"
D="$W/scalino-$VERSION-macos-arm64"
BENCH="python3 $BASE/bench.py"
CSV="$R/bench.csv"
M2="$HOME/Library/Caches/Coursier/v1/https/repo1.maven.org/maven2"
mkdir -p "$W" "$R"

fetch() {
  local tgz="$W/scalino-$VERSION-macos-arm64.tar.gz"
  local url="https://github.com/lolgab/scalino/releases/download/$VERSION/scalino-$VERSION-macos-arm64.tar.gz"
  [ -f "$tgz" ] || curl -fsSL -o "$tgz" "$url"
  curl -fsSL "$url.sha256" | awk '{print $1}' | grep -q "$(shasum -a 256 "$tgz" | awk '{print $1}')"
  [ -d "$D" ] || tar xzf "$tgz" -C "$W"
  "$D/scalino-dotc" -version
}

# Shared flag sets. The stdlib jar is the JVM one (it carries the TASTy); JDK types come
# from scalino's java.base.jar because a native binary has no jrt:/ filesystem.
NCP() { sed "s#lib/#$D/lib/#g" "$D/nativelibs.cp"; }
JCP() { sed "s#lib/#$D/lib/#g" "$D/compiler.cp"; }
nat() { echo "$D/scalino-dotc -javabootclasspath $D/java.base.jar -classpath $D/lib/scala-library-3.9.0.jar"; }
jvm() { echo "java -cp $(JCP) dotty.tools.dotc.Main -classpath $D/lib/scala-library-3.9.0.jar"; }
tasty() { echo "-Ystop-after:firstTransform -Xearly-tasty-output $1 -d $1"; }
nir() { # nir [EXTRA_CP]
  echo "$D/scalino-dotc -javabootclasspath $D/java.base.jar -classpath ${1:+$1:}$(NCP)" \
       "-Xplugin:$D/lib/nscplugin_3.9.0-0.5.12.jar -Xplugin-require:scalanative -Yretain-trees"
}

setup_work() {
  for s in hello moderate cpu; do
    mkdir -p "$W/$s/out"
    cp "$BASE/scripts/$s.scala" "$W/$s/$s.scala"
  done
  mkdir -p "$W/macros/out"
  cp "$HERE"/scripts/macros/*.scala "$W/macros/"
}
mutate() { echo "echo \"// \$(uuidgen)\" >> $W/$1/$1.scala"; }
clean_out() { echo "rm -rf $W/$1/out && mkdir -p $W/$1/out"; }

b() { # b NAME RUNS WARMUP [--prepare CMD] -- CMD...
  local name="$1" runs="$2" warm="$3"; shift 3
  $BENCH --name "$name" --runs "$runs" --warmup "$warm" --csv "$CSV" "$@" | tee -a "$R/bench.txt"
}

bench() {
  echo "name,runs,mean_ms,sd_ms,min_ms,median_ms,max_ms" > "$CSV"; : > "$R/bench.txt"
  setup_work
  b "scalino-dotc -version (process floor)" "$RUNS" 2 -- "$D/scalino-dotc" -version
  b "scalino version" "$RUNS" 2 -- "$D/scalino" version
  for s in hello moderate cpu; do
    local F="$W/$s/$s.scala" O="$W/$s/out"
    b "[$s] native dotc -Ystop-after:parser" "$RUNS" 1 -- "$(nat) -Ystop-after:parser -d $O $F"
    b "[$s] native dotc -Ystop-after:typer" "$RUNS" 1 -- "$(nat) -Ystop-after:typer -d $O $F"
    b "[$s] native dotc TASTy only" "$RUNS" 1 --prepare "$(clean_out $s)" -- "$(nat) $(tasty "$O") $F"
    b "[$s] native dotc full NIR" "$RUNS" 1 --prepare "$(clean_out $s)" -- "$(nir) -d $O $F"
    b "[$s] JVM dotc TASTy only (cold java)" "$SLOW_RUNS" 1 --prepare "$(clean_out $s)" -- "$(jvm) $(tasty "$O") $F"
    b "[$s] scalino run, changed src" "$SLOW_RUNS" 1 --prepare "$(mutate $s)" -- "cd $W/$s && $D/scalino run -q $s.scala"
    b "[$s] scalino run, cached" "$RUNS" 1 -- "cd $W/$s && $D/scalino run -q $s.scala"
  done
  local MF="$W/macros"
  b "[compiletime] native dotc TASTy only" "$RUNS" 1 --prepare "$(clean_out macros)" -- "$(nat) $(tasty "$MF/out") $MF/compiletime.scala"
  b "[compiletime] native dotc full NIR" "$RUNS" 1 --prepare "$(clean_out macros)" -- "$(nir) -d $MF/out $MF/compiletime.scala"
  b "[compiletime] JVM dotc TASTy only (cold java)" "$SLOW_RUNS" 1 --prepare "$(clean_out macros)" -- "$(jvm) $(tasty "$MF/out") $MF/compiletime.scala"
  b "[quoted] native dotc TASTy only, Macro+Use" "$RUNS" 1 --prepare "$(clean_out macros)" -- "$(nat) -Yretain-trees $(tasty "$MF/out") $MF/Macro.scala $MF/UseMacro.scala"
  b "[quoted] native dotc full NIR, Macro+Use" "$RUNS" 1 --prepare "$(clean_out macros)" -- "$(nir) -d $MF/out $MF/Macro.scala $MF/UseMacro.scala"
  b "[quoted] JVM dotc full .class, Macro+Use (cold java)" "$SLOW_RUNS" 1 --prepare "$(clean_out macros)" -- "$(jvm) -d $MF/out $MF/Macro.scala $MF/UseMacro.scala"
}

rss() {
  setup_work
  : > "$R/rss.txt"
  peak() { # peak NAME CMD...
    local name="$1"; shift
    local kb
    kb=$( { /usr/bin/time -l sh -c "$*" >/dev/null 2>/dev/null; } 2>&1 | awk '/maximum resident/ {print int($1/1048576)}')
    printf '%-48s %6s MiB\n' "$name" "$kb" | tee -a "$R/rss.txt"
  }
  for s in hello moderate; do
    local F="$W/$s/$s.scala" O="$W/$s/out"
    peak "[$s] native dotc -Ystop-after:typer" "$(nat) -Ystop-after:typer -d $O $F"
    peak "[$s] native dotc TASTy only" "$(nat) $(tasty "$O") $F"
    peak "[$s] native dotc full NIR" "$(nir) -d $O $F"
    peak "[$s] JVM dotc TASTy only" "$(jvm) $(tasty "$O") $F"
    peak "[$s] scalino run (compile+link+run)" "cd $W/$s && rm -rf .scalino-build && $D/scalino run -q $s.scala"
  done
}

macros() {
  setup_work
  local MF="$W/macros" CP
  CP="$(NCP)"
  {
    echo "== scala.compiletime + Mirror derivation (single file)"
    (cd "$MF" && rm -rf .scalino-build && "$D/scalino" run -q --color never compiletime.scala 2>&1 | tail -8)
    echo "== quoted macro, separate file, same compile run"
    (cd "$MF" && "$D/scalino" run -q --color never Macro.scala UseMacro.scala --main-class useMacro 2>&1 | tail -8)
    echo "== quoted macro, separately compiled (TASTy on the classpath)"
    rm -rf "$MF/lib" "$MF/app" "$MF/link" && mkdir -p "$MF/lib" "$MF/app"
    $(nir) -Xearly-tasty-output "$MF/lib" -d "$MF/lib" "$MF/Macro.scala"
    $(nir "$MF/lib") -d "$MF/app" "$MF/UseMacro.scala"
    "$D/scalino-linkdriver" "$MF/app:$MF/lib:$CP" "$MF/link" useMacro "$(command -v clang)" "$(command -v clang++)" >/dev/null 2>&1
    "$MF/link/useMacro"
    echo "== quote type patterns ('[Int], '[List[t]])"
    (cd "$MF" && "$D/scalino" run -q --color never Macro.scala UseTypePattern.scala --main-class useTypePattern 2>&1 \
      | perl -pe 's/\e\[[0-9;]*m//g' | grep -E "does not support|error found|^kind|^listElem" || true)
  } 2>&1 | grep -v llvm-config | tee "$R/macros.txt"
}

case "${1:-all}" in
  fetch) fetch ;;
  bench) bench ;;
  rss) rss ;;
  macros) macros ;;
  all) fetch; bench; rss; macros ;;
  *) echo "unknown target: $1" >&2; exit 2 ;;
esac
