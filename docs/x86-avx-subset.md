# Initial x86 AVX subset

AVX is opt-in through `x86::Features { avx: true, ..Default::default() }`
and the feature-aware standalone/typed assembler APIs. The ordinary APIs reject
these opcodes. Compiler selection uses `FeatureSelector`; the CLI contract is
`--target-features avx`. This implementation does not implement all AVX or AVX2.

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

## References and validation

Encoding and operand assumptions follow Intel's [64 and IA-32 Software
Developer's Manual, Volume 2](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html),
section 2.3 (VEX format), and the individual ADDSS/ADDSD, SUBSS/SUBSD, MULSS/MULSD,
DIVSS/DIVSD, ADDPS/ADDPD, SUBPS/SUBPD, MULPS/MULPD, DIVPS/DIVPD, SQRTSS/SQRTSD, MIN/MAX, UCOMISS/UCOMISD, MOVSS/MOVSD, MOVAPS/MOVUPS,
MOVAPD/MOVUPD, AND/OR/XORPS/PD, ROUNDSS/ROUNDSD and VZEROUPPER entries.
In particular, scalar moves have distinct register-merge and memory forms;
unused vvvv fields must be encoded as 1111, and scalar forms use L=0.

`crates/keel-asm/tests/x86_avx.rs` compares a matrix of 57,935 instructions
against Clang and GNU as, including bytes and relocation offsets/types/addends/
symbol names. It covers every XMM register in destination/source fields,
aliases, VEX2/VEX3, extended GPR/SIB operands, displacement boundaries, RIP/local/
external/GOT fixups, segments and trailing rounding immediates. GNU's reference
source explicitly requests the equivalent store-direction moves and bitwise
source ordering that Clang selects automatically, so encoding choices match.
Clang reference source uses `{vex3}` for packed add/mul cases where it would
otherwise exchange FP sources to shorten the prefix. On Linux GNU as is required; other hosts can set `KEEL_TEST_GNU_AS` to a GNU
x86-64 cross-assembler. Clang is always required.

The same file checks default/missing-feature rejection, malformed typed operands,
parse/display roundtrips, and zero-text typed/text object equality across backend
fixtures and selection/allocation tiers. Linux x86-64 runtime tests guard on
`is_x86_feature_detected!("avx")`. They run the existing comprehensive floats
fixture (NaNs, signed zero, saturation, calls and pressure), plus direct XMM
merge/load-zeroing, high-register/SIB, rounding, sqrt and unordered-compare probes.
The tests can be built on other hosts, but these runtime checks execute only on
Linux x86-64 with AVX available.

`crates/keel-asm/tests/fixtures/x86_avx_arithmetic.keel` is a reusable arithmetic
chain in both widths for baseline/AVX output inspection or a caller-driven
microbenchmark. No speedup claim is made without measurements on the target CPU.

The guarded runtime tests passed on the i7-1360P Linux NUC. See the
integration report for
combined AVX/BMI application and baseline compatibility validation.

## Packed arithmetic extension

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
These descriptions identify coverage; execution evidence for this revision is
recorded separately and historical reports above do not validate this extension.

Final packed-extension execution evidence and exact commands/hashes are in
the scoped validation report. Independent review
and coordinator integration remain separate steps.
