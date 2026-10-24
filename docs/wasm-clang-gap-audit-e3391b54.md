# Keel–Clang wasm32 gap audit at e3391b54

This is a read-only production-source audit of
`e3391b547f74cf59d9f47939f7d05c5a820d9f14` on 2026-10-24. The research worker
owns this document only; the comparison worker owns the corpus, build, decoder
and execution harness. Parent owns independent review, implementation and
integration. No compiler change, Rust build, timing run, remote access, tool
installation or second benchmark campaign belongs to this audit.

Read alongside the [CLI contract](wasm-cli.md), [C subset](wasm-c-freestanding.md),
[basic-pass safety audit](wasm-basic-optimization-audit.md) and
[backend contract](../crates/keel-wasm/README.md). The supported comparison is
integer/pointer C with plain i32/i64 call boundaries, fixed owned memory,
static data/frames, direct calls and recursion. FP, SIMD, imports, WASI/libc,
indirect calls, general C ABI and LTO are excluded. Defined unsigned arithmetic
and bounded accesses avoid comparing different C undefined-behavior assumptions.

## Evidence identity and limits

Source SHA-256, Git blob identities and byte-for-byte comparisons against the
audited commit are recorded in
`/tmp/keel-wasm-gap-audit-e3391b54-20261024-r1/source-identity.json`
(SHA-256 `367ef444cef06c73e08f2b1961e94bcca2b632e73211d13db361ac77253d98c3`).
That record covers all source stages cited below and the four required docs.
The first corpus snapshot has SHA-256
`6323eab334c63c49344cf2a792b9a435787d93002a229a7b7abd7ab7c6bd38cf`.

The final frozen comparison manifest is
`/tmp/keel-wasm-comparison-e3391b54-20261024-r3/manifest.json`, SHA-256
`a43403a5ad24b52ea120c11706def605500efda98923166d2a6953463559e01d`.
This worker checked its revision, corpus, four tool binaries, eight raw/stripped
Wasm binary hashes and all 44 function bodies using an independent Python body
decoder. Local declarations, operator offsets/immediates/counts and body bytes
match. The verification record is
`/tmp/keel-wasm-gap-audit-e3391b54-20261024-r1/static-r3-verification.json`,
SHA-256 `db28f189f593aa441271db83344192723d878bd3f13d817cc0655e87b17c42bc`.
The sibling `verify_static_r3.py` is the independent checker; no Wasm execution
or Rust compilation occurs in it.

The earlier r1 manifest contained failed Clang links and is excluded. Frozen r2
had the same final binaries but displayed signed constant immediates as unsigned
LEB payloads. Independent decoding caught this metadata defect: the i32 constant
for `0x9e3779b9u` appeared as 32719206841 instead of -1640531527, and `i32.const
-4` appeared as 124. The comparison owner corrected its decoder in r3. This was
an evidence-tool defect, not a compiler, validator or runtime miscompile;
binary bytes and all body/operator counts are unchanged.

Separately verified IR identities are `plain.ir` SHA-256
`e724cc959932b7bf7434fa0de965d582d721de5e0e27f04dcf2ee3490eeb90c1` and
`basic.ir` SHA-256
`9943aaf2bd360be79afa4e6041f757141c91fd09e2ea526c8e2af5a10407f36e`,
under the comparison r2 directory. Its `ir-manifest.json` also pins helper
source and lockfile; all listed hashes were checked. Copies and a verified
header/lockfile/harness identity record are in this audit's temporary directory.
The helper only dumps frontend/basic IR; final Wasm legality and execution
evidence comes from the real CLI binaries, not that helper.

Emission, independent validation, runtime correctness,
compile latency and warmed execution are separate results. Body bytes include
local declarations and exclude the enclosing body-length LEB. Operator counts
include structural operators and local accesses. Neither is a runtime or JIT
machine-code performance claim. Clang O0 and O2 must remain separate baselines;
Keel basic is four audited cleanup passes, not an equivalent O2 pipeline.

## Observed gaps, separated from speed claims

All four profiles emitted, linked where applicable, and passed the comparison
worker's Node v24.22.1 validation/instantiation and 640 independent BigInt result
and exact 1040-byte arena checks, including 16 canary bytes. The harness covers
four seeds and five bounded lengths per workload/profile. This worker inspected
the harness and verified its evidence identities, without rerunning it. This
finite defined-input corpus found no result mismatch; it is not an exhaustive
trap or memory proof. The worker separately owns focused regression results.

Module bytes/body-byte sums are plain **3052/2777**, basic **2940/2665**,
Clang O0 **1569/1347**, Clang O2 **1453/1231**. Basic removes 112 body bytes
from plain here, but leaves every declared-local count unchanged. Clang uses
Apple Clang 21, `--target=wasm32-unknown-unknown -mcpu=mvp`, freestanding C11,
no standard library/builtins/LTO/fast-math, and the same checked-in integer
header. Existing LLD 23.1.1 links complete modules, uniformly stripped of custom
sections. O0 and O2 are actual separate command-line requests in the manifest.

Both memories are fixed at one 64-KiB page, but their software stacks and static
placement differ: Keel data begins at 1024 and stack spans 32768..65536; Clang
uses a 32-KiB stack-first layout with global base 32768. Address getters let the
harness compare the corresponding arena, rather than assume equal addresses.
These settings do not establish equal exhaustion or out-of-bounds behavior.
Keel's checked frame contract, Clang's frame choices and address-constant LEB
sizes remain confounders even in body-byte comparisons.

The largest **basic-versus-O2 absolute body-byte gap** is `branch_switch`:
496 versus 108 bytes (+388), 245 versus 59 operators, 29 versus zero declared
locals. It is also largest against O0, 496 versus 229 (+267), narrowly ahead
of `local_array`, 518 versus 259 (+259). This supports investigating control
flow/local traffic; it does not isolate their individual runtime costs.

Other source-stage witnesses are:

- `local_array`: basic 518 bytes/241 operators/44 locals; O0 259/128/7;
  O2 337/175/5. Indexed address generation repeats in the actual IR, alongside
  two loops and a real 64-byte static frame.
- `pointer_mutate`: basic 353/163/32; O0 208/91/3; O2 209/108/5. Basic has five
  wraps, two unsigned extends and three signed extends; both Clang bodies have
  none. These counts include pointer arithmetic as well as the explicit cast
  chain, so they are not all redundant casts.
- `recursive_helper`: basic 246/116/18; O0 146/59/2; O2 98/40/1. Its basic IR
  is exactly an entry branch with two terminal return arms and one eight-byte
  slot rounded to a 16-byte Keel frame. The recursive call itself does not make
  this three-block CFG cyclic.
- `scalar`/`wide`: plain and basic remain 114/50/10 and 131/53/11; O0 is
  63/26/1 and 74/27/1, O2 is 26/12/0 and 37/13/0. They have no dispatcher,
  establishing local traffic plus runtime scaffolding as a separate gap.

O2 is not always smaller: `integer_loop` grows from O0 112 to O2 176 body bytes,
and `checksum` grows 114 to 188. Its assembly contains loop unrolling. Do not
use byte rank as a speed ranking or treat Clang O0 as having no backend work.
The single bounded timing attempt was deferred by the quiet gate because an
existing rustc process was busy. There are no compile/runtime timing samples
to interpret and no retry is requested by this audit.

## Source-stage findings

1. **Scalar expression trees: emission and locals.** `scalar` and `wide` are
   single-return straight-line cases. [Body::new](../crates/keel-wasm/src/lib.rs)
   already bypasses the dispatcher for exactly one layout block ending in
   `Return`; proposing that optimization again would miss the current base.
   It nevertheless declares a separate raw local for every laid-out SSA
   result and non-entry block parameter. `emit` stores every result, and `get`
   reloads resolved operands. There is no use-count analysis, lifetime reuse
   or expression stackification. Consecutive `local.set L; local.get L` and
   constant-to-local traffic are therefore concrete emission opportunities.
   Distinct raw definition destinations are intentional: a retained aliased
   faulting producer must execute without overwriting its replacement's local.

2. **Loops and switch-heavy control flow: CFG emission plus phi transport.**
   `integer_loop`, `branch_switch` and the memory loops have multiple blocks.
   `Body::emit` surrounds every block with `pc == block_id`, an `if`, and a
   dispatcher loop; `edge` assigns the next PC and branches back. Switches
   compare cases sequentially. Layout-order scans and explicit edge copies
   remain even for reducible C loops. `edge` first pushes all resolved incoming
   values, then writes destination parameters in reverse: that implements a
   simultaneous copy, including cycles. Deleting those copies or reusing
   locals requires a real liveness/interference argument. A large body alone
   cannot establish how much runtime is dispatch, copies or engine optimization.

3. **Indexed pointers and address casts: frontend IR plus literal emission.**
   [FnGen::scaled/convert](../crates/keel-c/src/irgen.rs) use i64 pointer offsets.
   A dynamic i32 index is extended to i64, multiplied by element size and
   wrapped to i32 by `Ptradd` emission before i32 addition. The frontend's
   integer-to-pointer path explicitly truncates to i32 and re-extends to i64;
   the backend then wraps again. Pointer-to-32-bit-integer conversion similarly
   extends to i64 and truncates. `pointer_mutate`, `buffer_offset` and
   `object_offset` expose these shapes without a pointer call ABI. The four
   basic passes do not perform dynamic cast cancellation or pointer algebra.
   These are representation costs, not evidence of incorrect normalization.

4. **Local arrays and recursion: static frames and observable checks.**
   [Frame::new](../crates/keel-wasm/src/runtime.rs) retains the slot table and
   rounds packed frames to 16 bytes. The runtime emitter saves incoming SP,
   checks unsigned subtraction underflow, subtracts frame size, checks stack
   bottom, updates SP and restores it on every normal return. Even zero-sized
   frames take this path. `local_array` and the volatile recursive helper have
   actual memory/frame work; their stack checks cannot be counted as redundant
   copies or silently removed to match Clang. The contract requires allocation
   failure before body effects, unchanged exact memory access widths and
   discarding an instance after a trap. Slot deletion changes exhaustion behavior.

5. **Optimization coverage: deliberately bounded cleanup.**
   [optimize_wasm32_basic](../crates/keel-opt/src/wasm32.rs) invokes exactly
   `const-fold`, `simplify-cfg`, `phi-simplify`, `dce`, once in that order, with
   whole-module WASM32 verification after each. `simplify-cfg` itself repeats
   its CFG cleanup internally; the overall four-pass pipeline does not iterate.
   Dynamic algebraic combines, common-subexpression elimination, inlining,
   loop unrolling and strength reduction are absent. The frontend already
   folds literals, removes elementary identities and combines constant pointer
   offsets, so it is inaccurate to describe all input C syntax as emitted
   literally. Clang O2 may also change algorithmic shape, unroll loops or inline
   calls: its byte delta cannot all be charged to the backend dispatcher.

## Ranked small implementation pilots

Ranked by bounded implementation risk and demonstrated activation, rather than
claimed speedup. Each belongs only to `crates/keel-wasm/src/lib.rs` and focused
backend tests, with native/frontend/basic-pipeline behavior unchanged.

1. **Adjacent local round-trip peephole.** Owner: `Body::op`/instruction output,
   with regressions in `tests/direct_return.rs`. Replace only immediately
   adjacent `local.set L; local.get L` with `local.tee L`. Independent decoding
   finds **73 eligible pairs in each Keel profile**, including `pointer_mutate`
   16, `branch_switch` 12, `local_array` 10 and `scalar` 3. Each pair loses one
   encoded operator; this is an activation count, not an implemented reduction.
   Preserve the same raw local index, local declarations and stack value.
   Flush any pending instruction at all other operators/control boundaries.
   There is no lifetime reuse, alias substitution, phi-copy deletion, scheduling
   or frame-check removal. This can cover prologue pairs while preserving the
   checks and effect order. Assert actual tee activation plus identical results,
   trap/store observations and unchanged nonmatching bodies.

2. **Exactly three-block terminal conditional.** Owners: `Body::new` shape
   selection and `Body::emit`, sharing the current instruction/Return emitter;
   tests in `tests/direct_return.rs` and `tests/runtime.rs`. Eligibility is
   exactly three layout blocks: entry ends in Brif to two distinct blocks,
   each successor has no parameters and ends in ordinary Return, with no other
   edges/terminators. Fresh basic `recursive_helper` matches; plain has four
   layout blocks and must retain fallback. Emit an ordinary selected `if/else`
   instead of PC dispatch; preserve the prologue, both arm instruction orders,
   volatile accesses, recursive call and every return's SP restoration. This
   does not evaluate both arms as select. Reject alias/definition arrangements
   that require unsupported cross-arm transport; retain raw destinations and
   resolved reads. All unmatched CFGs keep the existing dispatcher. Require
   decoded proof of PC-test removal on this helper, plus byte-stable fallback,
   branch-only traps, aliased faulting producers and recursive-frame canaries.

3. **Exact single-use low-bit cast chains in the runtime emitter.** Owners:
   `Body::new` use classification and `Body::convert`/instruction emission;
   tests in `tests/memory.rs` and `tests/direct_return.rs`. The actual
   `pointer_mutate` entry has `uext.i64 -> trunc.i32 -> uext.i64 -> inttoptr`,
   starting from the i32 result of `buffer_offset`; its machine conversions
   can collapse to that original i32 bit pattern. Getter IR also contains
   `ptrtoint.i64 -> trunc.i32`. Recognize only these exact width/type chains,
   with all intermediate resolved uses confined to the chain and no phi,
   external alias user or retained producer needing the intermediate value.
   Skip only the covered pure conversion definitions; keep the call/symbol
   producer, raw final destination and every other instruction. Escaping,
   multiuse and nonmatching chains use the old lowering. Do not
   cancel arbitrary integer-to-pointer-to-i64 round trips: upper bits must
   still be discarded. Demand observed elimination on these matched cases,
   extrema/high-bit tests, aliases and complete-memory/trap-order controls.

The dominant switch/loop gap warrants a parent checkpoint before any larger
CFG project. These pilots intentionally leave it only partly addressed.
No general loop structurizer, register-allocation framework, optimizer-tier
expansion, stack promotion or memory-offset absorption is proposed. Measure
each pilot's actual activation and preserved fallback before widening it.

## Correctness assessment and implementation boundaries

No clear production correctness defect was established by this source audit.
The observed conservative costs agree with the documented subset and runtime
contract. Build failures or failed validation must be reported in their own
stage, rather than relabeled as wrong results. A runtime mismatch needs a
minimal defined-input reducer before attributing it to Keel.

Any pilot must keep original whole-module runtime admission before pruning,
per-pass WASM32 verification, final backend admission and independent MVP byte
validation. It must retain unexported/unused unsupported-input rejection,
plain i32/i64 signatures, volatile/call ordering, faulting unused loads and
division, and the explicitly promised dead nonvolatile `notrap` exception.
Do not infer new `notrap` promises.

Pointer gates include high-bit/negative offsets, modulo-2^32 arithmetic,
integer-pointer round trips, exact narrow/unaligned access widths, complete
memory and canaries. In particular, wrapping pointer addition cannot generally
be absorbed into a nonwrapping Wasm memory immediate. Phi gates include duplicate
edges, swaps/cycles, aliased operands, removed layout blocks and unused pinned
producers. Frame gates include zero frames calling framed callees, all normal
return paths, nested/recursive slots, repeated calls, exhaustion before stores
and discarding every trapped instance. Keep unsupported shapes on the unchanged
fallback emitter.

Implementation compilation remains subject to the supplied shared-cargo rules:
`python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl
cargo build/test --offline --locked -j2` with existing pins/features,
default shared cache and checkout-managed targets. This research task ran no
Rust commands and does not duplicate the comparison worker's measurements.
