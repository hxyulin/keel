# Exact 16-byte x86 constant-address pilot

At frozen base `8690a355b2986288e5b53b6401a79ba342cc16f9`, constant-offset
address selection accepts accesses up to eight bytes on x86. Its existing
16-byte exception applies only to ARM vectors. This conservative size gate
cannot simply become `size <= 16`: x86 `i128` lowering emits two scalar accesses
and reads the original address directly, bypassing the selected-memory helper.
Other types, stores and indexed-address selection need their own audit.

The bounded exception in `select.rs` admits only ordinary, nonvolatile
`load.i32x4` with a sole-use, same-block `ptradd` and an i64 constant fitting
signed disp32. It retains source-line boundaries, rejects a located address
feeding an unlocated load, and respects existing constraints. Negative and zero
displacements are admitted; values outside signed disp32 retain materialized
64-bit modular arithmetic. No displacements are combined or truncated.

The ordinary emitter already uses `mem` for one exact m128 `movdqu`. The closed
AVX2 `load.i32x4; swiden_low.i64x4` cover already copies the selected load's
operands to its root before allocation, then uses `mem` for one exact m128
`vpmovsxdq`. No production emitter change is required. Only pure address
construction disappears: memory order, flags, alignment and access widths stay
the same. The base remains live at the actual selected consumer, including when
it needs a spill reload. Shared addresses remain materialized. Symbolic bases
remain ordinary materialized leaves; this adds no relocation rewrite.

There is no frontend, optimizer, default-width, FP, C ABI or feature-policy
change. Ordinary loads remain legal at v1/v2. AVX without AVX2 retains its
existing widening fallback; v1/v2 still reject wide SSA/ABI operations. Release
instruction legality remains enforced. This does not enable other vector
loads, vector stores, volatile vector loads, atomics, i128 or indexed vectors.

## Correctness evidence

`x86_constant_vector_address` tests signed-disp32 boundaries, adjacent rejected
values and i64 extrema; zero/negative offsets; sole/shared addresses; resolved
base aliases; cross-block and debug boundaries; symbolic-base object emission;
all eight selection-option combinations; and excluded operation shapes.
Private `packed_load` tests exercise the real constant-address cover's leaf
transfer to the widening root and rejection at constraints/debug boundaries.

The original exported corpus had 44 functions per configuration: eleven offsets times
ordinary/closed widening, shared address, live result across a clobbering call,
and a load kept live beside its widening. Four profiles (v1, v2, AVX without
AVX2, v3), three selectors, two allocators and normal/empty register pools produce
48 object/C pairs. Text, typed and chunked objects must match; every typed
instruction must fit its effective feature set. The AVX-only profile uses wide
transport and the existing legal widening fallback, rather than AVX2.

On LinuxHost, both frozen base and candidate passed all 48 pairs with each of Clang
and GCC: 192 native harness runs. Each run checks 34 input placements including
both protected-page edges and unaligned addresses, seven signed-value patterns
including i32 extrema, exact 16-byte input reads, output canaries, live values
and unchanged MXCSR. Synthetic integer base addresses exercise wrapping and
extreme displacements without dereferencing the synthetic base. Every reported
object/C hash is retained alongside the results. Symbolic bases are checked by
cross-object parity, not by this native harness.

Existing local debug regression gates passed `scalar_address`, `vector_aarch64`
(including native Apple ARM execution), `x86_avx256_generic`,
`x86_wide_conversions`, `x86_widen_sum` and `x86_avx2_direct_add`. LinuxHost release
passed the scalar/wide/conversion/signed-sum gates and codegen library suite;
the final private cover tests and release emission guard also passed. Clippy
completed with existing library warnings and none in the added test.

Frozen base/candidate comparisons confirm all 12 exported control assemblies
(ARM, scalar, other vector and volatile, across three selectors) are unchanged.
All 16 Baseline-selection objects are byte-identical, as are the 528 individual
shared-address function assemblies across the corpus. No sibling work was
integrated.

## Separate measurements

The unchanged ignored `x86_avx2_direct_add::measure_signed_sum_direct_add` gate
optimizes the maintained signed-sum fixture with explicit wide preference at
both revisions. Its optimized IR is byte-identical. Full selection/allocation
changes **102 to 96 instructions, 350 to 332 text bytes**. The assembly diff is
exactly three removed `mov`/`add` pairs and `vpmovsxdq` operands changed to
`[base + 16]`, `[base + 32]`, `[base + 48]`. Memory instructions retain order
and widths. Counts include function overhead, rather than just the loop body.

After correctness, release compiler-stage samples on LinuxHost used the same
pre-parsed, already optimized module, 100 observations after three warmups.
Median base to candidate microseconds were 70.527 to 55.071 for text codegen,
68.207 to 52.931 for typed codegen, and 155.684 to 113.082 for typed object
encoding/ELF generation. These are single-module observations in sequential
runs, not compiler-throughput claims; parsing, optimization, Rust builds and
process startup are excluded. Cache-dependent Rust build wall time is not
compared.

A separate same-width native driver links both objects under distinct names,
checks a scalar wrapping-sum oracle, alternates their order, pins CPU 4, discards
one warmup round and retains twelve rounds. Median base to candidate ns/call
for lengths 16, 64, 256, 4096, 262144 and 1048576 were respectively:
3.478 to 3.463; 4.621 to 4.282; 9.501 to 8.863; 118.086 to 116.316;
7657.053 to 7455.558; 30953.797 to 30287.037. This small hot-kernel experiment
is not an application speedup claim, nor a width-policy comparison. Independent
guarded signed-sum oracles pass for both objects before timing. Native timing
held the shared investigation lock; recorded load was 0.50 on 32 logical CPUs.
Compiler-stage load was 0.42–0.66. No timing is inferred from cross compilation.

## Reproduction and retained artifacts

Use the existing toolchain, cargo default shared cache and checkout-specific
managed targets, with `CARGO_CACHE_LINKS=0`. For example:

```sh
TMPDIR=/tmp KEEL_ADDRESS_EVIDENCE=/tmp/keel-address-candidate \
python3 suite/fast_opt_locked.py \
  /tmp/keel-address-pilot-build.jsonl cargo test \
  --offline --locked -j2 -p keel-codegen --test x86_constant_vector_address
```

Archive the frozen base into a disposable `/tmp` source directory and copy only
the new dedicated integration test and its C fixtures into it. At base, run
`exact_loads_features_parity_and_native_exports` and `export_unchanged_controls`
individually (the new selection assertions intentionally describe candidate
behavior). Run the existing ignored signed-sum measurement unchanged at both
revisions, setting `KEEL_AVX2_ADD_EVIDENCE` to separate `/tmp` directories.

Copy exported `.o`/`.c` pairs to LinuxHost and run the maintained
`tests/x86_widen/run_native.py --dir EXPORT`, and again with `--cc gcc`, through
`suite/fast_opt_locked.py`. Do not nest locks. Retained LinuxHost evidence is
`/tmp/keel-address-pilot-8690`: isolated baseline/candidate snapshots,
`base`/`candidate` exports and native reports, `audit.json`, independent GNU
sum disassemblies, `timing` drivers/raw samples/environment/hashes, compiler
stage CSVs and lock/build logs. Generated artifacts remain outside Git.

Candidate native report SHA-256 values are
`8317875b08fca0824976351ae4e0a2c96b8e47d86e18f66c89e2a33f5bb3662e`
(Clang) and
`081bf00e80d72518307c44acd20f79855172b3f31d75b1698c27a9a8f2c8c42c`
(GCC). The frozen base sum object's SHA-256 is
`179109e189a23853c3909dd2868cb3e75419067d85c01c52faec9cda3b0542ff`;
candidate is
`c41979cf4a1413e752861f6431b206cd6f5ef8a8d2de52d90a7586fd95c47150`.

This is finite backend/lowering/encoding/native evidence on an AVX2-capable
host. It does not establish execution on v1/v2-only or AVX-only hardware,
exhaustive ISA certification, new frontend support, arbitrary FP exception
behavior, or general address rewriting.

## Independent review on current main

Review replayed candidate `765e82f6aac1559252a3eabe0b5691ad33757cbd` onto
`4611704b187f44eeafd347d29875545b89e63eac` in its own worktree. The intervening
main commit changes benchmark helpers/documentation, not the backend. Neither
main nor the original candidate checkout was modified. No production defect
was found in this bounded audit; review changes only the dedicated regression
corpus and this document.

The size gate matters because `lower_i128` reads the original address through
`use_reg` for two m64 accesses. In contrast, both admitted i32x4 consumers use
`mem` with the original resolved address key and an exact m128 type. The
selected base replaces the covered ptradd operand before allocation; the
adjacent wide cover transfers that operand list to its root. Reserved integer
address scratch is distinct from the SIMD destination/spill scratch. The
existing indexed selector still accepts only 1/2/4/8-byte accesses. Other
128-bit shapes, every 256-bit load, stores and volatile loads therefore gain
no address cover from this exception. Atomics use separate instruction forms.

Signed disp32 is sign-extended in a 64-bit effective address, matching ptradd's
modular arithmetic even for synthetic wrapping bases. Outside that interval,
the original materialized arithmetic remains. Symbolic bases retain ordinary
symbol materialization rather than a new relocation expression. Resolved use
counts retain shared addresses; same-block, covered-node and source-file/line
checks retain their existing boundaries. The exception additionally rejects
constraints and a located child under an unlocated load. The wide cover keeps
its stronger complete-location, condition, constraint, existing-cover and
resolved-result checks. Moving across its adjacent pure conversion changes no
memory ordering. No physical FLAGS preservation, frontend extension, FP
arithmetic or ABI change is implied.

Review adds offsets 128 and -129 around the disp8 boundary, explicit exclusion
checks for every other 128-bit vector shape and a representative 256-bit shape,
five overlapping input/output placements, and both crossing-page input faults
before any output write to shared memory. Its fifth function mode inserts a
call between ptradd and load; the native helper clobbers all SysV caller-saved
GP and SIMD registers to check the selected base's lifetime across the call.
The corpus now has 65 functions (13 offsets by five modes) in each of the same
48 feature/selection/allocation/pool configurations. Aliased-output references
simulate the shared-address mode's reload after its first store. Fault children
exit from SIGSEGV/SIGBUS handlers to avoid core-dump overhead.

Local debug validation passed 28 integration tests across the dedicated,
scalar-address, ARM-vector, wide-generic, wide-conversion, signed-sum and
direct-add suites, plus all 116 codegen library tests. Release passed the same
x86 suites (22 integration tests) and all 116 library tests, including the
release instruction-feature guard. The final dedicated corpus was regenerated
in both profiles. Text/typed/chunked object parity and per-instruction effective
feature checks remain required. Repository-wide formatting reports existing
unrelated differences; the changed Rust test passes its focused format check.

Review independently matched the retained frozen source hashes for select.rs
and packed_load.rs, report/object hashes, identical optimized IR and the exact
three-pair assembly diff. Recomputing all three medians from the 100-row raw
CSVs reproduces the values above. These remain the original frozen-tree
measurements, not new timing results for the rebased review. No timing rerun or
application performance claim is made.

Fresh LinuxHost native execution passed all 48 final bundles with Clang 23.1.1 and
all 48 with GCC 16.2.0, including overlap, call-clobber and fault-order checks.
All 48 debug/release object pairs are byte-identical. The final native evidence
is `/tmp/keel-address-review-167a5e84/complete` on LinuxHost, with separate reports
`complete-clang.json` and `complete-gcc.json` and `complete-lock.jsonl` in its
parent directory. Report SHA-256 values are respectively
`9b15a28da8618266aad83b2d2407fbb423b9b28f2ffc77fc7117bbee846dfd98` and
`1c24e7ed28673f28e038dcda33c152f0ca7172a37da28b2166f94fd7f9a4cfde`.
Local final exports are `/tmp/keel-address-review-complete-{debug,release}`;
logs and the parity report use the `/tmp/keel-address-review-` prefix.
Every build and native invocation held the shared lock. No timing conclusions
are drawn from these correctness runs. This is finite execution on a Ryzen 9
9950X3D supporting newer ISA levels, not v1/v2-only or AVX-only certification.
