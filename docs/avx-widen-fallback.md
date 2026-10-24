# AVX-only vector widening fallback

When AVX is enabled but AVX2 is not, generic vector lowering must still
produce every lane of a 256-bit integer widening result. This page describes
that fallback and how it is tested. It adds no new hardware operation or
feature; the default vector projection remains 128 bits.

## The problem

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

## Lowering rule

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

## Tests

`x86_avx_widen_fallback` covers all six valid 128-to-256 element-width pairs
for signed/unsigned low/high widening, including non-doubling widths, under
v1+AVX, v2+AVX, v3-minus-AVX2 and v3. It checks:

- text/typed/chunked object parity and each typed instruction's required
  features;
- all three selection tiers, both allocators, and normal/tiny/empty register
  pools;
- vector argument/result ABI, live YMM upper halves and live source values,
  aliasing, unaligned addresses, sign extrema and distinct lanes;
- exact 16-byte input / 32-byte output accesses against guard pages;
- controls for 64/128-bit results and equal-width 256-bit bitcasts;
- that strict v1/v2 (no AVX) reject the wide value on all output routes.

```sh
cargo test -p keel-codegen --test x86_avx_widen_fallback
```

Native execution requires a Linux x86-64 host with the needed CPU features.
Set `KEEL_AVX_FALLBACK_EVIDENCE=<dir>` to export objects, assembly, IR and C
harnesses, then run the exported `run_native.py`. The C oracle is compiled
independently with `-march=x86-64 -mno-avx`.

Validated on x86-64 Linux (an AVX2-capable CPU running AVX-only code); this is
not execution on an AVX-only physical CPU. The combined check with the AVX2
widening-load cover is described in
[the AVX2 signed widening load cover](avx2-signed-widen-load-pilot.md#validation).
