#!/bin/bash
# Builds QuickJS 2024-01-13 (with BigNum) with keel's C compiler and runs
# its test suite, as its `make test` does.
#
#   scripts/test-quickjs.sh [-O] [work-dir]
#
# QuickJS is GNU C: computed gotos in its interpreter loop, a function of
# well over a megabyte, C11 atomics, `__int128` arithmetic in its bignum
# library. `qjsc`, built by clang, compiles the REPL's JavaScript to the C
# files `repl.c` and `qjscalc.c`, as the Makefile does.
set -euo pipefail

opt=()
if [ "${1:-}" = -O ]; then
  opt=(-O)
  shift
fi
work=${1:-target/quickjs}
root=$(cd "$(dirname "$0")/.." && pwd)
version=2024-01-13
name=quickjs-$version
sha=3c4bf8f895bfa54beb486c8d1218112771ecfc5ac3be1036851ef41568212e03
defines=(-D_GNU_SOURCE "-DCONFIG_VERSION=\"$version\"" -DCONFIG_BIGNUM)

mkdir -p "$work"
cd "$work"
[ -f $name.tar.xz ] || curl -sSfLO https://bellard.org/quickjs/$name.tar.xz
echo "$sha  $name.tar.xz" | shasum -a 256 -c --quiet
[ -d $name ] || tar xf $name.tar.xz

cargo build --quiet --manifest-path "$root/Cargo.toml" -p keel
keel=$root/target/debug/keel

mkdir -p build
cd $name
lib=(quickjs libregexp libunicode cutils quickjs-libc libbf)
clang -w -O2 "${defines[@]}" qjsc.c $(printf '%s.c ' "${lib[@]}") -o ../build/qjsc -lm
../build/qjsc -c -o ../build/repl.c -m repl.js
../build/qjsc -fbignum -c -o ../build/qjscalc.c qjscalc.js
objs=()
for src in qjs.c $(printf '%s.c ' "${lib[@]}") ../build/repl.c ../build/qjscalc.c; do
  base=$(basename "$src" .c)
  clang -E -std=gnu11 "${defines[@]}" -I. "$src" -o "../build/$base.i"
  "$keel" cc ${opt[@]+"${opt[@]}"} "../build/$base.i" -o "../build/$base.s"
  clang -c "../build/$base.s" -o "../build/$base.o"
  objs+=("../build/$base.o")
done
clang -o ../build/qjs "${objs[@]}" -lm
echo "built $work/build/qjs"

for test in tests/test_closure.js tests/test_language.js tests/test_builtin.js \
  tests/test_loop.js tests/test_std.js tests/test_worker.js \
  "--bignum tests/test_op_overloading.js" "--bignum tests/test_bignum.js" \
  "--qjscalc tests/test_qjscalc.js"; do
  echo "$test"
  # shellcheck disable=SC2086 # the flag and the file are two words
  ../build/qjs $test
done
echo "all QuickJS tests passed"
