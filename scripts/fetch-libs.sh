#!/usr/bin/env bash
# Downloads the Scala Native library jars the VM loads from Maven Central into ref/libs and
# checks their sha256.
set -euo pipefail
dir=$(cd "$(dirname "$0")/.." && pwd)/ref/libs
mkdir -p "$dir"
repo=https://repo1.maven.org/maven2/org/scala-native
while read -r sum lib version; do
  file=${lib}_native0.5_3-$version.jar
  if [ ! -f "$dir/$file" ]; then
    curl --proto '=https' -sSfL "$repo/${lib}_native0.5_3/$version/$file" -o "$dir/$file.part"
    mv "$dir/$file.part" "$dir/$file"
  fi
  if command -v sha256sum >/dev/null; then
    echo "$sum  $dir/$file" | sha256sum -c --quiet -
  else
    echo "$sum  $dir/$file" | shasum -a 256 -c --quiet -
  fi
done <<'EOF'
f3bd2ead9788b9c90b2758e2e1f4988168b0b65674a7a1affc485ab4661a1c29 auxlib 0.5.12
c1d1ae542473bfdfacd1f27c41db2f6eae7ce665a0a61500e61d3b0e307fdad0 clib 0.5.12
1a643e587e7e4da38a6ce989a7541df2bf5a6d290372e50765e0c78d21695af4 javalib 0.5.12
b05fe9855ca50ac60854de6700f518e031df76c85021727505dab660d8757886 nativelib 0.5.12
25d292daace63da5b468d82bf0f830ca381800b69c364d18f714f18f95cd5dde posixlib 0.5.12
50fe972cf958bbb0f350a2aa0f16916f2ed8089232331e00a7004321576652cf scalalib 3.9.0%2B0.5.12
EOF
