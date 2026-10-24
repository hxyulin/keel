# Linux binary128/TC declaration checkpoint: results

This page records the object-compilation and native application results for
the [binary128/TC declaration and layout checkpoint](../c-system-header-compat.md#implemented-declarationlayout-checkpoint),
which separates declarations/layout from unsupported values and ABI. All runs
are on x86-64 Linux with glibc 2.44, GCC 16.2 and Clang 23.1.

## Twelve unchanged object attempts

Every attempt uses `keel cc -O --opt-tier none --target x86_64-unknown-linux-gnu
--cpu-level v1 INPUT --emit-object -o OUTPUT`. Raw inputs additionally use the
original definitions/includes and source working directory:

```sh
# QuickJS 2024-01-13
-D_GNU_SOURCE '-DCONFIG_VERSION="2024-01-13"' -DCONFIG_BIGNUM -I.
# Lua 5.4.7
-DLUA_USE_LINUX
# SQLite 3.46.1 (LONGDOUBLE_TYPE is part of the original configuration)
-DSQLITE_THREADSAFE=1 -D_GNU_SOURCE -DLONGDOUBLE_TYPE=double
```

The preprocessed inputs are the same GCC, Clang and Keel `.i` files used in
the earlier investigation, not fresh preprocessing. Nine attempts emit full
ELF objects; three diagnose before object emission:

| TU | Input | Result / first diagnostic |
| --- | --- | --- |
| QuickJS `libregexp.c` | raw | Full object |
| QuickJS `libregexp.i` | GCC | `stdlib.h:145:8`: unsupported extended floating type `_Float32` |
| QuickJS `libregexp.i` | Clang | Full object |
| QuickJS `libregexp.i` | Keel | Full object |
| Lua `lvm.c` | raw | Full object |
| Lua `lvm.i` | GCC | `mathcalls-helper-functions.h:20:40`: expected `)`, found `__value` (the `_Float128` declaration) |
| Lua `lvm.i` | Clang | Full object |
| Lua `lvm.i` | Keel | Full object |
| SQLite `sqlite3.c` | raw | Full object |
| SQLite `sqlite3.i` | GCC | `stdlib.h:145:8`: unsupported extended floating type `_Float32` |
| SQLite `sqlite3.i` | Clang | Full object |
| SQLite `sqlite3.i` | Keel | Full object |

Raw and Keel-preprocessed outputs are identical for all three TUs; Lua's Clang
output also matches, while QuickJS/SQLite Clang outputs differ, so no general
foreign-preprocessing identity claim is made. No new frontend blocker appeared
in the raw/Clang/Keel attempts. The GCC family blockers remain separate work;
accepting Clang's header typedefs does not implement GCC `_FloatN` identities.

## Independent native provenance and application results

All 44 raw C translation units of the configured applications were compiled
from the original upstream sources at strict v1 with `-O --opt-tier none`, and
linked with `/usr/bin/gcc` and `/usr/bin/clang` using ordinary system `-lm`,
`-ldl` (and `-lpthread` for QuickJS/SQLite). No application source, header,
include path, predefined macro identity or acceptance flag was changed.
QuickJS's generated `repl.c` and `qjscalc.c` were regenerated with the existing
`gen/qjsc` using the upstream Makefile commands and match the original inputs.

- **Lua:** 33 objects, excluding the separate `luac.c` executable. Both links
  pass `lua -e '_U=true' all.lua` against a copy of the portable upstream test
  tree, reaching `final OK !!!`.
- **QuickJS:** 9 objects (`qjs`, `quickjs`, `libregexp`, `libunicode`,
  `cutils`, `quickjs-libc`, `libbf`, `repl`, `qjscalc`). Both links pass
  `test_closure`, `test_language`, `test_builtin`, `test_loop` and
  `--bignum test_bignum`.
- **SQLite:** 2 objects (`shell.c`, `sqlite3.c`). Both links run the maintained
  `suite/sqlite/queries.sql` in-memory workload, with output matching each
  system compiler's own native build using the same flags.

This is native execution of complete configured applications for these finite
correctness tests. It is not full upstream conformance, GCC-preprocessed input
support, coverage of other optimization profiles or ISA levels, or a benchmark.

## Pointer-only fixture checks

The maintained tests run native GCC/Clang layout oracles and link Keel's
pointer-only fixture on Linux x86-64. CLI probes also emit strict v1/v3 ELF
objects, with and without `-g`, link each through both native compilers and
execute each harness. `readelf --debug-dump=info` succeeds on the debug
objects; debug TC is a pair of 16-byte real components. Native storage belongs
to the harness: Keel only computes addresses/layout and writes a supported
`char` member. Canary comparison checks every byte of three wrappers; pointer
stride/alignment and nested union/array offsets are checked independently.
The four pointer objects contain only baseline integer operations and NOPs
(78 instructions without debug info, 105 with).

The existing aggregate ABI reproducer is rejected in both plain and
`-O --opt-tier none` strict-v1 compilation, with no output object created.

`mode(TC)` is also accepted on `_Complex long double` and its aliases (which
Keel represents as `Ty::Opaque`), matching native GCC and Clang. Regressions
cover attributes before/among specifiers, after typedef/member declarators and
in type names; pointer composition; idempotent TC attributes; compatible
function/callback redeclarations; and wrapper layout, plus negatives for late
record completion, recursive union/array containment, static function
addresses, discarded definitions, evaluated compound literals and atomic
subjects/operations.

To run the maintained tests:

```sh
cargo test -p keel-c --test binary128_layout --test unsupported_values \
  --test system_header_compat
```

Execution on a newer CPU is not proof of v1-only CPU compatibility. Results on
other targets are frontend/lowering/encoding checks, not execution or binary128
ABI evidence on AArch64 or Darwin. There are no Keel binary128 value,
arithmetic, FP-rounding, helper or ABI claims.
