# Exact low-bit cast emission pilot

Implementation `2948123f5dc1947841e16b6ad00559074b95e342` starts from reviewed
main `a45e1e96723c6343206adf3634d43e5c627b6427`. This is the conversion/getter
pilot from the [gap audit](wasm-clang-gap-audit-e3391b54.md) and
[comparison](wasm-clang-comparison-e3391b54.md). Parent owns independent review,
combination with the adjacent-tee/terminal-if pilots, and integration. This
checkout contains neither sibling transformation and performs no timing work.

## Exact recognition and proof

`LowBitCasts` recognizes only adjacent conversions in one layout block:

- I32 source → `uext.i64` → `trunc.i32` → `uext.i64` → `inttoptr.ptr`.
- Ptr source → `ptrtoint.i64` → `trunc.i32`.

It counts all laid-out instruction operands after alias resolution, including
calls, returns and every edge argument, with saturating counts. Every
intermediate must have exactly one resolved use, feed the next conversion by
its raw value, and neither be an alias nor have any raw alias pointing to it.
Even an unused external alias conservatively prevents a match. Every conversion
must have exactly one result and the exact opcode and destination type above.
Candidates inspect at most four instructions; there is no recursive expression
walk, optimizer pass, CSE, control-flow analysis or lifetime/local reuse.
Bookkeeping storage is proportional to values and matched instructions; the
layout scans are linear apart from existing alias resolution.

For an arbitrary i32 bit pattern x, the first chain computes
`low32(zext32(low32(zext32(x)))) = x`. For the second, the Wasm pointer's i32
address bits p become `low32(zext32(p)) = p`. These are backend representation
identities, with no native pointer identity assumption. In particular an
arbitrary `inttoptr(i64) → ptrtoint.i64` still discards the input's upper 32 bits
and returns an unsigned 64-bit address. Its original wrap/unsigned extend remain.
`ptradd` still wraps the offset to i32, adds modulo 2^32, and executes before a
zero-offset, exact-width memory access.

Only covered intermediate pure conversions and their result stores are omitted.
At the final instruction, emission reads the original source and writes the
original raw final destination. All local declarations and numbers remain.
Deferring this source read across adjacent pure conversions cannot cross a
call, fault, volatile access, block edge or a write to the source's distinct
local. No memory operation, call, symbol/stack-address producer or faulting
producer is skipped or reordered. Shared uses and aliases of the final result
remain supported because that result is still materialized. Intermediate
aliases, multiple uses, cross-block chains and different cast shapes retain
literal lowering. The first corpus check caught an unnecessarily conservative
final-result alias exclusion; restricting that exclusion to intermediates
activates the actual shared pointer in `pointer_mutate`.

Public APIs, original-input admission, the actual runtime WASM32 verifier,
wasmparser WASM1 final validation, native/frontend/optimizer/CLI sources,
prologue/allocation guards, normal-return SP restoration, phi parallel copies,
control wrappers and instruction output buffering are unchanged.

## Maintained validation

`crates/keel-wasm/tests/low_bit_casts.rs` adds six tests. Node is required and
wasmparser validates fixture bytes explicitly with `WasmFeatures::WASM1`.
Independent BigInt expectations cover zero, negative values, signed extrema,
0x80000000, 0xffffffff, 2^32 and full-width i64 high bits. Decoded operator
assertions establish both matching activation and retained nonmatching casts.
Controls include calls, multiuse intermediates, duplicate-target phi transport,
cross-block chains, retained aliased producers, unused aliases targeting an
intermediate, and aliases of the final pointer with shared replacement users.

Memory tests compare every byte of the 64-KiB memory, including surrounding
canaries, for unaligned four-byte volatile accesses, high/negative offsets,
modulo-2^32 wrap and the last valid address. Fresh instances test OOB stores
without partial writes, a preceding call's volatile store, and store/division/
store ordering. Each trapped instance is discarded after inspection. Unsupported
unexported instructions and pointer signatures still reject. A runtime data
pointer initializer plus address getter exercises real WASM32 admission.

All **62 backend integration tests pass in debug and release**, including the
six new tests and all existing scalar, fixed-memory, runtime, admission and
frozen dispatcher-body controls. Existing runtime controls retain frame
exhaustion before body effects, recursion, repeated normal-return restoration,
volatile/fault observations and stack canaries. `cargo check --all-targets` and
`cargo build` for keel-wasm pass without warnings. The release CLI build used
for corpus emission also passes. `git diff --check` passes. No separate Rust
formatter invocation was made under the supplied restricted Rust command rule.

Every Rust invocation used:

```text
python3 suite/fast_opt_locked.py /tmp/wasm-lowbit-a45e-<gate>.jsonl
  cargo <test|check|build> --offline --locked -j2 ...
```

Existing pins/features and default shared cache/managed targets were retained.
The final test gates use `-p keel-wasm` (release adds `--release`), check adds
`--all-targets`, and the corpus CLI build uses `--release -p keel`. No manual
output deletion, tool installation, toolchain change, remote cache or shared
`CARGO_TARGET_DIR` was used. Rust command records and output are outside Git:
`/tmp/wasm-lowbit-a45e-{debug-final,release,check,build,build-cli}.{jsonl,log}`.
The earlier debug log retains one malformed negative-fixture parser failure;
the fixture was fixed before the passing full runs.

## Static corpus observations, separate from execution

The unchanged corpus hash is
`6323eab334c63c49344cf2a792b9a435787d93002a229a7b7abd7ab7c6bd38cf`.
The frozen e3391b54 baseline backend production source is identical to the
starting a45e1e96 backend source. Fresh plain/basic modules were emitted with
the comparison's original memory/export/optimization flags, final backend MVP
validation, and the maintained independent byte decoder. Body bytes include
local declarations and exclude enclosing body-size LEBs.

| Profile/function | Body bytes before → after | Operators before → after | Locals |
| --- | ---: | ---: | ---: |
| Both / object_offset | 65 → 59 | 32 → 28 | 5 → 5 |
| Both / buffer_offset | 81 → 75 | 39 → 35 | 7 → 7 |
| Plain / pointer_mutate | 367 → 351 | 171 → 161 | 32 → 32 |
| Basic / pointer_mutate | 353 → 337 | 163 → 153 | 32 → 32 |
| Plain / all bodies | 2777 → 2749 | 1321 → 1303 | 204 → 204 |
| Basic / all bodies | 2665 → 2637 | 1257 → 1239 | 204 → 204 |

Every other function body is byte-identical to its frozen baseline. Complete
stripped module bytes change 3052 → 3024 (plain), 2940 → 2912 (basic).
No local-count or runtime-speed reduction is claimed.

Separately, Node v24.22.1 independently validated/instantiated both fresh Keel
modules and the two unchanged frozen Clang modules. The existing correctness-only
harness passed all 640 BigInt-result and exact 1040-byte arena checks, including
16 canary bytes: 320 invocations cover fresh Keel modules, 320 the frozen Clang
controls. No Clang rebuild or timing campaign was run.

Artifacts, decoder output, deltas and manifest are in
`/tmp/wasm-lowbit-a45e-corpus/`; the outside-Git emission/analysis helper is
`/tmp/wasm-lowbit-a45e-corpus.py`. Serialized emission and Node command records
are `/tmp/wasm-lowbit-a45e-corpus{,-node}.jsonl`, with corresponding `.log` files.
The emission log retains an initial statistics-script error (treating a local
count as a list); corrected reruns supply the reported results. Finite execution
and structural reductions establish neither exhaustive correctness nor engine
performance. Independent review remains the parent's next step.
