# F80 implementation stages

F80 is an x87 extended value, never an alias for F64. `Type::bytes()` is
10, the exact load/store payload. SysV object layouts give it size and
alignment 16; six padding bytes are not part of the value. Existing Fconst
and its u64 payload remain unchanged. `F80const { significand, sign_exp }`
and `fconst.f80 0x3fff, 0x8000000000000001` preserve all payload bits,
including signed zeros, NaNs and noncanonical encodings. Decimal conversion
is deliberately absent until a correctly rounded converter exists.

The first prerequisite accepts lossless IR/text and explicitly rejects F80
at backend entry, including foreign signatures and aggregate layouts.
Subsequent stages will replace x86 rejection only for implemented forms.
AArch64 always rejects this foreign format.

The x86 design assigns a stack home to every F80 SSA value and uses short
x87 sequences. No x87 value survives an IR instruction except an ABI result
in ST0. Calls consume return ST0 immediately, including unused results.
Returns expose exactly ST0; calls and tail jumps otherwise have empty x87
stacks. Payload moves should use integer copies to preserve noncanonical
encodings and avoid unintended exceptions. Arithmetic/conversions may raise
x87 exceptions and obey the inherited x87 control word (precision control,
rounding control, masks); default SysV environment has extended precision.
No change to MXCSR or x87 control word is implicit. Unmasked exceptions may
interrupt a sequence with temporary stack operands present, as in C-generated
code; completed sequences balance TOP. This is a value compiler contract,
not an exact raw x87 instruction/environment or MMX-state implementation.

ABI work must distinguish X87/X87UP classification from argument placement:
long double and its wrappers are MEMORY arguments but ST0 results. Mixed
or larger aggregates use MEMORY. No synthetic XMM return is permitted.
Varargs long double consumes an aligned 16-byte overflow-area object and
never consumes gp_offset/fp_offset. Tail calls require the existing incoming
stack-area checks and return-layout agreement.

Completion remains unproven until Linux C/GCC/Clang execution validates
precision, extrema, rounding modes, spills/phis/loops, aliasing, direct and
indirect calls, tail calls, aggregate results and variadic traffic. Local
cross assembly and textual/typed tests cannot substitute for those runs.

## Implemented x86 stage

The next stage implements stack homes, raw constants and exact-width memory
transport, scalar and X87/X87UP aggregate ABI transport, all scalar float
arithmetic/unary operations, IEEE min/max, copy-sign, explicit FMA through
`fmal`, ordered/unordered comparisons, selects, block arguments/loops, scalar
direct/indirect calls and tails, and SysV overflow-area varargs. F32/F64
extension/truncation uses x87 loads/stores; integer conversions preserve the
existing saturating/truncate/NaN-zero contract. Integer conversion and explicit
rounding temporarily select the required RC and restore the original CW.
Unsigned u64 construction normalizes its exact payload with integer operations;
u64 results in [2^63,2^64) are the explicit significand directly. Neither path
passes through binary64 or depends on reduced precision control.

The F80 homes reserve 16 bytes each; control-word staging reuses the existing
16-byte parallel-copy temporary between instructions. No metadata operation
group or dedicated frame-save slot was introduced. Sparse allocators that try
to put F80 in a register fail closed. Existing aggregate tail restrictions,
i128/float conversion restrictions, and foreign AArch64 rejection remain.

Local validation: workspace offline/locked checking; full keel-ir tests; SysV
classification tests; 108 configurations across v1/v2/v3, baseline/trees/full
selection, fast/full allocation, three pressure pools, and optimized/raw IR.
Every configuration's typed and text assembly produces byte-identical ELF.
The header-free Clang Linux cross oracle independently uses stack MEMORY
arguments and ST0 returns, including the single-F80 aggregate.

To produce and seal the strict native packet from the repository root:

```sh
KEEL_F80_EXPORT_DIR="$PWD/target/f80-packet" CARGO_TARGET_DIR=target/f80-private cargo test -p keel-codegen --test f80 --offline --locked -j2
python3 suite/validate_f80_packet.py seal target/f80-packet
python3 suite/validate_f80_packet.py verify target/f80-packet
```

On x86_64 Linux with independent GCC and Clang installations:

```sh
python3 target/f80-packet/validate_f80_packet.py run target/f80-packet
```

The runner requires every one of 432 compiler/configuration/transport executions
and verifies hashes and the exact finite roster. Its C driver checks 8/10/16
byte format distinctions, extrema/subnormals, precision beyond F64, arithmetic,
NaN and signed-zero comparisons/minmax, conversions and all RC modes, precision
control, CW restoration, TOP balance at external calls/after returns, spills,
loop/phi swaps, direct/indirect/tail calls, aggregate/varargs interoperability,
unaligned/overlapping exact-width payload moves, padding sentinels, guard-page
memory boundaries, a dead volatile-load fault and an unmasked divide fault.
A sealed packet is explicitly native-pending until that runner succeeds.

## Native u64 caller diagnostic

The first native packet passed all 216 GCC executions (4638 checks each), then
Clang failed the reduced-precision assertion whose argument was `0x1p63L+1`.
This is an expression evaluated by the C caller, not a single literal. Local
Clang Linux cross output with `-frounding-math` emits `flds` of 2^63 followed
by `fiaddl` of 1, so reduced x87 PC can round that input before the Keel call.
The single exact literal `0x1.0000000000000002p63L` instead loads the extended
constant payload (significand 0x8000000000000001, exponent 0x403e) directly.
The fixed-input driver assertion now uses that literal. No backend change is
justified by the cross output alone; native diagnostic results are required.

`u64-diagnostic.c` retains the original caller expression alongside the exact
literal. Its 24 rows expose both argument payloads, expected and actual u64,
and CW before/after for PC24/53/64 and all four RC modes. The exact packet
runner executes this diagnostic under both independent compilers before the
432-run main roster and records the rows in the final report. The original
failed packet remains unchanged. `u64-caller-oracle.c` and its sealed Clang
cross assembly make the caller instruction difference independently visible.

## Raw callback entry witnesses

The corrected-input packet again passed all 216 GCC executions, then Clang
failed the callback boundary counter while the caller's post-call TOP check
passed. A header-free Linux Clang cross oracle reproduces why the old inline
measurement is inconclusive: `original_inline_identity` emits `fldt 8(%rsp)`
(and `wait`) before the inline `fnstsw`. That observes the C body's legitimate
return-value load rather than the ABI entry state.

The driver now defines separate noinline C bodies. Three raw assembly shims
sample status before reaching those bodies. Each pushes RAX, records original
AL and the status/count, checks TOP, restores the complete RAX, and tail jumps
to its C body. The push/pop leaves the original argument stack and alignment
intact; AL preservation is essential to the variadic register-save protocol.
Counters/status arrays are volatile C globals because assembly writes them.
The shim also carries CFI metadata for the temporary push.

`entry-diagnostic.c` independently checks identity, aggregate and variadic
callback entries, exact callback counts, TOP/status, original variadic AL,
and actual callback results. Both compiler diagnostics are mandatory before
the 432-run driver roster. The packet includes the original inline oracle's
Clang Linux assembly, the replacement diagnostic assembly, and a cross-assembled
shim ELF. Previous failed packets remain unchanged. Backend code is unchanged;
raw native entry measurements remain the deciding evidence.

## F80-R2-001: preserve sparse allocation dataflow

The parent reports the raw-entry packet passed all 432 GCC/Clang executions,
4638 checks each, with both u64 and raw-entry diagnostics passing. That built-in
allocator proof does not cover custom sparse allocation plans.

The review's custom-parameter repro was compiled before this fix and failed a
focused assertion: incoming stores used `[rbp-48]`, while return FLD used
`[rbp-32]`. Replacing only fallback homes had left the redundant parameter
binding pointing at its original home. Merely accepting any stack binding also
failed to give sparse transfer homes their complete payload copy width.

Sparse F80 functions now expand every original spill unit from eight to sixteen
bytes and rewrite the whole allocation consistently: all fallbacks, parameter
bindings, instruction bindings, and before/after/edge transfers. Existing stack
home identity and custom dataflow are preserved. Copy-width propagation follows
F80 transfer components through otherwise unbound temporary stack slots; every
such home receives 128-bit internal copies and two units of frame capacity.
Exact external payload loads/stores remain ten bytes. Frame overflow and invalid
original capacities reject before emission.

Sparse F80 register bindings, transfer components crossing registers, register
fallbacks with sparse transfers, and transfer homes shared with wide vectors
fail explicitly until type-aware register transitions are supported. Empty-edit
Fast/Full allocations retain their existing dedicated-home behavior. All 216
built-in objects (108 configurations, typed/text) are byte-identical to the
parent's native-proven a06434cf packet.

Focused custom tests cover redundant parameter and instruction bindings;
full-payload before/after/edge copies; transit-only temporary homes; and mixed
integer/pointer/F80 frame mapping. Register parameter/instruction/transfer plans
and invalid capacities are negative cases. The new native packet includes 16
custom configurations (eight plans, baseline/full selection), each checked with
12 exact payload identities: signed zeros, finite values with precision beyond
F64, extrema/subnormals, infinities, and quiet-NaN payload/signs. Its required
native roster is now 496: the original 432 plus 64 custom GCC/Clang typed/text
executions. Custom proof remains native-pending until that finite roster passes.

Produce the expanded packet with both integration tests:

```sh
KEEL_F80_EXPORT_DIR="$PWD/target/f80-packet-sparse" CARGO_TARGET_DIR=target/f80-private cargo test -p keel-codegen --test f80 --test f80_allocation --offline --locked -j2
python3 suite/validate_f80_packet.py seal target/f80-packet-sparse
```

## F80-R3-001 and F80-R3-002: dying inputs and temporal types

The parent reports all 496 executions of the sparse-allocation packet passed,
including its 64 custom identity runs. Two further valid plans were reproduced
locally before this fix. In copysign and equal min/max, source assembly read the
second sign at instruction 5 after the coalesced result write at instruction 3.
The exact F80-to-i64 shared-home repro produced scalar MOV with xmmword memory;
both text and typed assemblers rejected it with operand width/class mismatch.
Those focused regression tests now pass.

Copysign and equal min/max capture the second sign/exponent in reserved RCX
before any payload write whenever the result shares that input's home. The
copy's RAX/RDX scratch cannot destroy that capture. Nonoverlapping plans retain
the existing instruction sequence, including all built-in allocations.

Frame addresses now default to the 64-bit scalar spill transport width.
`copy_slot` alone applies the conservative untyped full-copy metadata. Typed
FP/vector reloads and definitions choose their own widths; GPR, pointer and
integer-half accesses stay 64-bit regardless of a home's earlier F80 use.
Full stack transfers still copy the complete F80 payload in their allocated
16-byte capacity. Dedicated AVX256 constant staging explicitly uses its full
copy width, preserving wide loads and constants.

New native coalescing witnesses use both first- and second-input result sharing
under v1/v3 and baseline/full selection. Each object includes copysign, min/max,
arithmetic and comparisons; temporal F80/integer/F32/F64/pointer reuse; and
scalar integer/float transfers through F80-marked stack components. The C
oracle covers 144 binary pairs with opposite signed zeros and signed quiet-NaN
payloads, 12 temporal floating inputs, saturating conversions, exact integer
and pointer round trips, and mixed transfer components. All 248 prior objects
remain byte-identical (216 built-in and 32 sparse custom). Targeted allocator,
AVX256 and vector-wrapper gates also pass locally.

The strict native roster is now 528: 432 built-in, 64 earlier custom identity,
and 32 coalescing executions, across GCC/Clang and typed/text transports. These
new coalescing rows remain native-pending until the parent runs the packet.

```sh
KEEL_F80_EXPORT_DIR="$PWD/target/f80-packet-r3" CARGO_TARGET_DIR=target/f80-private cargo test -p keel-codegen --test f80 --test f80_allocation --test f80_coalescing --offline --locked -j2
python3 suite/validate_f80_packet.py seal target/f80-packet-r3
```
