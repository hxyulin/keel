# Linux binary128/TC declaration checkpoint provenance

Implementation source: `eb43eb9513d77780796b8909365c7735fc81aa09`, based on
`d0aec154fe5e4fd0c77b1afc1d8431e4c735be0f`. Isolated branch:
`feat/linux-binary128-tc-layout`. Main and sibling worktrees were not changed.
The [maintained boundary](../c-system-header-compat.md#implemented-declarationlayout-checkpoint)
separates declarations/layout from unsupported values and ABI.

## Evidence identity

Evidence is outside Git at `/tmp/keel-tc-d0aec154/` on LinuxHost, with scripts,
JSON and logs copied to the same local directory. The final archive
`final-source.tar` has SHA-256
`9045de620bebfef0f637400d823dda9d26287fd885fe4efba8ce328e031c55ee`.
`audit.py` hash-compared all **1,994** archive files with the native build
checkout without mismatch. The final debug-profile compiler is
`src/target/debug/keel`, SHA-256
`f49197cccdde886440231f516252cd2e0354b69bdf4e75eb624c937a4f4298fe`.
The archive records the source checkpoint; this provenance is a later docs commit.

The original inputs remain in `/tmp/keel-header-boundary-8bb21e9a/` and the
original application source directories listed in its three `*-provenance.json`
files. No input was rewritten. Those provenance files retain working directories
and original preprocessing flags. Frozen GCC preprocessing used `/usr/bin/gcc`
16.2.1 20260810; Clang used `/usr/bin/clang` 23.1.1. The final native controls use
those explicit compiler paths. Keel raw inputs retain its own C11 identity.

`probe.py`, `probe.json`, `metadata.json`, `audit.json`, `final-probe.log` and
`final-probe-lock.jsonl` retain commands, source/input/object hashes, complete
stdout/stderr, disassembly, DWARF output and lock metadata. `apps.py`,
`apps.json`, `final-apps.log` and `final-apps-lock.jsonl` separately retain
application commands and correctness output. `initial-*` files retain the
intermediate checkpoint and are not the final-source verdict. No elapsed-time
fields from lock records or upstream test output are performance conclusions.

## Exact twelve unchanged object attempts

Every attempt uses the final compiler, `cc -O --opt-tier none --target
x86_64-unknown-linux-gnu --cpu-level v1`, followed by `INPUT --emit-object -o
OUTPUT`. Raw inputs additionally use the original definitions/includes and
original source working directory:

```sh
# QuickJS 2024-01-13
-D_GNU_SOURCE '-DCONFIG_VERSION="2024-01-13"' -DCONFIG_BIGNUM -I.
# Lua 5.4.7
-DLUA_USE_LINUX
# SQLite 3.46.1 (the LONGDOUBLE_TYPE flag was already in the frozen config)
-DSQLITE_THREADSAFE=1 -D_GNU_SOURCE -DLONGDOUBLE_TYPE=double
```

Nine attempts emit full ELF objects; three diagnose before object emission.
Each row is a full object attempt, not preprocessing or a frontend-only result.

| TU | Input | Final result / first diagnostic |
| --- | --- | --- |
| QuickJS `libregexp.c` | raw | Full object success |
| QuickJS `libregexp.i` | frozen GCC | `stdlib.h:145:8`: unsupported extended floating type `_Float32` |
| QuickJS `libregexp.i` | frozen Clang | Full object success |
| QuickJS `libregexp.i` | frozen Keel | Full object success |
| Lua `lvm.c` | raw | Full object success |
| Lua `lvm.i` | frozen GCC | `mathcalls-helper-functions.h:20:40`: expected `)`, found `__value` (the `_Float128` declaration) |
| Lua `lvm.i` | frozen Clang | Full object success |
| Lua `lvm.i` | frozen Keel | Full object success |
| SQLite `sqlite3.c` | raw | Full object success |
| SQLite `sqlite3.i` | frozen GCC | `stdlib.h:145:8`: unsupported extended floating type `_Float32` |
| SQLite `sqlite3.i` | frozen Clang | Full object success |
| SQLite `sqlite3.i` | frozen Keel | Full object success |

These are the same twelve inputs from the earlier investigation, including the
frozen Keel `.i` files, not new preprocessing that might hide a difference.
The raw source hashes are checked against the original provenance; frozen GCC
hashes are also checked against the original record. All input hashes follow.

| TU | Input | SHA-256 |
| --- | --- | --- |
| libregexp | raw | `076be6e24046d5561b7009b7777e959033811b15270ad9ad3c962f89db56c464` |
| libregexp | GCC | `c47f60de637b7785053ac2f582c66163b78f35fe6d454673a5daa790a00912e6` |
| libregexp | Clang | `629a619529f16d533df7e599c71321e80953c78402d9e03926e8e96b556d89f0` |
| libregexp | Keel | `9db82dd0b32aca25b8050094d35c0fc6c60ee15fcd9864e74832af338fa5b114` |
| lvm | raw | `e72a783157291555290d2de9b4e8855c9bd00a4ea02dc367caf498daa25928b6` |
| lvm | GCC | `542ebc728cf60e8464e7732d5b399148d3d6899519dc2486c82c659fa1815c27` |
| lvm | Clang | `d926ee9af9bfba8a571f9f2dd34ad79ab6124dd46de6f7ecf8aabb4835b33fed` |
| lvm | Keel | `7ef2675b3c1181a55bdf816e516cda6c23595e20d7f1b410274384d9b5c6baa3` |
| sqlite3 | raw | `6c35bc5f7f85eac9c49928bacbb02bb694b547aabf69197e058cca245ad80e83` |
| sqlite3 | GCC | `0c4d913e97a762ab0b3e67e55303169e767b7c7a7b5e7a5844c9da68abe3fb85` |
| sqlite3 | Clang | `7ec26f3eaa63d5c2e0c86ef7886809e9bf5c25623ad4368c04105a75b743611f` |
| sqlite3 | Keel | `46710170200dfefeebf3bd56c84857f9a1151d4ab2b1840f961131743111aea4` |

Object hashes are retained per invocation in `probe.json` and in `audit.json`.
Raw/Keel outputs match for all three TUs; Lua's Clang output also matches.
QuickJS/SQLite Clang outputs differ, so no general foreign-preprocessing
identity claim is made. No new independent frontend blocker appeared in the
raw/Clang/Keel attempts. The known GCC family blockers remain separate work;
passing Clang's header typedefs does not implement GCC `_FloatN` identities.

## Native application follow-through

All inputs are original configured sources, using the definitions above.
Compilation retains strict v1 and `-O --opt-tier none`. Links use `/usr/bin/gcc`
and `/usr/bin/clang` with the Keel objects and ordinary system `-lm`, `-ldl`
(and `-lpthread` for QuickJS/SQLite). No app source/header or acceptance flag was
changed. Objects, linked executables and reference outputs are under the
`lua/`, `quickjs/` and `sqlite/` subdirectories of the evidence directory.

- Lua: **33** objects, excluding the separate `luac.c` executable. Both links
  pass `lua -e '_U=true' all.lua` against a private `/tmp` copy of the original
  portable upstream test tree, reaching `final OK !!!`.
- QuickJS: **9** objects: `qjs`, `quickjs`, `libregexp`, `libunicode`, `cutils`,
  `quickjs-libc`, `libbf`, plus the already prepared original `repl.c` and
  `qjscalc.c`. Both links pass `test_closure`, `test_language`, `test_builtin`,
  `test_loop` and `--bignum test_bignum`.
- SQLite: **2** objects, `shell.c` and `sqlite3.c`. Both links run the maintained
  `suite/sqlite/queries.sql` in-memory workload. Output matches each system
  compiler's own native implementation built with the same original flags.

This is native execution of complete configured applications for these finite
correctness tests. It is not full upstream conformance, GCC-preprocessed input
support, all optimization profiles/ISA levels, or a benchmark campaign.

## Validation boundaries and reproduction

Rust commands use the existing pins and shared default cargo cache, with separate
managed targets, disabled native-link caching and the shared suite lock:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-tc-d0aec154/test.jsonl \
  cargo test --offline --locked -j2 -p keel-c
```

Final-source local full `keel-c`: **278 passed**. Final LinuxHost focused debug
batch: **40 passed** across `binary128_layout`, `unsupported_values`,
`system_header_compat`, `gnu_types`, `gnu_vector_abi`, `complex` and
`x86_intrinsics`, skipping the already documented `raw_platform_complex_header`
oracle failure. Release `binary128_layout`, `unsupported_values` and
`system_header_compat`: **17 passed**. Existing native backend `f80`,
`x86_vector_wrappers`, `x86_vector_varargs`: **8 passed**; release
`x86_64::feature_guard_tests`: **1 passed**. The latter unchanged backend checks
ran before the final parser query/test refinements; backend sources are
identical. Local final `keel-c --tests` clippy completes with existing warnings;
none refers to new production/test lines. No full native suite pass is claimed:
the earlier documented atomic/complex host-oracle incompatibilities are outside
this checkpoint.

The new maintained tests run native GCC/Clang layout oracles and link the
pointer-only fixture on Linux x86-64. Separate CLI probes emit strict v1/v3 ELF
objects, with and without `-g`, link each through both native compilers, and
execute each harness successfully. `readelf --debug-dump=info` succeeds for all
four debug-object inspections without diagnostics. Debug TC is a pair of
16-byte real components. Native storage belongs to the harness: Keel only
computes addresses/layout and writes a supported char member. Canary comparison
checks every byte of three wrappers; pointer stride/alignment and nested union
array offsets are checked independently. The existing aggregate ABI reproducer
is rejected in both plain and `-O --opt-tier none` strict-v1 compilation, and
`must-not-exist.o` is absent.

`objdump` listings of the four distinct pointer objects show ordinary baseline
integer operations and NOPs, with 78 instructions without debug and 105 with
debug. `cs` in the retained mnemonic inventory is a NOP prefix. This is finite
encoding inspection, not an exhaustive ISA decoder proof. Release feature
legality is enforced; execution on LinuxHost's newer CPU is not v1-only CPU proof.
Other-target tests are frontend/lowering/encoding evidence, not execution or
binary128 ABI evidence on AArch64/Darwin. There are no Keel binary128 value,
arithmetic, FP-rounding, helper or ABI claims, and no timing conclusion.
