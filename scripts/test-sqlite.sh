#!/bin/bash
# Builds SQLite 3.46.1 (the amalgamation and its shell) with keel's C
# compiler, and checks that it answers scripts/sqlite-queries.sql exactly
# as SQLite built by clang does.
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
defines=(-DSQLITE_THREADSAFE=0)

mkdir -p "$work"
cd "$work"
[ -f $name.zip ] || curl -sSfLO https://www.sqlite.org/2024/$name.zip
echo "$sha  $name.zip" | shasum -a 256 -c --quiet
[ -d $name ] || unzip -q $name.zip

cargo build --quiet --manifest-path "$root/Cargo.toml" -p keel
keel=$root/target/debug/keel

mkdir -p build
for src in sqlite3 shell; do
  clang -E -std=c11 "${defines[@]}" $name/$src.c -o build/$src.i
  "$keel" cc ${opt[@]+"${opt[@]}"} build/$src.i -o build/$src.s
done
clang build/sqlite3.s build/shell.s -o build/sqlite3-keel
clang -O2 -w -std=c11 "${defines[@]}" $name/sqlite3.c $name/shell.c -o build/sqlite3-clang
echo "built $work/build/sqlite3-keel"

queries=$root/scripts/sqlite-queries.sql
build/sqlite3-clang :memory: < "$queries" > build/clang.out 2>&1
build/sqlite3-keel :memory: < "$queries" > build/keel.out 2>&1
diff build/clang.out build/keel.out
echo "same results as clang ($(wc -l < build/keel.out | tr -d ' ') lines)"
