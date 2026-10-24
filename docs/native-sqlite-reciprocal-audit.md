# Native SQLite reciprocal arithmetic audit

This note analyzes the duplicate reciprocal (division-by-constant) arithmetic in
SQLite's `sqlite3FpDecode` digit loop when compiled natively for x86-64 Linux
with Keel's Full tier. It is an analysis only: no pass, schedule, inlining,
feature, FP or default policy was changed.

## Finding

Native Full compilation of the raw SQLite source contains two high multiplies
in `sqlite3FpDecode` at both `--cpu-level v3` and `v1`. They operate on
**different SSA loads**, separated by the byte digit store, so this is not an
identical-operand arithmetic duplicate that GVN could remove. Appending an extra
`gvn,dce` to Full leaves both high multiplies in place.

The first `instcombine` (pipeline index 3, zero-based) expands the separate
unsigned remainder and division into two reciprocal sequences, each already
reading a different loaded dividend. All three GVN runs keep them, and the late
`instcombine` does not recreate them. `private-aggregate-cleanup` leaves this
function's IR byte-identical, and `internal-fast-calls` changes only its private
calling convention. No late pass introduces a same-SSA duplicate.

The loop, with names shortened but types and order retained:

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

## Why the loads are not merged

The source first calls `memcpy(&v,&r,8)`. This remains an ordinary IR
`call @memcpy`, not a compiler-owned memory intrinsic. Both `stack_forward.rs`
and `stack_promote.rs` conservatively treat an address passed to an ordinary
call as escaping for the whole function, and the later assignment of `v` does
not undo that. The intervening output store therefore prevents the memory
analyses from identifying the two loads. Caching by stack address or source
variable would bypass that safety boundary, and a nocapture/libc contract must
not be inferred from the linker name (`small_memory.rs` requires intrinsic
provenance for the same reason).

In the emitted ELF function, GNU objdump shows two scalar `mul` instructions
with the byte store and a second eight-byte slot load between them. The loop
has no BMI2 dependency, even at v3.

## Reductions

`digits.c` contains two header-free functions compiled through native Full at
v3 and v1:

- `digits_unsigned(out, unsigned long v)`: one high multiply.
- `digits_bits(out, double bits)`: copies eight bytes into a local unsigned
  value with an ordinary `memcpy` declaration, then runs the same digit loop;
  two high multiplies. This reproduces the address-escape obstacle without
  SQLite's surrounding FP arithmetic.

`late-digit.keel` uses the **same i64 SSA input** for `urem 10`, one byte
store, then `udiv 10`. `instcombine` alone produces two high multiplies;
`instcombine,gvn,dce` produces one. So reciprocal expansion does duplicate work
and same-SSA reuse is valid there, but that is not the SQLite situation.

A native reference harness built with GCC and Clang (`-O2 -ffp-contract=off`,
`-march=x86-64-v3` and `-march=x86-64`) checks both digit functions and each
same-SSA variant over 100,011 unsigned inputs (boundaries, `UINT64_MAX` and
100,000 deterministic random values), comparing digit strings,
quotient/remainder and surrounding byte sentinels. All variants pass.

## Possible follow-up

If a native same-SSA case is demonstrated later, the appropriate fix is reuse
inside the existing reciprocal expansion in
`crates/keel-opt/src/instcombine/constant_division.rs` and its worklist caller,
not an extra whole-module GVN or a schedule change. A bounded design:

- Admit functions with at most 8,192 laid-out instructions and 16,384 values,
  at most 256 expansions per invocation and 32 cached quotient recipes per
  block; on any cap, stop caching and use ordinary lowering.
- Key by resolved dividend SSA value, exact i32/i64 width and unsigned divisor
  bits; reuse only an earlier complete quotient in the same block with an
  identical recipe. Never equate two loads by address. A pure quotient may be
  reused across a store only because its SSA inputs are identical.
- Unsigned i32/i64 only. No moving divisions, speculating memory, changing
  zero-divisor or signed MIN/-1 behavior, widening stores, changing fault order,
  or touching FP, ABI or ISA policy.

This would not improve the current SQLite loop, which has no same-SSA match.
Removing its escape/alias obstacle needs separately justified semantic
provenance for `memcpy`.

## Validation and reproduction

Full `sqlite3.c` plus `shell.c`, compiled with Keel and linked with GCC and
Clang, runs `suite/sqlite/queries.sql` successfully at v3 and v1 with output
matching native references. The x86-64 ISA, [C intrinsics](c-x86-intrinsics.md),
[GNU vector](gnu-vector-mode.md) and [userspace ISA](userspace-isa-status.md)
contracts are unchanged. This is not a timing study and claims no speedup.

SQLite was compiled from the unmodified amalgamation with:

```sh
keel cc -DSQLITE_THREADSAFE=1 -D_GNU_SOURCE -DLONGDOUBLE_TYPE=double \
  -O --opt-tier full --target x86_64-unknown-linux-gnu --cpu-level v3 \
  --emit-object sqlite3.c -o sqlite3.o      # or --emit-ir, or assembly
gcc sqlite3.o shell.o -lm -lpthread -ldl -o sqlite3   # or clang
```

Replace `v3` with `v1` for the baseline control. The relevant guard test is:

```sh
cargo test --release -p keel-codegen --lib x86_64::feature_guard_tests
```
