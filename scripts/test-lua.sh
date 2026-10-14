#!/bin/bash
# Builds Lua 5.4.7 with keel's C compiler and runs Lua's test suite.
#
#   scripts/test-lua.sh [-O] [work-dir]
#
# Each source file is preprocessed by `clang -E` and compiled by
# `keel cc`; clang only assembles and links. Lua is built as `make
# macosx` builds it: GNU C99 with its POSIX extras (`dlopen`, `mkstemp`,
# `popen`), and with computed gotos in the interpreter loop, which Lua
# uses whenever the compiler is GNU C. The tests run in their portable
# mode (`_U`), which skips the ones that need Lua's internal test
# library.
set -euo pipefail

opt=()
if [ "${1:-}" = -O ]; then
  opt=(-O)
  shift
fi
work=${1:-target/lua}
root=$(cd "$(dirname "$0")/.." && pwd)
version=5.4.7
lua_sha=9fbf5e28ef86c69858f6d3d34eccc32e911c1a28b4120ff3e84aaa70cfbf1e30
tests_sha=8a4898ffe4c7613c8009327a0722db7a41ef861d526c77c5b46114e59ebf811e

mkdir -p "$work"
cd "$work"
fetch() {
  local file=$1 url=$2 sha=$3
  [ -f "$file" ] || curl -sSfL -o "$file" "$url"
  echo "$sha  $file" | shasum -a 256 -c --quiet
}
fetch lua-$version.tar.gz https://www.lua.org/ftp/lua-$version.tar.gz $lua_sha
fetch lua-$version-tests.tar.gz https://www.lua.org/tests/lua-$version-tests.tar.gz $tests_sha
[ -d lua-$version ] || tar xzf lua-$version.tar.gz
[ -d lua-$version-tests ] || tar xzf lua-$version-tests.tar.gz

cargo build --quiet --manifest-path "$root/Cargo.toml" -p keel
keel=$root/target/debug/keel

mkdir -p build
objs=()
for src in lua-$version/src/*.c; do
  name=$(basename "$src" .c)
  [ "$name" = luac ] && continue
  clang -E -std=gnu99 -DLUA_USE_MACOSX "$src" -o "build/$name.i"
  "$keel" cc ${opt[@]+"${opt[@]}"} "build/$name.i" -o "build/$name.s"
  clang -c "build/$name.s" -o "build/$name.o"
  objs+=("build/$name.o")
done
clang -o build/lua "${objs[@]}" -lm
echo "built $work/build/lua"

cd lua-$version-tests
../build/lua -e "_U=true" all.lua
