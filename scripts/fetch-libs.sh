#!/usr/bin/env bash
# Downloads the library jars the VM loads (Scala Native's) and the frontend reads (the Scala
# standard library's TASTy) from Maven Central into ref/libs and checks their sha256.
set -euo pipefail
dir=$(cd "$(dirname "$0")/.." && pwd)/ref/libs
mkdir -p "$dir"
repo=https://repo1.maven.org/maven2
while read -r sum path; do
  file=${path##*/}
  if [ ! -f "$dir/$file" ]; then
    curl --proto '=https' -sSfL "$repo/$path" -o "$dir/$file.part"
    mv "$dir/$file.part" "$dir/$file"
  fi
  if command -v sha256sum >/dev/null; then
    echo "$sum  $dir/$file" | sha256sum -c --quiet -
  else
    echo "$sum  $dir/$file" | shasum -a 256 -c --quiet -
  fi
done <<'EOF'
f3bd2ead9788b9c90b2758e2e1f4988168b0b65674a7a1affc485ab4661a1c29 org/scala-native/auxlib_native0.5_3/0.5.12/auxlib_native0.5_3-0.5.12.jar
c1d1ae542473bfdfacd1f27c41db2f6eae7ce665a0a61500e61d3b0e307fdad0 org/scala-native/clib_native0.5_3/0.5.12/clib_native0.5_3-0.5.12.jar
1a643e587e7e4da38a6ce989a7541df2bf5a6d290372e50765e0c78d21695af4 org/scala-native/javalib_native0.5_3/0.5.12/javalib_native0.5_3-0.5.12.jar
b05fe9855ca50ac60854de6700f518e031df76c85021727505dab660d8757886 org/scala-native/nativelib_native0.5_3/0.5.12/nativelib_native0.5_3-0.5.12.jar
25d292daace63da5b468d82bf0f830ca381800b69c364d18f714f18f95cd5dde org/scala-native/posixlib_native0.5_3/0.5.12/posixlib_native0.5_3-0.5.12.jar
50fe972cf958bbb0f350a2aa0f16916f2ed8089232331e00a7004321576652cf org/scala-native/scalalib_native0.5_3/3.9.0%2B0.5.12/scalalib_native0.5_3-3.9.0%2B0.5.12.jar
b844bf6fb3080c73df6b9c523b7a69b9ef4a980bb15e5c37c35443d1d0b0e0cd org/scala-lang/scala-library/3.9.0/scala-library-3.9.0.jar
EOF
