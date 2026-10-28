# Experimental Keel IR → Cranelift bridge

`keel-cranelift` translates verified Keel IR into Cranelift IR (CLIF) and
emits an Apple arm64 Mach-O object with Cranelift. It is a diagnostic tool:
it compares Keel's native backend with Cranelift's on **the same Keel IR**,
for correctness and for compile/runtime cost. It is not a production
backend. It does not replace or change the native backend, the `keel`
driver, optimizer defaults or any production build.

It is a separate Cargo workspace with its own `Cargo.lock`. The root
`Cargo.toml` lists it in `exclude`. Main-workspace builds, including
`--offline --locked` builds, never fetch or compile Cranelift.

## Build, test, use

```sh
cd crates/keel-cranelift
cargo build --release --locked --target-dir ../../target/keel-cranelift
cargo test  --release --locked --target-dir ../../target/keel-cranelift
../../target/keel-cranelift/release/keel-cranelift --target arm64-apple-macosx11.0.0 \
    --opt speed input.keel -o output.o           # or --emit clif -o -
```

- `--target` is mandatory, as for `keel-llvm`. Only
  `arm64-apple-macosx11.0.0` is accepted, and the host is never inferred.
- `--opt none|speed` selects Cranelift's `opt_level`. `speed_and_size` is
  not exposed.
- The CLIF verifier runs unless `--no-verify` is given. Output is identical
  either way; a test checks this.
- Translation is buffered. Any unsupported feature fails with
  `function @f: blockN, instM (<instruction>): unsupported: ...`, and nothing
  is written. Output is published through a temporary file and a rename.

`keel-cranelift-bench` is the comparison harness:

- `objects` writes one object per configuration.
- `time` times all configurations in-process from one parsed module.
- `support` reports per-function coverage.
- `subset` keeps only supported functions, for compile-time inputs.

`tools/compare.py` drives the whole procedure and writes the evidence. Its
order is: correctness gate, then compile timing, then exploratory runtime.

## Pinned versions and settings

- `cranelift-codegen`, `-frontend`, `-module` and `-object` are pinned to
  `=0.136.3`, the newest stable release (2026-10-14). The lock file pins
  `regalloc2 0.15.2`, `object 0.40.0` and `target-lexicon 0.13.5`.
- Only the `arm64` backend feature is enabled: no host detection, JIT,
  disassembler or unwind support.
- API use was checked against the 0.136.3 sources: `MemFlagsData` interning,
  `FunctionBuilder::finalize(config)` and `Switch::emit`.
- The ISA is built from triple `aarch64-apple-macosx11.0.0`, which sets
  LC_BUILD_VERSION minos 11.0.
- Calling convention: `apple_aarch64`.
- Explicit settings: `opt_level`, `is_pic=true` (GOT access for
  non-colocated symbols), `unwind_info=false` and `enable_verifier`.
  - Native Keel objects can carry `__compact_unwind` and `__eh_frame`;
    this bridge emits neither. Compile time and object size therefore compare
    the configured pipelines with different unwind services. They are not
    equal-feature measurements.
  - All other shared and AArch64 flags are defaults. The defaults assume
    no LSE, PAuth or BTI, and keep backtracking register allocation.

## Semantic mapping (checked against Keel's spec and CLIF's definitions)

Sources: Keel's `docs/design.md`, `docs/simd-ir.md` and
`keel-ir/src/inst.rs`, and Cranelift's instruction definitions in
`cranelift-codegen-meta-0.136.3/src/shared/instructions.rs`.

**Values, control flow and constants**

- Keel block arguments map 1:1 to CLIF block parameters. Blocks are
  translated in reverse postorder, and unreachable blocks are dropped.
- Keel already forbids branches to the entry block.
- Comparisons produce `i8` 0/1 in both IRs. `select` and `brif` test for
  nonzero in both.
- `ptr` is `i64`. `ptradd` becomes `iadd`, and `ptrtoint`/`inttoptr` are
  identities.
- Narrow `iconst` immediates are masked to the type width, as CLIF requires.
  `i128` constants are built with `sextend`.

**Integer arithmetic**

- `udiv`, `sdiv`, `urem` and `srem` map directly. CLIF traps on zero
  divisors and on `sdiv` MIN/-1, exactly as Keel does. `srem` MIN/-1 is 0 in
  both.
  - `keel-llvm` rejects division, because LLVM's division is undefined
    behavior in exactly these cases.
  - `i128` division and `mulhi` are rejected.
- Shift and rotate amounts are masked modulo the width of the shifted
  operand in both IRs, whatever the amount's type. An `i128` amount is
  reduced to `i64` first.
- `clz` and `ctz` of 0 give the bit width in both.
- The overflow operations (`uadd_overflow` and so on) map to CLIF's
  two-result forms. `i128` `umul_overflow`/`smul_overflow` is rejected
  because CLIF has no AArch64 lowering for it.

**Floating point**

- `fmin` and `fmax` follow the Wasm rules in CLIF: NaN propagates and -0 <
  +0, matching Keel's IEEE 754-2019 `minimum`/`maximum`.
- `fnearest` is CLIF `nearest`, rounding ties to even.
- `fptosi`/`fptoui` become `fcvt_to_{s,u}int_sat`. Both saturate and turn
  NaN into 0.
- `sitofp`/`uitofp` from `i8`/`i16` extend to `i32` first.
- `fcmp`: Keel `ne` becomes CLIF `ne`, which is unordered-or-not-equal and
  matches C's `!=`. The other conditions are ordered; `ord` and `uno` map
  directly.

**Memory**

- Loads and stores are little-endian. `aligned` is set when `align` is at
  least the access size, and `notrap` is forwarded.
- **Volatile** accesses are never `notrap`. Each is bracketed by
  `sequence_point`, which emits no machine code but is a fence for
  Cranelift's alias analysis. Speed-level load elimination and dead-store
  elimination cannot merge or remove them; a disassembly test with a
  non-volatile negative control verifies this.
- Fixed stack slots map to explicit CLIF stack slots.
- `memcpy`, `memmove` and `memset` become libc calls, as in the native
  backend without small-memory expansion.

**Calls and symbols**

- Direct calls, `call_indirect`, function and data addresses, data
  relocations with addends, and local/exported linkage are all supported.
  - Unreferenced declarations, such as header prototypes, are ignored.
  - Narrow `sext`/`uext` parameters and returns carry CLIF extension
    attributes. Apple's caller-extension rule applies, and both call
    directions are tested.

**Vectors and traps**

- All six 128-bit vector shapes are supported, with splat, extract, insert,
  shuffle, compares, bitcasts and integer/float lane arithmetic.
  - `vshuffle` becomes a byte shuffle on `i8x16`.
  - `vselect` first normalizes any nonzero mask lane to all ones, then uses
    `bitselect`.
  - Integer vector reductions (add, and/or/xor, signed/unsigned min/max)
    extract lanes and fold at the lane width.
  - Keel's per-lane shift amounts have no CLIF equivalent. Vector shifts
    are split into lanes; this is correct but slow.
- `trap` becomes CLIF `trap` (`udf`, SIGILL), while the native backend uses
  `brk` (SIGTRAP). Both terminate abnormally, and the tests check every trap
  case per backend.

## Explicit limitations (rejected with an error)

**ABI**

- Variadic signatures, both calls and definitions. Apple puts variadic
  arguments on the stack, and CLIF cannot express that.
- `returns_twice` (setjmp).
- Aggregate `byval`/`sret`, `i128` and vector parameters or returns, and
  multiple returns.

**Instructions**

- `stack_alloc`, `stack_save`, `stack_restore` and `va_start`; CLIF has no
  dynamic allocation.
- `frame_address` and `va_arg`.
- Atomics and fences.
- Inline assembly.
- Guaranteed tail calls, which need CLIF's `tail` convention on both sides.
- Exception handling.
- Vector `clz`, `ctz`, `popcnt` and `bswap`.
- Switch on `i128` or `ptr`.

**Module features**

- Thread-local data.
- Debug information. Bare lexical scopes left by the inliner are ignored,
  because they carry nothing.

**Scope**

- Only Apple arm64 Mach-O. There is no x86-64/ELF profile and no unwind
  info.
- Scope is kernels, not C applications. Lua 5.4.7 coverage is reported
  only as a translation statistic: 980 of 1107 functions are accepted, and
  the rest fail on variadic calls (126) or setjmp (1).

## Measurement method

Both backends compile the same parsed and verified `keel_ir::Module`
in-process. No IR passes run inside the harness.

**IR variants**

- `raw` is `keel cc --emit-ir`.
- `O` is `keel cc -O --emit-ir`, the full default pipeline including
  loop vectorization.

**Native configurations**

These use the driver's `--emit-object` composition: `TunedSelector`
generic, `AllocationTier::Full`, then `assemble_program_object`.

- `keel-baseline`: baseline selection, as in `keel compile`.
- `keel-full`: all selection passes, which `-O` adds.

**Cranelift configurations**

`clif-none` and `clif-speed`, the two `opt_level` settings.

**Stage boundaries**

The boundaries differ between the backends, so only totals are directly
comparable:

- Native: `codegen` covers selection, register allocation and frame layout
  into a typed assembly program. `object` covers the integrated assembler's
  encoding plus Mach-O writing.
- Cranelift: `setup`, then `translate` (symbol declaration and CLIF
  construction), then `codegen` (`Context::compile`, including machine-code
  emission), then `object` (definitions plus Mach-O serialization).

**What timing includes and excludes**

- Parse and verify is timed once and excluded from both.
- No external assembler or linker is involved. Linking for execution uses
  Clang and ld for every configuration identically, outside any timing.
- Timed objects are byte-checked against the CLIF-verified objects that
  passed the correctness gate.

**Runtime**

- Runtime is exploratory: in-process loops on a local desktop, seeded
  random interleaving, and no core pinning or physical-page control.
- Clang `-O2` runtimes are a C→object reference, not a same-IR result.
- Publication-quality runtime claims need the fixed-page protocol on x86-64
  Linux (see `docs/executable-page-backing.md`), which was not run.

`k_stackobj` accumulates and returns unsigned 64-bit values. Its arithmetic wraps
modulo 2^64 for the driver's full-range signed inputs; the input sign still chooses
the accumulator. Earlier signed accumulators overflowed on those inputs. Rebuild
both compilers' objects when comparing results across this source change.

## Three-input conversion harness

The Rust `keel-ir-harness` accepts Keel, LLVM and CLIF input and exercises
round trips with execution checks. `keel-clif-import` adds a verified scalar
CLIF importer and an independent CLIF execution oracle. See
[the harness contract and commands](tools/ir-harness.md) for the initial subset,
evidence format and remaining feature-loss work.
