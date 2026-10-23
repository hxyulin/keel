# Native SQLite reciprocal audit at d7d89d7d

Research base: `d7d89d7daf6dfd8778869f4aa5487707deaa229e` (2026-10-23).
This is a documentation-only follow-up to priority 2 in
[the application-gap investigation](application-gaps-dbe0b1f6.md#2-deduplicate-reciprocal-arithmetic-created-by-late-cleanup).
No production pass, schedule, inlining, feature, FP or default policy changed.
Main and sibling worktrees were not modified; no subagents were spawned.

## Finding

Fresh **native raw-source** SQLite Full compilation contains two high
multiplies in `sqlite3FpDecode` at both explicit v3 and v1. However, they operate
on **different SSA loads**, separated by the byte digit store. This is not the
identical-operand arithmetic duplicate demonstrated by the earlier Darwin IR
replay. An extra diagnostic `gvn,dce`, including a fresh raw-source compile with
those passes appended to Full, retains both native high multiplies.

The first instcombine, pipeline index 3 (zero-based), expands the separate
unsigned remainder and division into two reciprocal sequences. Each sequence
already reads a different loaded dividend. All three GVNs, including the last
at index 38, retain them. The late instcombine at index 55 does not recreate
these expressions. Although private aggregate cleanup activates elsewhere in
this module at index 62, **this function's textual IR is identical immediately
before and after it**. Internal fast calls at index 63 changes its private
calling convention, not this loop's arithmetic. There is no identified late
pass introducing a same-SSA duplicate in the qualified native pipeline.

The native loop, with names shortened but types and order retained, is:

```text
v = load.i64 align 8 slot
h = umulhi v, 0xcccccccccccccccd
q = ushr h, 3
byte = trunc.i8 ((v - q*10) + 48)
store byte, output
v_again = load.i64 align 8 slot
h_again = umulhi v_again, 0xcccccccccccccccd
q_again = ushr h_again, 3
store align 8 q_again, slot
```

The actual native source first calls `memcpy(&v,&r,8)` (SQLite line 35958).
This remains an ordinary IR `call @memcpy`, not a compiler-owned memory
intrinsic. Both `stack_forward.rs` and `stack_promote.rs` conservatively mark
an address passed to an ordinary call as escaping. The later assignment of
`v` does not undo that whole-function escape classification. The intervening
output store therefore prevents the ordinary memory analyses from identifying
the two loads. Simply caching by stack address or source variable would bypass
that safety boundary. Do not infer a nocapture/libc contract from the linker
name: `small_memory.rs` explicitly requires intrinsic semantic provenance.

The original report's Darwin replay remains useful as a scalar diagnostic,
but its same-operand cleanup activation is not established for this native
configuration. The historical 5–6% SQLite profile is not a current profile;
there is no measured removable cost or current hotness claim.

## Native evidence and pass trace

All final pipeline-prefix runs invoke `cc` directly on the unchanged raw
`sqlite3.c`; they do not replay Darwin IR or substitute preprocessed input.
The complete 64-pass prefix reproduces direct Full `sqlite3FpDecode` IR
exactly. Report-enabled and ordinary Full results also match exactly, and v1
and v3 final function IR match exactly. An initial exploration through serialized
unoptimized IR did not match raw Full; it was rejected as pipeline authority.
The final receipts are `native-trace.log` and `trace-commands.json`.

- After the first instcombine, each GVN, late instcombine, and private aggregate
  cleanup: **two** `umulhi`, with distinct dividend loads.
- Final native function: **35 loads, 32 stores**; extra diagnostic `gvn,dce`
  leaves those counts and the two high multiplies unchanged at v3 and v1.
- GNU objdump decodes two scalar baseline `mul` instructions in each freshly
  emitted ELF function. V3 object offsets are `0x10c13c` and `0x10c160`;
  v1 offsets are `0x10c11c` and `0x10c140`. The byte output store and second
  eight-byte slot load occur between them. There is no BMI2 dependency in
  this loop, including at v3.
- Full raw `shell.c` plus Full raw `sqlite3.c`, linked separately with GCC and
  Clang, runs the maintained `suite/sqlite/queries.sql` successfully at both
  levels. All four outputs match their respective retained native references
  from [the TC review](provenance/linux-binary128-tc-layout.md#independent-native-provenance-and-application-results).
  Those reference binaries are reused, not freshly rebuilt in this round.

Since no native transformation was applied, application memory accesses, fault
order, scalar FP scaling/Dekker operations and ABI are unchanged. The exact
before/after private-cleanup function comparison also includes every load,
store, FP instruction and terminator. These observations do not prove that
removing the second native load would preserve its semantics.

## Reductions and bounded recommendation

The external `digits.c` has two header-free functions compiled through native
Full at v3 and v1:

- `digits_unsigned(out, unsigned long v)`: one high multiply.
- `digits_bits(out, double bits)`: copies eight bytes into a local unsigned
  value using an ordinary `memcpy` declaration, then executes the same unsigned
  digit loop; two high multiplies. This reproduces the address-escape obstacle
  without SQLite's surrounding FP arithmetic.

The external `late-digit.keel` instead uses the **same i64 SSA input** for
`urem 10`, one byte store, then `udiv 10`. Existing `instcombine` produces two
high multiplies; diagnostic `instcombine,gvn,dce` produces one. Both emit one
byte store and no FP operations. Thus reciprocal expansion exists and pure
same-SSA reuse is valid in that reduction; it is insufficient for the native
SQLite loads above.

The only arithmetic follow-up recommended here, **if a native same-SSA
activation is subsequently demonstrated**, is reuse inside the existing
instcombine reciprocal expansion in `instcombine/constant_division.rs` and
its worklist caller. Do not add a whole-module GVN or change the schedule.
One concrete finite pilot could admit only functions with at most 8,192 laid-out
instructions and 16,384 values at entry, at most 256 expansions per invocation,
and 32 cached quotient recipes per block. On any cap, stop caching and retain
ordinary lowering. Key by resolved dividend SSA value, exact i32/i64 width and
unsigned divisor bits; reuse only an earlier complete quotient in the same
block with an identical deterministic reciprocal recipe. Never equate two
loads by address. A pure quotient may remain reusable across a store only
because its SSA inputs, rather than mutable memory, are identical.

This proposed scope admits unsigned i32/i64 only; signed admission is explicitly
excluded. It must not move divisions, speculate memory, change zero-divisor or
signed MIN/-1 behavior, widen stores, change fault order, or touch FP operations,
rounding/contraction/FMA, ABI or ISA policy. Focused optimizer/native tests would
still be required before implementation is accepted. **Do not implement this
pilot on a claim of improving the current qualified native SQLite loop:** it
has no same-SSA match. Removing its escape/alias obstacle requires separately
justified semantic provenance and is outside this arithmetic cleanup proposal.

Ownership for a possible future arithmetic pilot is
`crates/keel-opt/src/instcombine.rs`, `instcombine/constant_division.rs` and
focused optimizer regressions. `private_aggregate.rs` is an inspected caller,
not an identified source of this native issue. This commit owns only this audit.

## Validation limits

GCC 16.2.1 and Clang 23.1.1 independently compile a native unsigned reference
harness with `-O2 -ffp-contract=off` and matching `-march=x86-64-v3` / `-march=x86-64`.
It checks both Keel digit functions and each same-SSA one-digit variant over
100,011 unsigned inputs per executable: 0, 1, 9, 10, additional decimal/width
boundaries, `UINT64_MAX`, and 100,000 deterministic randomized values. All
**eight** executables pass digit-string, quotient/remainder and surrounding
byte-sentinel comparisons. The bit-copy reduction transports arbitrary double
bit patterns without FP arithmetic; it is not an FP environment/NaN semantics
oracle. Sentinels are finite footprint checks, not protected-page fault tests.
Signed division/remainder admission was not tested or proposed.

The unchanged release `x86_64::feature_guard_tests` passes (one test). Actual
release CLI emissions retain legality enforcement. Decoded objects establish
encoding and both linked applications and reducers establish native LinuxHost
execution for these finite cases. LinuxHost is not a v1-only CPU; this is not
exhaustive ISA certification, ABI conformance or upstream SQLite conformance.
The [ISA contract](userspace-isa-status.md), [C intrinsics](c-x86-intrinsics.md),
[GNU vectors](gnu-vector-mode.md), and unsupported-value boundaries remain in
force. No broad timing campaign, timing comparison or speedup is claimed;
lock waits/load and static counts are not performance evidence.

## Provenance and reproduction

Primary artifacts on LinuxHost, with scripts/JSON/logs/selected IR copied locally:
`/tmp/keel-native-reciprocal-d7d89d7d/`. Generated sources, objects, executables,
IR, assembly and reports stay outside Git. Fleet access used the installed
fleet skill. `git-archive-audit.json` matches all **1,995** archived files to
base Git blobs; `audit.json` matches all 1,995 files to the native build snapshot.
It also rechecks all four SQLite distribution files against the retained ZIP
(SHA-256 `77823cb110929c2bcb0f5d48e4833b5c59a8a6e40cdea3936b99e199dbbe5784`).
No raw source/header byte was changed.

- Source archive: `214595dbce65da95ab31f334a0105204bdf85b61af11a6a90420cfae3fc63cb2`.
- Fresh native release compiler: `72b0c01e1bf06d2c2e57233c8f2bc213f24f64b75cc8b757aaa640b158ea7681`.
- Raw `sqlite3.c`: `6c35bc5f7f85eac9c49928bacbb02bb694b547aabf69197e058cca245ad80e83`.
- Raw `shell.c`: `bed3cd2ea73b5afa4d43d7db400c66fe2b8127a21e1b6e4b7c96a48e6862165d`.

The raw-source working directory is
`/home/hxyulin/dev/projects/keel/target/suite/sqlite/sqlite-amalgamation-3460100`.
Every app compile uses the existing definitions
`-DSQLITE_THREADSAFE=1 -D_GNU_SOURCE -DLONGDOUBLE_TYPE=double`,
`-O --opt-tier full --target x86_64-unknown-linux-gnu --cpu-level v3`
(or explicit v1 control), plus `--emit-object -o OUTPUT`, `--emit-ir -o OUTPUT`,
or assembly `-o OUTPUT`. No feature override, sysroot, identity macro override
or source rewrite is added. Full is the deliberate optimization profile under
investigation; the earlier TC compatibility qualification used tier none.
Links use `/usr/bin/gcc` or `/usr/bin/clang`, both Keel objects, and
`-lm -lpthread -ldl`. Selection/allocation retain `-O` defaults.

All Rust invocations use the existing pins/features, default managed cargo cache,
disabled link caching and shared lock:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-native-reciprocal-d7d89d7d/build.jsonl \
  cargo build --offline --locked -j2 --release -p keel --bin keel
python3 suite/fast_opt_locked.py /tmp/keel-native-reciprocal-d7d89d7d/guard-lock.jsonl \
  cargo test --offline --locked -j2 --release -p keel-codegen --lib x86_64::feature_guard_tests
```

Native probes, traces, application checks, reducers and snapshot audit also use
`suite/fast_opt_locked.py`. No build output was deleted and no toolchain,
wrapper or remote-cache configuration changed. `probe-commands.json`,
`trace-commands.json`, `followup-commands.json`, `reducer-commands.json`,
`metadata.json`, `trace-summary.json`, `followup-summary.json`,
`reducer-summary.json`, `direct-gvn-commands.json`, `direct-gvn-summary.json`,
`audit.json` and the lock JSONL files retain exact argv,
source/object/reference/fixture hashes, stdout/stderr, counts and outcomes.
`v1/v3-sqlite3FpDecode.objdump` supplies independent instruction decoding.
