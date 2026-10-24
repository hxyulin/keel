# AVX2 direct-destination packed integer adds

Generic 256-bit integer `viadd` emits

```text
vpaddb/w/d/q destination, left, right
```

directly into the allocated result register, instead of staging both operands
and publishing a scratch result. All four integer lane widths share this path.
The optimizer, optimization context, default vector width, address folding,
ABI and FP selection are unchanged. Subtraction and other wide operations keep
their existing recipes.

## Why this is safe

The rule relies on existing allocation and emission contracts:

- Wide functions exclude SIMD indices 12-15 from allocation, including their
  XMM views. `use_freg` resolves SSA aliases; an allocated operand is read in
  place, while a spill is reloaded only into its requested scratch register.
- Left and right operands use distinct reload scratches FS0 and FS1, in their
  existing order, before the result is written. A spilled result uses FS0 only
  after both operands have been captured, and `def_done` completes the
  exact-width spill store. No operand is folded into a memory operand and no
  application load/store is moved or widened.
- VEX integer instructions read both complete YMM sources before writing the
  destination, so either source may share the destination when its SSA lifetime
  permits. Live sources keep separate allocations. Register views remain
  256 bits, so no XMM write can discard an upper half.
- Lane arithmetic wraps; there is no FP operand-order or NaN question.
- The existing `keep` gate requires AVX2. Strict v1/v2 still reject wide SSA
  transport, AVX without AVX2 keeps scalar/narrow lowering, unsupported forms
  keep existing diagnostics, and the release emission guard still checks
  operand-dependent feature requirements.

## Tests

`x86_avx2_direct_add` checks text/typed/chunked object parity and per-instruction
feature legality on an unoptimized 16-function corpus covering:

- all four integer widths; distinct, same and resolved-alias operands;
- inputs live after the add, and both inputs live across an external call that
  executes `vzeroall`;
- all three selectors, both allocators, and normal, one-SIMD, empty-SIMD and
  completely empty register pools;
- seven controlled layouts per selector that force either source/result alias,
  independent left/right/result spills, both sources spilled with a register
  result, and all spills. The all-spilled case asserts the exact instruction
  window: two 256-bit reloads, the add, then the 256-bit result store;
- strict v1/v2 rejection on all three output paths, a register-ABI case that
  asserts neither reload scratch appears, and byte-identical AVX-only output.

The native C oracle uses unsigned scalar lane addition and `memcpy`, independent
of compiler vector arithmetic. It checks deterministic patterns and overflow
edges, all byte offsets 0-31, output aliases to either input, adjacent
canaries, both protected-page boundaries and unchanged MXCSR. Inputs are
exactly 32 bytes; the output holds the result and the original inputs in three
separate 32-byte stores.

```sh
cargo test -p keel-codegen --release --test x86_avx2_direct_add
```

Set `KEEL_AVX2_ADD_EVIDENCE=<dir>` to export objects, assembly and C harnesses,
then run `crates/keel-codegen/tests/x86_widen/run_native.py` on a Linux x86-64
host. All 47 dedicated native oracles pass on x86-64 Linux with Clang 23 and
GCC 16, alongside the existing generic, wide SysV ABI and conversion oracles.
These are finite native tests on AVX2 hardware, not exhaustive ISA
certification or execution on v1/v2-only CPUs.

## Measurements

The ignored `measure_signed_sum_direct_add` test compiles the
`signed_widen_sum.ir` fixture with wide optimization explicitly requested, so
base and candidate share identical optimized IR and only emission differs:

```sh
cargo test -p keel-codegen --release --test x86_avx2_direct_add \
  measure_signed_sum_direct_add -- --ignored --nocapture
```

Whole-function static results (Full selection/allocation, normal pools):

| Metric | Before | After |
| --- | ---: | ---: |
| Emitted instructions | 123 | 102 |
| `.text` bytes | 444 | 350 |
| `vmovups` | 26 | 5 |

The four-add loop loses all 12 staging copies and the three final accumulator
adds lose nine more. All nine `vpaddq` instructions and all ten memory-address
instructions are unchanged and in their original order.

A pinned kernel timing on x86-64 Linux (one C driver, both objects checked
against an unsigned wrapping oracle first, alternating arm order, 12 measured
rounds) gave these median nanoseconds per call:

| Length | Before | After |
| ---: | ---: | ---: |
| 16 | 4.046 | 3.483 |
| 64 | 6.225 | 4.613 |
| 256 | 13.997 | 9.408 |
| 4,096 | 203.105 | 117.093 |
| 262,144 | 12,713.045 | 7,687.707 |
| 1,048,576 | 50,808.527 | 31,337.527 |

These are bounded kernel timings including call overhead; they do not compare
optimizer widths or establish application speedups.

Codegen stage time on the same pre-optimized module also dropped (median
microseconds: text codegen 83.2 -> 55.9, typed codegen 80.8 -> 53.7, typed
object generation 180.7 -> 115.3). These are small single-module observations,
not a compiler throughput claim.
