# x86 code generation

This reference describes current implementation contracts and limits. Use
[testing](../../development/testing.md) for validation commands; per-run results belong outside Git.

<a id="avx-widen-fallback-avx-only-vector-widening-fallback"></a>
## AVX-only vector widening fallback

When AVX is enabled but AVX2 is not, generic vector lowering must still
produce every lane of a 256-bit integer widening result. This page describes
that fallback and how it is tested. It adds no new hardware operation or
feature; the default vector projection remains 128 bits.

<a id="avx-widen-fallback-the-problem"></a>
### The problem

Minimal IR:

```text
func @minimal(ptr,ptr) {
block0(v0:ptr,v1:ptr):
 v2 = load.i32x4 align 1 v0
 v3 = swiden_low.i64x4 v2
 store align 1 v3,v1
 return
}
```

With AVX enabled and AVX2 disabled, input `[INT32_MIN, -1, 17, INT32_MAX]`
must produce four sign-extended i64 lanes
`[-2147483648, -1, 17, 2147483647]`. Earlier lowering initialized only the XMM
half with the SSE2 unpack recipe and then stored the full YMM register, leaving
the upper two lanes wrong.

<a id="avx-widen-fallback-lowering-rule"></a>
### Lowering rule

`avx256::keep` correctly declines integer widening without AVX2. The following
128-bit packed guard, however, only inspected operand widths and splat result
widths, not conversion result widths, so a 128-bit source with a 256-bit result
reached the unconditional low-widening keep rule. The guard now includes
`Convert.ty`, so the shared lane legalizer initializes every result lane.

- AVX transport (256-bit moves, loads and stores) remains permitted.
- AVX2 integer instructions remain excluded by the typed, operand-dependent
  feature checks.
- Experimental context and caller contracts are unchanged.

IR semantics: the verifier permits wider integer destination lanes with at most
as many lanes as the source. Low widening reads the first destination-lane
count of source lanes; high widening reads the last. High widening already
scalarized without AVX2. Width-changing bitcasts are invalid IR.

<a id="x86-asm-clobber-optimization-clobber-aware-inline-assembly"></a>
## Clobber-aware inline assembly

X86 inline assembly preserves the GPR homes its lowering sequence may
overwrite: declared register clobbers, assigned operand registers and the RAX
staging scratch. Memory and flags clobbers do not add GPRs to that set.
Saved homes occupy compact frame slots. Only affected callee-saved registers
are added to the enclosing function's prologue and unwind descriptions.

Templates without register operands, outputs or GPR clobbers need no staging
frame. This includes `asm volatile("sti" ::: "memory")` and compiler-only
memory barriers. Immediate-only templates also avoid staging. The assembler
still validates their instructions and target features.

Register inputs are staged before operand initialization. An input whose home
is preserved is read from its saved slot; other inputs use their untouched
homes. Output and flag capture precede restoration, then SSA results are
installed. The implementation retains support for tied/early outputs, fixed
register cycles, rematerialization, forced spills and reserved-stack calls.

<a id="x86-asm-clobber-optimization-optimizer-integration"></a>
### Optimizer integration

`InstData::memory_effects` now uses the declared asm contract:

| Declaration | Memory effect |
|---|---|
| Register/immediate operands and GPR/flags clobbers only | None |
| Read-only memory operands | Read |
| Write-only memory operands | Write |
| Read/write memory operands, or a mixture of reads and writes | ReadWrite |
| `memory` clobber or reserved-stack dependency | Barrier |

The existing `memory-forward` and `load-reuse` passes can retain facts across
register-only and read-only templates. Explicit memory writes still invalidate
their facts conservatively, including facts about other addresses. Existing
consumers such as load narrowing also receive the more precise effects.
Asm remains pinned and is never made pure, deleted by DCE or freely moved by
GCM. Explicit stack-observer and aggregate-escape restrictions remain.
Condition-code clobbers remain distinct from memory effects.

These rules follow the programmer's declarations; they do not inspect the
template to infer missing effects. GNU basic asm already receives conservative
memory/flags clobbers in the C frontend and promises GPR preservation.
See GCC's [extended asm contract](https://gcc.gnu.org/onlinedocs/gcc/Extended-Asm.html)
and [basic asm contract](https://gcc.gnu.org/onlinedocs/gcc/Basic-Asm.html).

<a id="x86-avx-subset-x86-avx-subset"></a>
## x86 AVX subset

AVX is opt-in through `x86::Features { avx: true, ..Default::default() }`
and the feature-aware standalone/typed assembler APIs. The ordinary APIs reject
these opcodes. Compiler selection uses `FeatureSelector`; the CLI contract is
`--target-features avx`. This is a subset; it does not implement all of AVX or AVX2.

The typed assembler parses and displays these Intel-syntax forms:

- `vaddss/sd`, `vsubss/sd`, `vmulss/sd`, `vdivss/sd`, `vminss/sd`,
  `vmaxss/sd`, `vsqrtss/sd`: `xmm_dest, xmm_merge, xmm_or_scalar_memory`.
- `vaddps/pd`, `vsubps/pd`, `vmulps/pd`, `vdivps/pd`:
  `xmm_dest, xmm_lhs, xmm_or_128_bit_memory_rhs`.
- `vandps/pd`, `vorps/pd`, `vxorps/pd`: three XMM operands, with an optional
  128-bit memory final source.
- `vroundss/sd`: `xmm_dest, xmm_merge, xmm_or_scalar_memory, imm8`.
- `vucomiss/sd`: `xmm, xmm_or_scalar_memory`.
- `vmovss/sd`: three XMM registers for merging register moves, or two operands
  for scalar memory loads/stores. Two-register scalar moves and three-operand
  memory moves are rejected. Scalar loads zero the rest of the destination XMM.
- `vmovaps/ups/apd/upd`: XMM register moves and 128-bit loads/stores.
- `vzeroupper`: no operands.

Memory widths may be inferred or explicitly match the instruction. Only XMM
registers are accepted. Alignment requirements of aligned moves remain the
caller's responsibility, as with the existing SSE moves. The shared address
encoder supports the existing 64-bit GPR/SIB/displacement, FS/GS, RIP-relative,
GOT and ELF relocation forms; the existing end-of-instruction fixup correction
also accounts for `vround*`'s trailing immediate.

The VEX helper emits W=0 and L=0, including WIG/LIG encodings. It inverts R/X/B
and the source register bits, uses encoded vvvv=1111 for reserved fields, and
selects VEX2 when map=0F and no X/B extension is needed; other cases use VEX3.
Scalar rounding uses map=0F3A. Register moves may use the equivalent store opcode,
and commutative bitwise sources may swap, to obtain VEX2. Floating arithmetic
operands never swap. No REX or legacy mandatory prefix accompanies VEX.

The compiler selects AVX for scalar fadd/fsub/fmul/fdiv and fsqrt, and packed
f32x4/f64x2 vfadd/vfsub/vfmul/vfdiv. Packed arithmetic preserves every lane and
the written source order, including add/mul and destination aliases. Binary
instructions directly write the allocated destination, including when it aliases
the second source, eliminating two-address copies. Both output modes use the
same typed instructions. Baseline selection stays unchanged. There is no FMA,
reassociation, allocator/ABI change, or change to min/max, NaN handling, sign-bit
operations, rounding or saturating conversions. Those existing paths can still
use SSE. Scalar upper XMM lanes are not IR values.

The compiler does not insert `vzeroupper`: it allocates only XMM state and emits
only 128-bit VEX instructions, which do not dirty upper YMM state. Explicit
assembly can request it. This is not a policy for future YMM allocation or for
functions containing opaque user-authored 256-bit instructions.

<a id="x86-avx-subset-packed-arithmetic-extension"></a>
### Packed arithmetic extension

The packed extension reuses the existing feature selector and address encoder.
It adds no vector shapes, attributes, ABI rules, fast-math behavior or upper YMM
state. Baseline SSE2 selection is unchanged; packed sqrt and non-arithmetic
vector operations keep their existing selection. In particular, the VEX2
bitwise-source optimization is restricted to AND/OR/XOR and never applies to
packed FP arithmetic, even when source exchange would shorten the encoding.

The encoding matrix includes all eight packed arithmetic mnemonics in every
XMM field, register aliases (including destination equal to rhs), high registers,
128-bit memory sources, SIB/displacement boundaries and RIP/GOT relocations.
Typed malformed-register rejection and textual malformed-operand/feature
rejection cover the new forms. The codegen test
`packed_avx_selection_preserves_sources_and_removes_two_address_copies` compares
baseline versus AVX instructions, copies and text-section byte counts under
both allocation tiers, with lhs/rhs supplied in ABI registers in reversed order.
The vector runtime tests additionally exercise AVX without requiring BMI,
including unaligned lane data, NaNs, signed zero, infinity, normal/subnormal
values, calls, forced spills in both allocator tiers, vector arguments and
returns. Those runtime tests remain guarded by Rust's AVX CPU/OS support check.

Run the encoding and runtime tests with:

```sh
cargo test -p keel-asm --test x86_avx
cargo test -p keel-codegen --test vector_x86
```

<a id="x86-avx2-direct-add-avx2-direct-destination-packed-integer-adds"></a>
## AVX2 direct-destination packed integer adds

Generic 256-bit integer `viadd` emits

```text
vpaddb/w/d/q destination, left, right
```

directly into the allocated result register, instead of staging both operands
and publishing a scratch result. All four integer lane widths share this path.
The optimizer, optimization context, default vector width, address folding,
ABI and FP selection are unchanged. Subtraction and other wide operations keep
their existing recipes.

<a id="x86-avx2-direct-add-why-this-is-safe"></a>
### Why this is safe

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

<a id="x86-avx2-shuffle-selection-avx2-half-local-dword-shuffle-selection"></a>
## AVX2 half-local dword shuffle selection

For equal resolved operands, generic `i32x8` and `f32x8` shuffles select

```text
vpshufd ymm, ymm, imm8
```

when both 128-bit halves use the same four-dword control. Repeated indices are
supported. Either concatenated-source spelling of an index is accepted, but
only when both operands resolve to the same value. For example
`[3,2,1,0,7,6,5,4]` selects immediate 27.

<a id="x86-avx2-shuffle-selection-behavior"></a>
### Behavior

- The selector stages the source with the existing wide helper, writes the
  reserved FS0 register, and publishes through the existing result helper.
- Requires AVX2. v1/v2 continue rejecting 256-bit transport; AVX without AVX2
  uses the existing legal lane fallback.
- Cross-half masks, different high-half controls, distinct sources and other
  element widths keep the general shuffle code (`vpshufb`).
- No optimizer, feature policy, memory access, FP arithmetic or MXCSR behavior
  changes. Unaligned loads and stores keep their exact widths.

<a id="x86-c-leaf-frames-native-x86-64-c-leaf-frames"></a>
## Native x86-64 C leaf frames

The SysV backend can omit the frame for `cc(c)` SIMD leaves and `cc(fast)`
leaves. Scalar C leaves retain their frames: a controlled LZ4 compression
comparison found that removing those frames regressed the application after
moving unrelated code. The C extension stays limited to SIMD functions.
An eligible function needs no stack storage, incoming stack arguments,
callee-saved registers, special-purpose parameters, TLS addresses, debug files,
or exception personality. Calls, including helper calls for wide division and
software FMA, retain frames. Intrinsics, inline assembly and exception
instructions also retain frames. This keeps dynamic stack allocation and frame
observers on the existing frame path.

A single-block function containing only a direct `return_call` can also omit
its frame and all argument moves when its signature exactly matches the callee
and each argument forwards the corresponding entry parameter. This includes
incoming stack arguments and aggregate parameters/results. Debug files,
variadic signatures, `returns_twice` and exception personalities keep the normal
path. An explicit tail terminator already permits discarding the activation;
this shortcut does not turn ordinary calls into tail calls.

A frameless leaf or forwarding tail leaves `rsp` and `rbp` unchanged. Its unwind rules define the
CFA as `rsp + 8`, with the return address at `CFA - 8`. The assembler still emits
an `.eh_frame` FDE. Wide-vector stack alignment and vector spills retain frames.

Parallel copies use reserved `r11` for integer copies and `xmm14`/`ymm14` when
SIMD locations participate. The SIMD temporary preserves every lane through a
copy cycle. Memory-to-memory transfers use the separate `xmm15`/`ymm15` scratch
register. A function does not reserve a stack copy temporary merely because it
contains ordinary 128-bit vectors. Wide-vector, F80 and intrinsic recipes
keep their existing staging slot conservatively. This does not change which
registers the allocator may use.

`cargo test -p keel-codegen --test x86_c_leaf --test fast_cc` checks frame
eligibility, typed/text object equality, exact leaf CFI and emitted CIE/FDE rules,
and forced register and spill copy cycles at 128 and 256 bits. On Linux x86-64,
the cycle test also links C callers and checks all lanes. The 256-bit runtime
case requires AVX2. These tests do not measure performance or exercise
asynchronous unwinding through a running leaf.

<a id="x86-compiler-output-bounded-x86-compiler-output-instruction-support"></a>
## Bounded x86 compiler-output instruction support

These assembler additions let Keel assemble the Clang-generated Lua and SQLite
assembly used by the application suite. They extend the assembler and shared
assembly model only; instruction selection is unchanged, and this is not full
compiler-output compatibility.

`cmpeqsd`, `cmplesd`, `cmpneqsd`, and `cmpordsd` select legacy `cmpsd`
predicates 0, 2, 4, and 7. They take an XMM destination and an XMM or
64-bit memory source, without an explicit immediate. Existing `cmpsd` and
less-than aliases retain their behavior, including RIP relocation addends
that account for the trailing predicate byte.

The x87 register class represents `st`/`st(0)` through `st(7)` at 80 bits.
It cannot serve as a general register, address base, or index. The supported
instruction forms are:

- `fld` and `fstp`: one stack register or explicit 32/64/80-bit floating memory.
- `fxch`: one stack register, or no operands for `st(1)`.
- `fucomi` and `fucomip` (`fucompi` is an alias): explicit `st(0), st(i)`,
  one `st(i)` with implicit first operand `st(0)`, or no operands for
  `st(0), st(1)`.
- `fadd` and `fmul`: explicit register pairs containing `st(0)`, one `st(i)`
  with implicit destination `st(0)`, or explicit 32/64-bit floating memory.
- `fmulp`, `fsubp`, and `fsubrp`: explicit `st(i), st(0)`, one destination
  `st(i)`, or no operands for `st(1), st(0)`.
- `fcmovnb`: explicit destination `st(0)` and source `st(i)`.
- `fild` and `fistp`: explicit 16/32/64-bit integer memory.
- `fldcw` and `fnstcw`: 16-bit control-word memory. `fldz` has no operands.

The shared model and formatter use Intel destination/source semantics.
AT&T reverses explicit operand order, uses `s/l/t` floating-memory suffixes
and `s/l/ll` integer-memory suffixes, and infers the fixed 16-bit width of
control-word operands. GAS/LLVM AT&T `fsubp` maps to Intel `fsubrp`, and
AT&T `fsubrp` maps to Intel `fsubp`, including one-operand/default forms.
Register prefix modes also apply to x87 registers.

Floating/integer memory widths remain mandatory; ambiguous unsuffixed AT&T
memory forms are rejected. The encoder validates explicit register positions
instead of discarding operands to match permissive assembler aliases.
Other x87 instructions and unsupported directives remain errors. Existing
memory encoding supplies FS/GS, address REX, SIB, displacements and fixups;
x87 data widths never introduce REX.W or a 16-bit operand-size prefix.

<a id="x86-compiler-output-testing"></a>
### Testing

`cargo test -p keel-asm --test x86_compiler_output` compares `.text` and relocation offsets, ELF types,
addends and target names against installed Clang in both dialects. GNU as is
also checked when an existing x86 assembler is found (`KEEL_GNU_AS` overrides
discovery). The tests cover every stack index, both register destinations,
subtraction direction, explicit/default operands, all supported memory widths,
extended addressing, typed/text equivalence, formatting round trips, malformed
typed registers, and invalid operands. These are encoding checks; they do not
execute the x87 code natively.

<a id="x86-constant-vector-address-exact-16-byte-x86-constant-address-loads"></a>
## Exact 16-byte x86 constant-address loads

Constant-offset address selection on x86 normally accepts accesses of up to
eight bytes; its 16-byte exception otherwise applies only to ARM vectors. This
document describes a bounded x86 exception that folds a constant `ptradd` into
an exact 16-byte vector load:

```text
v2 = iconst.i64 16
v3 = ptradd v0, v2
v4 = load.i32x4 v3    ; -> movdqu xmm0, [rdi + 16]
```

<a id="x86-constant-vector-address-why-the-size-gate-is-not-simply-size--16"></a>
### Why the size gate is not simply `size <= 16`

x86 `i128` lowering emits two scalar m64 accesses and reads the original address
through `use_reg`, bypassing the selected-memory helper. Widening the generic
gate would therefore be wrong for `i128`. Other types, stores and indexed-address
selection each need their own audit.

<a id="x86-constant-vector-address-contract"></a>
### Contract

The exception in `select.rs` admits only:

- ordinary, nonvolatile `load.i32x4`;
- whose address is a sole-use, same-block `ptradd`;
- with an i64 constant offset that fits signed disp32 (zero and negative
  offsets included).

It keeps source-line boundaries, rejects a located address feeding an unlocated
load, and respects existing constraints. Offsets outside signed disp32 keep the
materialized 64-bit modular arithmetic. Displacements are never combined or
truncated. Signed disp32 is sign-extended in the 64-bit effective address,
matching `ptradd`'s modular arithmetic even for wrapping bases.

Two consumers use the selected address, both via `mem` with an exact m128 type:

- the ordinary emitter's single `movdqu`;
- the AVX2 `load.i32x4; swiden_low.i64x4` cover, which copies the selected
  load's operands to its root before allocation and emits one m128 `vpmovsxdq`.

No emitter change is needed. Only pure address construction disappears; memory
order, flags, alignment and access widths stay the same. The base stays live at
the selected consumer, including across calls and spill reloads. Shared
addresses remain materialized. Symbolic bases remain ordinary materialized
leaves; no relocation rewrite is added.

Not affected: other 128-bit vector shapes, every 256-bit load, vector stores,
volatile vector loads, atomics, `i128` and indexed vector addresses (the
indexed selector still accepts only 1/2/4/8-byte accesses). There is no
frontend, optimizer, default-width, FP, C ABI or feature-policy change. Ordinary
loads remain legal at v1/v2; AVX without AVX2 keeps its existing widening
fallback.

<a id="x86-constant-vector-address-testing"></a>
### Testing

```sh
cargo test -p keel-codegen --test x86_constant_vector_address
cargo test -p keel-codegen --release --test x86_constant_vector_address
```

The suite covers signed-disp32 and disp8 boundaries and adjacent rejected
values, i64 extrema, zero/negative offsets, sole/shared addresses, resolved base
aliases, cross-block and debug boundaries, symbolic-base object emission, all
selection-option combinations, exclusion of every other 128-bit vector shape
and a representative 256-bit shape. Private `packed_load` unit tests check leaf
transfer to the widening root and rejection at constraints and debug boundaries.

The exported corpus has 65 functions (13 offsets by five modes: plain, closed
widening, shared address, live result across a clobbering call, and a call
between `ptradd` and load) in 48 configurations: v1, v2, AVX without AVX2 and
v3; three selectors; two allocators; normal and empty register pools. Text,
typed and chunked objects must match and every instruction must fit its
effective feature set.

Setting `KEEL_ADDRESS_EVIDENCE=<dir>` exports the `.o`/`.c` pairs. Run them on
x86-64 Linux with `crates/keel-codegen/tests/x86_widen/run_native.py --dir <dir>`
(add `--cc gcc` for GCC). Each run checks input placements at both
protected-page edges and unaligned addresses, signed patterns including i32
extrema, exact 16-byte reads, overlapping input/output, output canaries,
crossing-page faults before any output write, live values across calls that
clobber all SysV caller-saved registers, and unchanged MXCSR. All 48 bundles
pass with Clang 23 and GCC 16 on x86-64 Linux.

<a id="x86-mask-test-adjacent-i32-maskzero-test-selection"></a>
## Adjacent i32 mask/zero TEST selection

The x86 backend selects `TEST r32, imm32` for an i32 AND with a literal mask
whose only use is an Eq/Ne comparison against zero:

```text
v2 = iconst.i32 15
v3 = and v1, v2
v4 = iconst.i32 0
v5 = icmp ne v3, v4    ; -> test edi, 15 ; setne / jne / cmovne
```

This is an x86 backend selection change only: no frontend, optimizer, ISA-level,
ABI, FP, memory-width or default-target change. AArch64 selection is unchanged.
The [userspace ISA](../README.md), [C intrinsic](intrinsics.md)
and [GNU vector/ABI](../../frontends/c/extensions.md) contracts still apply.

<a id="x86-mask-test-selection-contract"></a>
### Selection contract

`Selection::fold_x86_mask_tests` runs after the supplied selector and before the
existing compare/branch/select fusion. It recognizes:

- an exact i32 AND with a literal i32 mask, in either input order;
- whose result has exactly one resolved use;
- which is an Eq/Ne comparison against literal zero in the same block.

Only intervening constants are permitted. Nothing traverses a call, load, store,
fence, intrinsic, raw expansion, FLAGS region or block boundary.

Excluded:

- existing covers, machine matches, compare-condition covers and memory-RHS
  covers on the AND or compare;
- constraints on the AND, compare or intervening constants;
- debug functions, variables and instruction locations (these keep the
  materialized path);
- removed/aliased definitions and previously covered source producers;
- shared AND results, other predicates, nonzero comparisons, register masks and
  i8/i16/i64 widths.

<a id="x86-mask-test-lowering"></a>
### Lowering

The recipe stores the literal mask bits separately and publishes the resolved,
unmasked source as the compare's allocation operand. The encoded zero RHS is
recorded as already folded, so later folding cannot remove an equal-valued
source operand. Existing branch/select fusion transfers this leaf to its own
live boundary; the original source stays live for any other users.

`x86_64::compare` reloads the source normally and emits register
`TEST r32, imm32`, returning Eq/Ne's ZF condition to the existing SETcc/Jcc/CMOV
machinery. No new flag lifetime, memory form, narrowed TEST or alternate
predicate is introduced. User loads, extensions and stores stay at their
original positions and widths; spill completion, boolean canonicalization and
ABI transport are unchanged.

<a id="x86-mask-test-testing"></a>
### Testing

```sh
cargo test -p keel-codegen \
  --test x86_mask_test --test x86_compare_select \
  --test x86_memory_cmp --test x86_memory_rhs
```

Repeat with `--release`. `crates/keel-codegen/tests/x86_mask_test.rs` covers
zero, all-one, sign-bit, alternating, small and arbitrary masks; Eq/Ne; both mask
input orders; resolved SSA source aliases; materialized booleans, branch and
select fusion, shared conditions, nonadjacent consumers and calls after boolean
capture; and composition with the [signed byte load](codegen.md)
cover. All three selectors, both allocators, normal/one-register/empty pools and
v1/v2/v3 must produce identical text/typed/chunked objects, with no raw text
fallback and per-instruction feature legality. An ARM control retains AND.

On x86-64 Linux the suite also executes 100 functions in 54
pipeline/feature/pool configurations, linked with both GCC and Clang. Unsigned C
oracles check extrema and random inputs; calls overwrite all caller-saved GPRs;
byte/word/dword reads are checked at both protected-page edges, including
faulting crossings with marker stores proving the fault position.

<a id="x86-mask-test-application-impact"></a>
### Application impact

Static counts from QuickJS, Lua and SQLite compiled at Full optimization (v1,
v2 and v3; optimized IR is unchanged):

| Translation unit | New register TESTs | TU `.text` change              |
|------------------|--------------------|--------------------------------|
| Lua `lvm.c`      | 13 (11 in `luaV_execute`) | unchanged (39,732 bytes) |
| QuickJS `quickjs.c` | 50              | -96 bytes (v1/v2), -112 (v3)   |
| SQLite `sqlite3.c`  | 66              | -96 bytes (v1/v2), -112 (v3)   |

Counts map each emitted TEST to its function's encoded bytes with an immediate
ZF consumer; raw linear disassembly is unreliable here because it decodes inline
jump tables as instructions. Byte changes include allocation and alignment
effects; an individual TEST does not necessarily save bytes (for example
`and edi,15; cmp edi,0` and `test edi,15` are both six bytes).

Rebuilt Lua (`all.lua`), QuickJS (closure/language/builtin/loop/bignum tests)
and SQLite (query corpus) pass with both GCC and Clang links. No runtime timing
was measured, and there is no speedup or general code-size claim.

<a id="x86-memory-compare-adjacent-x86-integer-memory-operands"></a>
## Adjacent x86 integer memory operands

Tree selection can encode the right operand of an integer comparison directly
as memory instead of materializing an adjacent load:

```text
v2 = load.i64 v0
v3 = icmp.ult v1, v2
```

This becomes `cmp register, qword ptr [address]`.

<a id="x86-memory-compare-comparisons"></a>
### Comparisons

Ordinary i8, i16, i32 and i64 loads can fold, with a single resolved use and
immediately adjacent instructions in the same block. It preserves the original subtraction
operand order and therefore every signed/unsigned comparison condition. The
load still reads exactly its original byte width at its original position in
the IR effect order; neither alignment nor `notrap` is assumed. No intervening instruction,
including a pure instruction or another read, is crossed.

Volatile and atomic operations, shared results, left-hand loads, 128-bit loads,
aliased result definitions, and debug-located functions keep the
existing lowering. A function with source locations, a declaration location,
or debug variables is excluded so independently breakable source positions and
debug materialization do not change. Baseline selection and the ARM target are
unchanged. Trees/full selection enable the rule, including the normal `-O`
selection path.

Selection transfers the load's actual address leaves to the comparison before
allocation. Existing pointer-offset selection remains usable. Compare/branch
fusion then transfers the same leaves to the branch. Both register allocation
tiers and fully spilled inputs use separate scratch registers for the integer
left operand and memory address. The text and structured-object emitters share
the same selected CMP; no opaque assembly peephole or new instruction is added.

<a id="x86-memory-compare-arithmetic-with-a-memory-rhs"></a>
### Arithmetic with a memory RHS

The same adjacency and effect guards permit an integer arithmetic RHS to use
memory: `iadd`, `isub`, `and`, `or`, and `xor` at i8/i16/i32/i64 widths, and
`imul` at i16/i32/i64 widths. The two-operand IMUL encoding has no byte form,
so i8 multiplication stays materialized. No operand commutation is needed;
subtraction retains `lhs - loaded_rhs`. Narrow arithmetic accesses its exact
byte width and explicitly zero-extends the modular result before moving or
spilling it. Signed extensions and signed comparisons continue to interpret
the low IR-width bits.

The selected ALU publishes its lhs and every selected address leaf before
allocation, preserving duplicate occurrences when lhs also supplies the index.
When result and lhs coalesce to the same register, that register is the ALU
accumulator. Memory address reloads touch only reserved RCX/RDX; the ALU reads
all address inputs before writing its result, so a coalesced lhs may also supply
an address leaf. Otherwise the accumulator uses reserved RAX and copies to the
allocated destination only after the memory operation. Thus a distinct destination
overlapping an address leaf cannot destroy the address before the read. Narrow
normalization uses the actual accumulator. This holds for Fast/Full allocation,
forced spills, and experimental rematerialization. Existing larger tree covers
retain precedence. Trees disabled, unsupported widths, debug functions, or any
intervening instruction retain the existing load and arithmetic lowering.
A shared i32 load used by a comparison and a shift store remains one materialized
read; selection never duplicates it to create a memory operand.

<a id="x86-memory-compare-indexed-addresses"></a>
### Indexed addresses

Indexed scalar memory selection also composes with this rule. A single-use,
same-block `ptradd` with an i64 offset can become `[base + index * scale]`
at the original load or store. A single-use i64 shift can supply scales 1, 2,
4, or 8 after masking its count modulo 64. Scalar integer, pointer, and float
accesses keep their exact 1/2/4/8-byte widths; atomic and 128-bit/vector accesses
remain excluded. Shared or cross-block addresses stay materialized, and narrow
shifts/sign extensions retain their own arithmetic. Address producers respect
the existing file/line source boundaries. Volatile accesses retain their
original effects and ordering, while volatile loads remain excluded from CMP
folding.

Both address leaves are allocation operands at the memory consumer, including
when that consumer becomes a comparison or fused branch. Indexed address reloads
use reserved RCX and RDX separately; integer store data, ALU accumulators, and
compare left operands remain in RAX or allocated registers. The backend validates that allocation pools
exclude these scratch registers, and rematerialization writes only its requested
scratch. A load destination can overlap either allocated address leaf because
the memory instruction reads its address before writing the result.

<a id="x86-memory-compare-testing"></a>
### Testing

```sh
cargo test -p keel-codegen \
  --test x86_memory_cmp --test x86_memory_rhs --test x86_indexed_memory
```

- `x86_memory_cmp` tests all ten conditions, branch arguments, materialized
  booleans, address offsets, both allocation tiers and forced spills, fallback
  boundaries and direct versus text-assembled objects. Its Linux native test
  compares against independently compiled Clang oracles and checks a valid final
  eight-byte access plus a protected-page crossing fault.
- `x86_memory_rhs` checks arithmetic/compare widths, subtraction order, lhs/index
  liveness, source/effect barriers, shared and resolved aliases, allocator
  pressure, disabled options and LLVM assembler parity. Its C oracle runs across
  seven selector/allocator settings, including calls, exact-width page
  boundaries, fault ordering, signed narrow normalization, materialized and
  branched comparisons, and stack addresses.
- `x86_indexed_memory` checks all scalar widths, legal scales, masked counts,
  selection options, both allocators, forced spills, overlap, source boundaries,
  fallbacks, optimizer interaction and LLVM assembly encoding. Its C oracle
  checks modular address wrap, calls across address formation, byte-exact
  stores, floats and protected-page faults and ordering.

The C oracles run on Linux x86-64 or macOS arm64. On arm64 they validate only the
oracle and the unchanged ARM lowering; native x86 correctness requires the Linux
run. Deliberate-fault children set `PR_SET_DUMPABLE=0` so no crash reporter is
involved.

<a id="x86-memory-compare-performance"></a>
### Performance

A manual QuickJS experiment that motivated the CMP fold measured 14.14% less
process wall time in one session on an AMD x86-64 machine; a second machine was
inconclusive. Those runs include function and linked-code layout changes and do
not establish isolated CMP latency or the profitability of every application of
the rule. No CPU-specific profitability tuning is applied, and no performance
improvement is claimed for the arithmetic and indexed-address simplifications.

<a id="x86-signed-byte-load-adjacent-signed-byte-load-to-i32"></a>
## Adjacent signed byte load to i32

The x86-64 backend folds an adjacent `load.i8` + `sext.i32` pair into a single
`movsx r32, byte ptr [address]` instead of a separate byte load and register
extension.

```text
v1 = load.i8 v0
v2 = sext.i32 v1      ; -> movsx eax, byte ptr [rdi]
```

<a id="x86-signed-byte-load-selection-contract"></a>
### Selection contract

The only admitted shape is an ordinary nonvolatile `load.i8` followed by
`sext.i32`, with exactly one resolved use of the loaded byte. Both results must
remain their own canonical definitions.

- The pair must be in one block, with identical complete source locations
  (file, line, column and scope, including absence of a location).
- Only intervening integer constants at that same location are skipped;
  constrained, matched, condition-owned or removed constants are boundaries.
  Any other instruction separates the pair. Loads, stores, calls, atomics,
  fences, raw regions and potentially faulting operations are never crossed.
- Existing covers, matches, constraints and condition ownership on either
  endpoint reject the pair.
- Raw IR use counts resolve aliases and include uses outside the pair.

The cover runs after existing address and packed-load selection and before
register allocation. It transfers the load's complete selected operand list to
the conversion, preserving every constant-address, indexed-address and symbolic
address leaf. Existing compare/binary/packed memory covers remain separate.

Not affected (these keep their existing paths): word/dword loads, other
extension result widths, unsigned extension, shared loads, volatile loads and
atomic loads.

<a id="x86-signed-byte-load-emission"></a>
### Emission

Emission resolves or reloads the address before defining the destination, then
emits `movsx r32, byte ptr [address]`. The memory operand is exactly m8.

- Address/destination aliasing is safe: `movsx edi, [rdi]` and the spilled
  `movsx eax, [rax]` case read the address before writing the destination.
- Indexed spilled leaves use the existing distinct scratch registers.
- The i32 result completes through ordinary `def_done`, including the full
  scalar spill home. There is no byte-sized result-spill shortcut.
- The 32-bit destination write clears the upper 32 bits; a following
  `uext.i64` keeps its separate unsigned-i32 semantics.

The fold adds no feature requirement, FLAGS lifetime, ABI, FP or MXCSR
operation. The original byte access position and width are preserved; no
dereferenceability, `notrap` or alias assumption is introduced. The
[userspace ISA](../README.md), [C intrinsic](intrinsics.md)
and [GNU vector/ABI](../../frontends/c/extensions.md) contracts are unchanged.

<a id="x86-signed-byte-load-testing"></a>
### Testing

```sh
cargo test -p keel-codegen --test x86_signed_byte_load
cargo test -p keel-codegen --release --test x86_signed_byte_load
```

The suite covers:

- A selection/allocation matrix over v1/v2/v3, Baseline/Trees/Full selection,
  Fast/Full allocation and normal, one-GPR and empty register pools
  (54 configurations). Text, typed and chunked objects must agree exactly, and
  operand-dependent feature requirements must fit the requested profile.
  Positive chains contain exactly one m8 MOVSX and no residual m8 MOVZX.
- A Clang assembler oracle for direct, address/destination-alias,
  indexed/high-register and negative-displacement encodings.
- Negatives: shared loads, volatile/atomic accesses, other widths/results,
  unsigned conversion, intervening loads/stores/fences/calls, nonconstant
  arithmetic, raw FLAGS ownership, division, cross-block pairs, debug-location
  boundaries and endpoint constraints.

Setting `KEEL_SEXT8_EVIDENCE=<dir>` exports the objects and a Linux C harness.
Run it on x86-64 Linux with `python3 <dir>/run_native.py --cc gcc` (or
`--cc clang`). The harness checks every byte value at both edges of a guarded
page, direct/displaced/indexed addresses, overlapping input/output, output
canaries, calls that physically clobber caller-saved registers, normal and
forced spills, unchanged MXCSR, i32 ABI returns, symbolic loads and the
following `uext.i64` case. Invalid pointers must fault at the expected address
before any later output write. The expected signed value is computed as
`b < 128 ? b : b - 256` to avoid out-of-range casts.

<a id="x86-signed-byte-load-application-impact"></a>
### Application impact

On QuickJS and SQLite, compiled at Full optimization for v1/v2/v3, the fold
activates on 67 loads in `quickjs.c` and 244 in `sqlite3.c`. Summed static
function size drops by 268 bytes (QuickJS) and 969 bytes (SQLite) at each
level. These are static counts, not dynamic execution counts, and no runtime
profitability measurement was made. The QuickJS test suite and SQLite query
suite produce output identical to native references at all three levels with
both GCC and Clang link drivers.

<a id="x86-signed-register-widen-sse41-signed-low-dword-register-widening"></a>
## SSE4.1 signed low dword register widening

When SSE4.1 is enabled, `swiden_low.i64x2` from exactly `i32x4` selects a single
`pmovsxdq destination, source` instead of the multi-instruction SSE2 recipe.
The change is in `x86_64/vector.rs::vector_convert` and reuses
`V2Opcode::Pmovsxdq`.

<a id="x86-signed-register-widen-contract"></a>
### Contract

- Eligibility is the effective `sse41` feature bit, not a CPU-level label.
  V1 and explicit `-sse41` keep the SSE2 recipe.
- The destination is the allocated result register, or reserved FS0 when the
  result is spilled.
- The source is resolved and read before the destination is defined. Register
  aliasing is legal and live allocated sources are not overwritten. A spilled
  input is reloaded from its complete 16-byte vector home before the register
  operation; a spilled result is stored at its full 16-byte width.
- This selector never emits a memory-source PMOVSXDQ or folds a user load. The
  existing m64/m128 load covers (signed widening loads)
  keep priority and their existing memory footprints.

Unchanged: unsigned/high widening, other lane widths, 64-bit vectors, FP
conversions and bitcasts, any 256-bit source or result, PMULLD selection, and
the [AVX-only wide-result fallback](codegen.md). There are no
dispatch, ARM, FP, ABI, tuning or default-policy changes.

The fold improves already formed IR, including vectorized ordinary scalar
loops. It does not change C frontend recognition or vectorizer formation, and
adds no numeric vector C builtin.

<a id="x86-signed-register-widen-testing"></a>
### Testing

```sh
cargo test -p keel-codegen \
  --test vector_widen_x86 --test x86_signed_widen_load \
  --test x86_signed_wide_load --test x86_avx_widen_fallback \
  --test x86_wide_conversions --test vector_widen_aarch64 \
  --test x86_pmulld_selection
cargo test -p keel-codegen --lib feature_guard_tests
```

Repeat with `--release`. `vector_widen_x86` covers v1/v2/v3, v1 plus SSE4.1 and
v2/v3 minus SSE4.1 through all three selectors, both allocators and
normal/constrained/empty vector pools. Controlled layouts test equal/distinct
registers and independent input/result spills. Text, typed and chunked objects
must agree, and every typed instruction must fit the effective feature set. A
Clang encoding test compares all 256 XMM register pairs.

Setting `KEEL_X86_WIDEN_EVIDENCE=<dir>` (and `KEEL_SIGNED_LOAD_EVIDENCE`,
`KEEL_WIDE_LOAD_EVIDENCE` for the load-cover suites) exports ELF objects and C
harnesses. Run them on x86-64 Linux with
`crates/keel-codegen/tests/x86_widen/run_native.py --dir <dir>`. The harnesses
use bitwise sign-extension oracles over signed extrema, every source bit and
random vectors, with aliased outputs, sentinels, live sources across calls that
clobber all sixteen XMM registers, exact 16-byte page edges, faults before
publication and unchanged MXCSR. These are finite value/ABI/footprint checks,
not exhaustive ISA certification.

<a id="x86-simd-selection-generic-x86-simd-selection"></a>
## Generic x86 SIMD selection

This page covers several bounded 128-bit SIMD selection rules in the x86-64
backend: generic shuffles, unsigned byte comparisons, a signed dword widening
load, and dword multiply retention. All of them reuse existing allocation,
spill and typed-assembler paths, and none changes optimizer widths, feature
policy, ABI or FP semantics.

<a id="x86-simd-selection-generic-shuffles"></a>
### Generic shuffles

The generic 128-bit shuffle selector uses existing SSE2 encodings for:

- Every one-source dword permutation, including resolved `[x,x]` inputs.
- Two-source dword shuffles with two low lanes from one source and two high
  lanes from the other, in either source order.
- Alternating-source dword shuffles, in either source order, using two
  `pshufd` operations followed by `punpckldq`.
- All qword permutations; the existing two-source `shufpd` window is retained.
- Byte/word permutations that preserve aligned groups of four bytes.

Classification is shared by vector legalization and final emission. `pshufd`
reads its source before defining the destination; destructive two-source
shuffles stage both inputs in reserved scratch before publishing the result.
These recipes are legal at explicit v1, v2 and v3.

Only register shuffles change. Original loads/stores keep their sizes, ordering
and alignment. The recipes move bits, including NaN payloads and signed zeros,
and do not modify MXCSR. No FP arithmetic, FMA contraction or min/max rewrite
is introduced.

<a id="x86-simd-selection-unsigned-byte-comparisons"></a>
### Unsigned byte comparisons

The unsigned `i8x16` compare recipe builds its sign-bit bias with
`pcmpeqd; psllw 7; packsswb`. Each all-ones word becomes `0xff80` (-128), which
packs exactly into one `0x80` byte; packing the same register as both inputs
fills all sixteen lanes. This replaces six bias-building instructions with
three, using the same reserved scratch register, with no new memory access or
feature requirement. Ordering, operand swapping, canonical masks and non-strict
comparison inversion keep their existing code paths.

`x86_unsigned_byte_compare` checks strict v1/v2/v3 encoding through typed and
textual object paths and compares the bias bytes with Clang. Its exported
oracles (`KEEL_BYTE_COMPARE_EVIDENCE`) check all 65,536 operand pairs at every
lane for all four unsigned ordering predicates, with unaligned/aliased outputs
and live inputs; they pass on x86-64 Linux together with the existing
comparison/mask oracles.

```sh
cargo test -p keel-codegen --test x86_unsigned_byte_compare
```

The ignored `measure_unsigned_byte_compare_codegen` test measures the
four-predicate corpus (same at every CPU level):

| Configuration | Instructions | `.text` bytes | Frame-memory |
| --- | ---: | ---: | ---: |
| Full alloc, normal pools | 120 -> 108 | 469 -> 404 | 8 (unchanged) |
| Fast alloc, normal pools | 124 -> 112 | 471 -> 406 | 8 (unchanged) |
| Full alloc, empty vector pools | 144 -> 132 | 601 -> 536 | 40 (unchanged) |
| Fast alloc, empty vector pools | 148 -> 136 | 636 -> 571 | 40 (unchanged) |

<a id="x86-simd-selection-reproducing-revision-comparisons"></a>
### Reproducing revision comparisons

The measurement tests are ignored by default:

```sh
cargo test -p keel-codegen --test x86_generic_shuffle \
  --test x86_unsigned_byte_compare -- --ignored --nocapture
```

To compare against an older revision that lacks these tests, export that
revision into a scratch directory (for example with `git archive`), copy only
the test files into it, and run only the ignored measurements there: the
ordinary encoding tests assert that the new recipes are selected. Compare the
printed instruction, byte and frame-access counts directly.

Cross-compiling from a non-x86 host checks object generation, typed/text
equality, Clang encodings and C syntax but does not execute the exported
oracles; run those on Linux x86-64 with the native runners above.

<a id="x86-simd-selection-remaining-limitations"></a>
### Remaining limitations

Arbitrary cross-source dword masks and byte/word masks that split dword groups
still use the lane legalizer. D (64-bit) vectors and 256-bit shuffle selection
keep their existing paths (see [AVX2 shuffle selection](codegen.md)
for the half-local 256-bit case). Single-source identity shuffles can still emit
`pshufd` when an earlier IR optimization has not removed them. Two-source
staging is conservative and can add copies. No generalized blend, FP
comparison, narrowing or addressing policy is added.

General widening-load fusion still needs separate profitability and semantic
checks; the signed-dword m64 cover below handles one closed chain. Noncanonical
select masks and other faulting-load shapes remain outside it.

<a id="x86-simd-selection-exact-signed-dword-widening-load-pilot"></a>
### Exact signed dword widening load pilot

The exact-low-eight-byte load cover recognizes this adjacent chain:

```text
v2 = load.i64 align 1 v0
v3 = vsplat.i64x2 v2
v4 = bitcast.i32x4 v3
v5 = swiden_low.i64x2 v4
```

When the effective feature set permits SSE4.1, it emits `pmovsxdq xmm, m64` at
the widen, covering the first three instructions.

Conditions:

- Each intermediate has exactly one use.
- All four instructions keep their own SSA result, share the same complete
  source location, and have no existing selection, condition or allocation
  constraint.
- Volatile loads, nonadjacent operations, shared intermediates, other shapes
  and unsigned/high widening keep existing lowering.
- Explicit v1 and `-sse41` keep the SSE2 fallback; v1 plus `+sse41` permits
  the cover.

The memory operand is exactly the original eight-byte access, at the same
position relative to effects and other accesses; only adjacent pure vector
operations precede the replacement. Address leaves, register allocation,
reserved address scratch, destination allocation and spill publication reuse
the existing low-load path. The instruction changes neither integer flags nor
MXCSR. Optimizer widths, feature policy, general conversion lowering, the
positive32 DAG and the AVX2 recipe are unchanged. The 256-bit AVX2 analogue is
described in AVX2 signed m128 widening load.

<a id="x86-simd-selection-maintained-widen32-measurement"></a>
#### Maintained widen32 measurement

The maintained `suite/runtime-kernels` kernels were compiled with
`cc -O --opt-tier full --cpu-level v1|v2|v3 --emit-object` before and after
this cover, linked with the same Clang `-O2` driver, and checked for
correctness before timing (cases `widen32`, `positive32`, `adjacent`).

At v2/v3 the `rk_widen32` body fell from 106 instructions / 378 bytes to
78 instructions / 256 bytes. The hot loop per eight inputs fell from 43
instructions / 180 bytes to 15 instructions / 61 bytes: four exact m64
`pmovsxdq` loads replace the four chains, removing all sixteen `movaps` copies.
V1 output and all other maintained kernel functions are byte-identical.

Median nanoseconds per call (before -> after), seven samples per cell, on
x86-64 Linux:

| Length | v1 | v2 | v3 |
| --- | --- | --- | --- |
| 257 | 38.97 -> 39.11 | 39.15 -> 13.40 | 38.91 -> 13.40 |
| 4096 | 624.99 -> 622.63 | 622.24 -> 221.14 | 622.55 -> 221.04 |
| 65536 | 10343.84 -> 10326.95 | 10386.62 -> 3404.84 | 10348.92 -> 3448.92 |
| 1048576 | 158800.00 -> 158816.67 | 158756.67 -> 55738.00 | 159533.67 -> 57761.33 |

The v2/v3 median ratio is 2.76-3.05x for these lengths; v1 and the
byte-identical controls stay within 0.9%. This is a warm repeated-input result
on one host; short calls include call/checksum overhead, and code layout,
boost and load are uncontrolled. It is not an application-wide speedup claim.

<a id="x86-simd-selection-effective-feature-dword-multiply-retention-pilot"></a>
### Effective-feature dword multiply retention pilot

The target-private `packed(op, shape, features)` helper recognizes generic
`vimul` on `i32x4` when `features.sse41` is enabled and returns the legacy
`PMULLD` encoding. Legalization retains the SSE2 recipe independently of the single-opcode
helper. Emission uses the effective feature set:

- Explicit v2/v3 and v1+sse41 select `PMULLD`.
- v1 and v2/v3 with `-sse41` use two SSE2 `PMULUDQ` operations. Dword
  shuffles select the even and odd low products; `PUNPCKLDQ` combines them.

The low 32 bits of each product implement the IR's modular integer semantics
for both signed and unsigned interpretation. Existing two-address source
protection, scratch registers and spill publication are reused. There are no
load folds or width changes: original 128-bit loads/stores keep their order
and footprints. The AVX FP helper, FP source order, ABI, release legality
enforcement and optimizer eligibility are unchanged, and no wider
multiplication or multiplication-loop recognition is added.

<a id="x86-simd-selection-private-c-vector-constructors"></a>
### Private C vector constructors

Full optimization recovers complete `i32x4` constructors from the existing
same-block byte facts in `stack-forward`. Four copies of one `i32` value become
`vsplat`; four extracts from at most two `i32x4` snapshots become `vshuffle`.
The sources must remain defined in the same block, including block parameters.
Partial or torn lanes, other scalar expressions, and mixed-width constructors
keep their memory representation. The existing escape, volatile, atomic,
stack-lifetime and debugger-home exclusions apply.

This recovers ordinary C literals such as `(U){s,s,s,s}` and
`(U){a[3],b[0],a[1],b[2]}` before target lane legalization. No producer is moved
or cloned. Private stores disappear only when all reads of the slot disappear.
The separate target selector then chooses the packed shuffle recipe.

<a id="x86-simd-selection-related"></a>
### Related

The [exact 16-byte constant-address pilot](codegen.md)
lets ordinary nonvolatile `i32x4` loads keep a selected base plus signed disp32
through the AVX2 m128 widening cover.

<a id="x86-size-codegen-x86-size-emission"></a>
## x86 size emission

`keel cc -Os` and `keel compile -Os` select size-oriented x86 emission in
addition to the Size IR pipeline. The target-specific changes are:

- Omit implicit 16-byte function alignment and 32-byte loop alignment. Explicit
  function alignment remains a lower bound.
- Omit RBP setup in scalar C leaves with no calls, stack storage, callee-saved
  registers, stack arguments, debug information or stack observers. The existing
  SIMD/private-leaf eligibility checks also apply here.
- Omit unused call-alignment padding in leaves with no calls, local storage,
  stack observers, TLS helper, debug information or stack realignment. Saved
  registers still receive their normal pushes, pops and unwind descriptions.
- Use one-byte `leave` instead of `mov rsp, rbp; pop rbp` when no other
  callee-saved registers need restoring.
- Share a return epilogue when the function saves registers and has multiple
  returns. Each redirected return replaces at least seven bytes with a jump
  of at most five bytes. One existing return owns the shared sequence; no helper
  call, new stack frame or extra epilogue copy is introduced. Return values and
  any AVX transition instruction are prepared before the jump.

Leaf-frame removal can shorten frame-pointer-only backtraces through those
leaves. These choices can trade runtime for size. Other optimization presets keep their
existing emission. The last `-O` preset selects the codegen goal; `--opt-tier`
and `-p` override IR passes independently. For example, `-Os --opt-tier none`
keeps size emission, whereas `--opt-tier size` alone changes only IR passes.
Selector and allocator overrides do not reset the size goal. The goal is not
serialized in IR; apply it again when compiling an IR file.

Library callers set `EmissionOptions::goal` to `CodegenGoal::Size` and use the
existing emission-aware compilation entrypoints. The default remains
`CodegenGoal::Speed`. AArch64 currently has no additional goal-specific emission.
Unwind emission and code models remain independent.

<a id="x86-size-codegen-stack-alignment-and-outlining"></a>
### Stack alignment and outlining

Call sites retain the existing SysV alignment and argument layout. Avoiding
padding in a register-only leaf does not relax the call ABI: the leaf makes no
calls and has no aligned local storage or stack observer. Removing arbitrary
16-byte alignment at private call boundaries would need a separate contract.
The proof must follow every direct and indirect call, stack argument, aligned
spill, TLS helper, intrinsic helper, variadic operation and assembly observer.
A private function that calls external code would still need an aligned call.
Reducing a stack adjustment from 16 bytes to 8 often keeps the same instruction
width. Eliminating an unused adjustment removes an instruction; a relaxed ABI
can instead introduce alignment fixups at its boundaries.
The [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex)
defines those boundary requirements.

Return sharing is a bounded tail merge, not a general machine outliner.
General outlining replaces repeated sequences with helper calls. It needs
encoded-byte costs for the sequence, calls, register preservation and helper
frame. It also needs a contract for implicit registers, flags, stack-relative
addresses, debug/unwind metadata and raw assembly. Keel currently streams one
function at a time, so cross-function matching would need a planning stage or
bounded buffering. LLVM's
[machine outliner cost model](https://llvm.org/doxygen/MachineOutliner_8h_source.html)
likewise includes call and frame overhead.

The next contained candidates are tiny/single-use private inlining with a byte
budget, broader equivalent CFG-tail merging, and shorter instruction forms
where the compiler proves flag and register-width effects irrelevant. General
outlining and relaxed private stack alignment are separate backend work.

<a id="x86-wrapping-reductions-x86-wrapping-add-reductions"></a>
## x86 wrapping add reductions

The x86 backend keeps `vreduce_add` on **i64x2 and i32x4** through shared
vector legalization and selects it with SSE2 only: `pshufd`,
`paddq`/`paddd`, and `movq`/`movd`. No frontend, optimizer, preferred-width or
ABI policy is involved. C vector expressions and intrinsics keep their existing
frontend contracts; this selection applies whenever typed IR contains the
operation.

<a id="x86-wrapping-reductions-lowering"></a>
### Lowering

- **i64x2:** swap the two qwords with `pshufd 0x4e`, then `paddq`.
- **i32x4:** add opposite 64-bit halves using the same shuffle, then add
  adjacent dwords using `pshufd 0xb1`.
- Extract lane zero at the original lane width.

Packed addition wraps modulo 2^64 or 2^32, so this tree produces exactly the
same bits as a sequential lane sum, and signedness does not affect the result.
An i32 result is written through a 32-bit GPR view; subsequent `sext` and
`uext` keep their distinct meanings. No widening sum is implied.

<a id="x86-wrapping-reductions-register-and-memory-contract"></a>
### Register and memory contract

- The input is staged in reserved xmm14 before the scalar result is defined;
  reserved xmm15 holds permutations. Allocated input registers are never
  modified, including inputs live across calls or after the reduction.
- Allocator validation rejects allocations that overlap reserved scratch
  (physical SIMD registers 13, 14 and 15).
- Spilled inputs reload exactly 16 bytes; scalar spill/result protocols are
  unchanged. Unspilled reductions use no lane buffer or lane memory traffic.

<a id="x86-wrapping-reductions-scope"></a>
### Scope

Other reduction operations, i8/i16 lanes, 64-bit vectors, 256-bit reductions
(AVX2 Y reductions keep their existing path) and all FP reductions keep their
existing behavior. FP evaluation order, min/max semantics, the PMULLD helper,
the AVX conversion result-width guard and Apple ARM64 lowering are unaffected.
The feature guard still checks debug and release emission.

<a id="x86-wrapping-reductions-results"></a>
### Results

Native execution on x86-64 Linux passes all exported oracles in debug and
release. This was a newer x86 CPU, not a v1-only machine; v1 legality of the
emitted code rests on feature checks and encoding validation.

In the v1 Full-selection/Full-allocation unspilled `sum` functions, explicit
data-memory operands fell from three to zero (i64x2) and five to zero (i32x4),
and the reserved stack frame fell from 32 to 16 bytes. Instruction counts are
unchanged (12 and 14). Short external-call kernel timings showed overlapping
ranges before and after, so no runtime speedup is claimed.
