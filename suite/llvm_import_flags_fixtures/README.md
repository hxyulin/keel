# LLVM integer flag refinement milestone

Run from the repository root with a built Keel and a new output directory:

```sh
cargo build -p keel
python3 suite/llvm_import_flags_validate.py \
  --libllvm /path/to/LLVM23/libLLVM.dylib \
  --keel target/debug/keel --out target/llvm-import-flags
```

The existing Apple AArch64, LLVM 23, scalar ABI/type restrictions still apply.
Accepted flags are:

- `add`, `sub`, `mul`, `shl`: `nsw`, `nuw`, or both, at i8/i16/i32/i64.
- `or disjoint`: i1/i8/i16/i32/i64.
- `trunc nsw`, `trunc nuw`, or both: any supported narrowing integer pair,
  including an i1 destination.
- `zext nneg`: supported widening integer pairs, including an i1 source.
- `icmp samesign`: all ten predicates at i8/i16/i32/i64; i1 eq/ne only.
  Pointer samesign stays rejected.
- `udiv exact`, `sdiv exact`, `lshr exact`, `ashr exact`: i8/i16/i32/i64.

These lower to existing ordinary operations, with no corresponding optimizer
assumptions. The flagged instruction agrees with the ordinary operation wherever
LLVM returns a non-poison result. Elsewhere the ordinary result refines poison;
this is safe even when a select masks that result. It does not promise wrapping
LLVM semantics to callers on source-undefined executions.

The importer additionally guards **every** signed division against the
`INT_MIN / -1` pair by selecting divisor 1 for that pair. A poison dividend
produced upstream can become INT_MIN during refinement; the guard prevents a
new Keel trap on a defined LLVM execution whose select masks the final poison.
This also fixes the pre-existing unflagged path from an oversized shift through
signed division. Keel signed remainder already returns zero without trapping on
this pair and needs no guard. Divisor zero continues to trap.

[LLVM's language reference](https://llvm.org/docs/LangRef.html#poisonvalues)
makes a poison divisor immediate UB. Its
[sdiv](https://llvm.org/docs/LangRef.html#sdiv-instruction) and
[srem](https://llvm.org/docs/LangRef.html#srem-instruction) semantics also make
concrete INT_MIN/-1 immediate UB, even if the result is unused or select-masked.
The validator excludes those concrete operands from reference assertions; its
poison-dividend cases create poison upstream instead. `exact` inexact results,
oversized shifts, overflow flags, overlapping disjoint inputs, opposite-sign
samesign operands, and violated cast flags are exercised only behind an
unselected select arm. Non-poison cases include integer boundaries. Explicit
`poison`, `undef`, `freeze`, vectors, unsupported widths, i1 arithmetic/ordered
comparisons, and integer-to-FP `nneg` remain rejected.

The validator generates a scalar matrix and a C driver with Python big-integer
oracle results. It compiles each exact original LLVM module with Clang at O0,
then compares native Keel None/Fast/Full runs. Both reference and Keel binaries
must pass the independent C assertions. `signed_contract.cpp` is the real
Clang signed-contract fixture copied here so this test owns its inputs: the
unmodified O1 compiler output must actually contain signed-overflow flags.
`poison_dividend.ll` adds downstream division/remainder, unused-result, exact,
and zero-divisor checks. All definitions/declarations must survive import.

Source bytes, replacement on success, existing/absent destination preservation
on failure, stdin/stdout, late whole-module rejection, parser rejection, failed
verification, and missing verifier are checked. Commands and completed checks
are saved in the output directory.
