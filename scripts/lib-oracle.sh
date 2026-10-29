#!/usr/bin/env bash
# Compiles Scala Native's library modules with sbt (JDK) and the Scala 3 version the frontend
# types (3.9.0, Scala Native's own build uses 3.8.3), zips their NIR and TASTy into tmp/lib/sn/jars
# beside copies of the other jars of ref/libs, and copies their NIR into tmp/lib/sn/<module>:
# the oracle of `just lib-compile` in place of the published jars (Scala 3.1.3). The build runs in
# a copy of ref/scala-native, tmp/lib/build, patched to use the published compiler plugin of that
# Scala version (the plugin's 0.5.12 sources do not compile against it) and without -Werror.
set -euo pipefail
scala=3.9.0
root=$(cd "$(dirname "$0")/.." && pwd)
native="$root/ref/scala-native"
build="$root/tmp/lib/build/scala-native"
[ -d "$native" ] || { echo "ref/scala-native missing: run \`just fetch-scala-native\`" >&2; exit 1; }
mkdir -p "$build"
rsync -a --delete --exclude target --exclude .bsp "$native/" "$build/"
perl -0pi -e 's/if \(isGeneratingForIDE\) project\s*else project\.dependsOn\(nscPlugin % "plugin"\)/project.settings(libraryDependencies += compilerPlugin("org.scala-native" % "nscplugin_'$scala'" % "0.5.12"))/' \
  "$build/project/Build.scala"
perl -pi -e 's/Seq\("-Werror", "-encoding:utf8"\)/Seq("-encoding:utf8")/' "$build/project/Settings.scala"
grep -q "nscplugin_$scala" "$build/project/Build.scala" && ! grep -q '"-Werror", "-encoding:utf8"' "$build/project/Settings.scala" ||
  { echo "patching the build in $build failed" >&2; exit 1; }
jars="$root/tmp/lib/sn/jars"
rm -rf "$jars"
mkdir -p "$jars"
modules=(nativelib clib posixlib windowslib javalib auxlib)
tasks=("++$scala!")
for m in "${modules[@]}"; do tasks+=("${m}3/compile"); done
(cd "$build" && sbt -batch "${tasks[@]}")
for m in "${modules[@]}"; do
  dirs=$(find "$build/$m/.3/target" -type d -name classes -path "*scala-$scala*")
  [ "$(echo "$dirs" | grep -c .)" -eq 1 ] || { echo "expected one $m classes directory, found: $dirs" >&2; exit 1; }
  out="$root/tmp/lib/sn/$m"
  rm -rf "$out"
  mkdir -p "$out"
  (cd "$dirs" && find . -name '*.nir' | while read -r f; do mkdir -p "$out/$(dirname "$f")"; cp "$f" "$out/$f"; done)
  echo "$m: $(find "$out" -name '*.nir' | wc -l | tr -d ' ') NIR files in $out"
  jar="$jars/${m}_native0.5_3-0.5.12.jar"
  (cd "$dirs" && find . -name '*.nir' -o -name '*.tasty' | sed 's|^\./||' | sort | zip -q -X "$jar" -@)
done
for j in "$root"/ref/libs/*.jar; do [ -e "$jars/$(basename "$j")" ] || cp -p "$j" "$jars/"; done
echo "the modules' NIR and TASTy, with the other library jars, in $jars"
