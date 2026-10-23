# AVX-only widening result-width repair

Based on main `96a1fad9eac3e9aa24dc42760ae3706aa6c5c14d` (default
vector projection remains 128 bits). This repairs a preexisting numerical bug
in generic vector lowering, independently of the AVX2 widening-load cover.

Minimal valid IR:

```text
func @minimal(ptr,ptr) {
block0(v0:ptr,v1:ptr):
 v2 = load.i32x4 align 1 v0
 v3 = swiden_low.i64x4 v2
 store align 1 v3,v1
 return
}
```

With AVX explicitly enabled and AVX2 disabled, input
`[INT32_MIN, -1, 17, INT32_MAX]` must produce four sign-extended i64 lanes
`[-2147483648, -1, 17, 2147483647]`. Baseline instead initializes only XMM
with the SSE2 unpack recipe and stores YMM, leaving the upper two lanes wrong.
The independent scalar-C oracle exits 2 before the fix and 0 afterward on LinuxHost.

`avx256::keep` correctly declines integer widening without AVX2. The subsequent
128-bit packed guard inspected operand widths and splat result widths, omitting
conversion result widths. A 128-bit source therefore reached the unconditional
low-widening keep rule despite its 256-bit result. The repair includes
`Convert.ty` in that guard, so the existing shared lane legalizer initializes
every result lane. No hardware operation or feature is added. AVX transport
remains permitted; AVX2 integer instructions remain excluded by the typed
operand-dependent feature checks. Experimental context and caller contracts
are unchanged.

The IR verifier permits wider integer destination lanes with at most as many
lanes as the source. Low widening reads the first destination-lane count;
high widening reads the last. All six valid 128-to-256 element-width pairs
are tested for signed/unsigned low/high operations, including non-doubling
widths. High widening already scalarized without AVX2. Width-changing bitcasts
are invalid IR; equal-width wide bitcasts and 64/128-bit conversions remain
controls rather than new lowering work.

`x86_avx_widen_fallback` checks text/typed/chunked object parity and every typed
instruction's required features under v1+AVX, v2+AVX, v3-minus-AVX2, and v3.
It covers all three selection tiers, both allocators, normal/tiny/empty pools,
vector argument/result ABI, live YMM upper halves and live source values,
aliasing, unaligned addresses, sign extrema, distinct lanes, output bytes,
and exact guarded 16-byte input / 32-byte output accesses. Additional controls
cover 64/128-bit results and equal-width 256-bit bitcasts. Strict v1/v2 reject
the wide value on all three output routes, tiers, allocators and normal/empty
pools. Native checks require a feature-checked Linux x86-64 host; export with
`KEEL_AVX_FALLBACK_EVIDENCE` for the existing `run_native.py` runner.

Validation on October 23, 2026 used `cargo test --offline --locked -j2`,
`CARGO_CACHE_LINKS=0`, the default shared cache and checkout-managed target,
serialized locally and on LinuxHost through `suite/fast_opt_locked.py` and
`/tmp/keel-avx-fallback-unique.jsonl`. No performance benchmark was needed.
LinuxHost is an AVX/AVX2-checked Ryzen 9 9950X3D, Linux 7.2.10-1-cachyos.
The C oracle is compiled independently with `-march=x86-64 -mno-avx`.

- Original 72-bundle campaign freshly rebuilt from its saved IR and unchanged
  C harnesses: baseline 54 AVX-only exits 2 / 18 v3 passes; fixed 72 passes.
  All 54 baseline AVX-only objects match the cover campaign byte for byte;
  all 18 fixed v3 objects match the baseline byte for byte.
- Dedicated regression: minimal baseline exit 2; fixed 121 native bundles pass
  (72 widening matrices, 48 narrow/bitcast controls, one minimal repro).
- Fresh native controls pass: 33 SSE2 widening/bitcast bundles, 108 PMULLD
  bundles, 108 signed PMOV load bundles, and 13 low-64 load bundles.
- Local bounded batch: 29 tests pass, two performance tests intentionally
  ignored; adding the narrow/bitcast control brings the dedicated gate to four
  passing tests. Existing wide generic/transport and vector IR suites pass.

Artifacts stay outside Git. Local and LinuxHost directories are
`/tmp/keel-avx-{campaign,fallback}-{before,after}`; each native directory retains
`native-results.json`, ELF objects, assembly, C and IR. Local reports are also
`/tmp/keel-avx-{campaign,fallback}-before-native-results.json` and
`/tmp/keel-avx-campaign-after-native-results.json`.
`/tmp/keel-avx-fallback-comparison.json` records byte comparisons.
Control bundles are `/tmp/keel-avx-fallback-{widen,pmulld,pmov,low64}-controls`.
Logs use `/tmp/keel-avx-fallback-*.log`. This is finite validation on a modern
CPU, not execution on an AVX-only physical CPU. The cover branch still needs
its parent's independent integration/rebase and numerical rerun.
