#!/bin/bash
# Builds SQLite 3.46.1 (the amalgamation and its shell) with keel's C
# compiler, and checks that it answers scripts/sqlite-queries.sql exactly
# as SQLite built by clang does. It is thread-safe, with pthread mutexes
# and atomics, as by default.
#
#   scripts/test-sqlite.sh [-O] [work-dir]
set -euo pipefail

opt=()
if [ "${1:-}" = -O ]; then
  opt=(-O)
  shift
fi
work=${1:-target/sqlite}
root=$(cd "$(dirname "$0")/.." && pwd)
name=sqlite-amalgamation-3460100
sha=77823cb110929c2bcb0f5d48e4833b5c59a8a6e40cdea3936b99e199dbbe5784
defines=(-DSQLITE_THREADSAFE=1)
libs=(-lm -lpthread)
# glibc hides POSIX functions such as `fileno` under strict -std=c11.
[ "$(uname)" = Darwin ] || { libs+=(-ldl); defines+=(-D_GNU_SOURCE); }
# keel has no x87 `long double`. SQLite then does its high-precision
# arithmetic in pairs of doubles, as on targets where `long double` is
# `double` (Apple arm64). Both builds get the same code.
[ "$(uname -m)" = x86_64 ] && defines+=(-DLONGDOUBLE_TYPE=double)
# shasum on macOS, sha256sum on Linux.
check_sha256() {
  if [ "$(uname)" = Darwin ]; then shasum -a 256 -c --quiet; else sha256sum -c --quiet; fi
}

mkdir -p "$work"
cd "$work"
[ -f $name.zip ] || curl -sSfLO https://www.sqlite.org/2024/$name.zip
echo "$sha  $name.zip" | check_sha256
[ -d $name ] || unzip -q $name.zip

cargo build --quiet --manifest-path "$root/Cargo.toml" -p keel
keel=$root/target/debug/keel

mkdir -p build
for src in sqlite3 shell; do
  clang -E -std=c11 "${defines[@]}" $name/$src.c -o build/$src.i
  "$keel" cc ${opt[@]+"${opt[@]}"} build/$src.i -o build/$src.s
done
clang build/sqlite3.s build/shell.s -o build/sqlite3-keel "${libs[@]}"
clang -O2 -w -std=c11 "${defines[@]}" $name/sqlite3.c $name/shell.c -o build/sqlite3-clang "${libs[@]}"
echo "built $work/build/sqlite3-keel"

queries=$root/scripts/sqlite-queries.sql
build/sqlite3-clang :memory: < "$queries" > build/clang.out 2>&1
build/sqlite3-keel :memory: < "$queries" > build/keel.out 2>&1
diff build/clang.out build/keel.out
echo "same results as clang ($(wc -l < build/keel.out | tr -d ' ') lines)"
