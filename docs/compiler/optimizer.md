# Optimizer contracts

This reference describes current implementation contracts and limits. Use
[testing](../development/testing.md) for validation commands; per-run results belong outside Git.

<a id="aggregate-call-boundaries-two-word-x86-aggregate-wrappers"></a>
## Two-word x86 aggregate wrappers

The x86 backend keeps small, straight-line aggregate wrappers in SSA when the
external System V ABI is identical to a pair of integer words. This removes
argument snapshots and return-buffer copies without inlining an exported
function or replacing a result pointer in target-independent IR.

Admission requires a single block, one direct call, and a complete two-i64
result. Byval layouts must be exactly `[2 x i64]` or `{i64, i64}`. The backend
compares every original argument location with the proposed i128 location,
including register-pair rollback, stack offsets, total stack bytes and stack
alignment. A pair starting at an eight-byte stack offset that would acquire
extra i128 alignment is rejected. The result must use rax and rdx.

The bounded analysis interprets only private stack homes and incoming byval
copies. Every observed word must be initialized. Addresses may only be used
for bounded constant offsets, nonvolatile word loads/stores, and ABI aggregate
transport. External memory accesses, address observation/capture, multiple
calls, indirect calls, explicit operand ABI descriptors, nonlocal control flow,
variadic calls, frame observers and debugger-visible functions fall back to
ordinary lowering. Public symbols, binding, calling conventions and attributes
are retained. Other targets use their existing lowering.

The temporary backend function uses i128 values only after the location proof.
It is not substituted into the public module or used to infer source-level
alias contracts. Unchanged pairs reuse their incoming SSA value; initialized
constant words are packed without redundant zero operations. Constant shifts
by one 64-bit word lower to half moves rather than a variable-shift sequence.

Validation includes negative admission checks, text/native-object parity and
a Clang-built C caller/callee oracle. The oracle checks integer extremes and
512 deterministic word pairs with both allocators, baseline/full selection and
empty register pools. It also checks constant signed and unsigned 128-bit
shifts by 64.

<a id="controlled-signed-sum-width-controlled-signed-i32--i64-reduction-width-comparison"></a>
## Controlled signed i32 → i64 reduction width comparison

This study compares Keel's default 128-bit vector width against an explicit
256-bit (AVX2) preference for a signed i32 → i64 wrapping sum on x86-64.
For long, warm inputs the 256-bit form took 26.7–31.0% less median time at
1,048,576 inputs, but short inputs regressed. Further bounded width-policy work
is justified; **all production defaults remain 128 bits**.

The [direct-add emission experiment](../targets/x86_64/codegen.md) measures a
different question (fixed wide IR across compiler revisions) and is not pooled
with this comparison.

<a id="controlled-signed-sum-width-method"></a>
### Method

The harness lives in `suite/width-comparison/`. `provenance.py` snapshots a
requested Git revision, verifies the archived base blobs against their Git IDs,
overlays only the width-comparison helpers, and copies `compiler.rs` to
`crates/keel/examples/width_comparison.rs`. The runner rechecks the snapshot
manifest before work and stores it in `results.json`.

The example runs the normal compilation path: C preprocessing and frontend,
initial verification, `DEFAULT_PIPELINE` with the default verification policy,
generic tuning, `SelectionOptions::ALL`, Full allocation, effective features and
chunked integrated object assembly. The only treatment is the library capability
`with_wide_preference(false|true)`. No IR is reparsed, and no frontend, ABI, FP
or default policy changes.

Parity controls, run for strict x86-64 v1, v2, v3 and v3-without-AVX2, and for
native arm64 macOS:

- The 128-bit library object **and optimized IR** match ordinary CLI output for
  the same input and flags.
- Both library compilations in one `pair` process match their standalone
  objects and IR.
- Both width requests produce identical objects and IR on v1, v2, AVX-only and
  arm64. Only v3 activates the wide reduction, and of twelve functions only
  `rk_widen32` changes.

<a id="controlled-signed-sum-width-correctness"></a>
### Correctness

The `runtime-kernels/kernels.c` kernels, driver and input generator are
unchanged. Both v3 arms pass an independent wrapping signed-sum oracle at
lengths 0–81, 257, 4096, 65536 and 1048576 before timing; each timed process
also checks its result before the timer and its final checksum. One Clang `-O2`
driver object links both arms without LTO.

Dedicated guards pass for every profile and width on x86-64 and both widths on
arm64: lengths 0–81 (covering group boundaries and tails), starts 0–5, all
sixteen byte alignments, both protected-page edges, signed extrema and negative
elements, and initial sums 0, `UINT64_MAX` and `0xfedcba9876543210`. Each
combination checks 26,568 seeded cases plus kernel cases, inaccessible-pointer
empty and reversed ranges, a negative end, and a valid negative start. Native
execution ran on an AVX2-capable CPU, not strict v1/v2-only hardware.

Supplemental release tests: `x86_widen_sum` (48 feature/selection/allocation
profiles, including byte parity across text/typed/chunked emission and native
guarded ranges), `x86_signed_wide_load`, and `x86_64::feature_guard_tests`.

<a id="controlled-signed-sum-width-generated-code"></a>
### Generated code

| `rk_widen32` | 128-bit | Explicit 256-bit |
| --- | ---: | ---: |
| Function bytes | 256 | 313 |
| Steady-loop inputs | 8 | 16 |
| Steady-loop instructions / bytes | 15 / 61 | 21 / 82 |
| Add-stage vector copies in loop | 0 | 0 |
| Finish instructions / bytes | 8 / 31 | 9 / 44 |

For eight inputs, the narrow loop makes four exact **8-byte** PMOVSXDQ reads at
offsets 0/8/16/24 and advances 32 bytes. For sixteen inputs, the wide loop makes
four exact **16-byte** VPMOVSXDQ reads at offsets 0/16/32/48 and advances 64
bytes. Guards require at least 8/16 elements, round group counts down and keep a
scalar fallback for address overflow. Both tails read exactly one 4-byte element
per iteration. Neither loop spills accumulators or accesses the stack. Six
address-building instructions remain in the wide loop (three
`mov r12,rbx` / `add r12,offset` pairs). The v1/v2/AVX-only outputs contain no
AVX-family mnemonic or YMM/ZMM operand in the reduction.

ABI: both keep the scalar SysV signature `U(const int *, const I *, const U *, U)`
with an RAX return; no vector argument crosses the ABI. Both save/restore
RBP/RBX/R12. Narrow reserves 32 bytes; wide additionally saves R15, reserves 71
bytes, aligns RSP to 32, restores RSP from RBP and executes `vzeroupper` before
returning. Narrow finishes with a 16-byte stack store and two 8-byte lane reads;
wide finishes in registers. No FP arithmetic or FP-control mutation is
introduced.

<a id="controlled-signed-sum-width-conclusions-and-limits"></a>
### Conclusions and limits

Wider reductions look worthwhile for long eligible signed sums, but a production
policy would need runtime-size and cost guards and broader workload and host
validation. Short-input regressions and larger code and stack size remain. This
study makes no cold-cache, concurrent, whole-application, other-x86
microarchitecture, unsigned, multi-stream, FP or ARM profitability claim; arm64
serves only as a correctness and parity control.

<a id="controlled-signed-sum-width-reproduction"></a>
### Reproduction

Snapshot a revision, then build and run inside the snapshot, keeping generated
artifacts outside the repository:

Repeat with a fresh output directory and a second seed. If the preflight check
fails, the run records `complete:false` and skips timing. For arm64 macOS
controls, build on that host and add `--target aarch64-apple-darwin
--controls-only`, omitting `--cpu`; cross-target native linking is unsupported.
Supplemental checks:

```sh
cargo test --release -p keel-codegen --test x86_widen_sum --test x86_signed_wide_load
cargo test --release -p keel-codegen --lib x86_64::feature_guard_tests
```

<a id="if-convert-bounded-scalar-if-conversion"></a>
## Bounded scalar if-conversion

`if-convert` is an opt-in function pass. No default pipelines, allocator modes
or existing compatibility options change. It admits a branch whose two distinct,
single-predecessor arms end in jumps to the same join. Each arm may be empty.
Both arms together may have at most eight speculated instructions and the join
at most eight parameters. A conditional with two argument-carrying edges directly
to the same join is also supported. Entry/self-target arms and a join equal to
the decision block are rejected.

The pass substitutes incoming arm parameters simultaneously, moves the admitted
instructions before the decision, and selects each outgoing join argument using
the original nonzero condition. Identical arguments need no select. Signedness
and width remain in the original integer operations/comparisons; arithmetic still
wraps. Moved instructions retain source locations. Canonical integer/pointer
parameters and canonical instruction result definitions are required. Arm
values used outside their own arms are excluded, including unreachable users.

The explicit speculation allowlist contains integer constants, compares, selects,
wrapping add/subtract/multiply, integer logic, masked shifts/rotates, high multiply,
integer bit-count/negate/swap operations and integer extensions/truncations.
Loads, stores, atomics, calls (including returns-twice and indirect calls), division
and remainder, floating operations, frame operations, inline assembly, exception
operations and traps are excluded. New IR opcodes do not enter implicitly.

<a id="if-convert-explicit-schedule"></a>
### Explicit schedule

For experiments with the explicit pass:

```text
inline,small-memory,simplify-cfg,stack-promote,phi-simplify,dce,
if-convert,simplify-cfg,canonicalize,instcombine,phi-simplify,dce
```

Inlining and intrinsic initialization expansion expose private fields; phi
cleanup exposes small scalar diamonds; cleanup after conversion removes
redundant parameters and jumps. Keep the pass out of Fast until compile cost and
register pressure have been evaluated.

Full does not schedule general `if-convert`. Its reduction passes use bounded
private trials and publish normalization only after vectorization succeeds.
`loop-reduce-multi` can normalize independent conditional wrapping sums and fold
an unsigned unit counter observed only after the loop. See
[simd-ir.md](simd-ir.md#conditional-multi-reduction-normalization).

The real `k_stackobj` now uses [post-capture promotion](optimizer.md#stack-promote-post-capture-promotion)
and conditional multi-reduction normalization. It keeps the ordinary initializer
and three entry field reads, then vectorizes the two sums. Its accumulators and
result use defined unsigned wrapping arithmetic in the C benchmark source.

<a id="instruction-costs-instruction-costs-and-optimization-profitability"></a>
## Instruction costs and optimization profitability

The assembler supplies an inventory of every encoding Keel supports and exact
instruction sizes. It does not supply CPU execution timings. Those depend on the
processor, operand form, dependencies, register allocation and memory behavior.
This infrastructure reports these quantities separately; optimizer defaults do
not change as part of adding it.

<a id="instruction-costs-data-sources-and-coverage"></a>
### Data sources and coverage

`tools/instruction_metrics.py catalog` derives all 534 current encoding forms
from the checksum-pinned Arm specification in `crates/keel-asm/isa/a64.json`.
Every entry has its encoding mask and exact four-byte length. Timing fields are
null unless an explicit CPU model supplies them. Mask matches are candidates,
not a semantic disassembler or permission to assign one cost to all operands.

A modeled catalog constructs one representative word per form, disassembles it
with LLVM, checks its exact round-trip through Keel, then obtains latency,
reciprocal throughput, micro-ops and resource information from
[llvm-mca](https://llvm.org/docs/CommandGuide/llvm-mca.html). The report includes
CPU, tool versions and hashes, specification hashes, samples and explicit reasons
for missing data. A sample is not coverage of all operand variants. In
particular, zero-register idioms, writeback addressing, conversions and vector
arrangements can change costs. Calls, barriers and atomics cannot be assigned
ordinary ALU costs when the scheduling model lacks their behavior.

LLVM 23.1.2 recognizes `apple-m5` but selects **CycloneModel** in its
[processor definitions](https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/lib/Target/AArch64/AArch64Processors.td).
These are provisional scheduling estimates, not measured M5 latencies.
The checked-in catalog has usable model metrics for 358 representative forms
and explicit gaps for 176, mostly LSE atomics, barriers and call/return
behavior. Hardware calibration needs separate dependency-chain and independent-stream experiments,
including operand variants, timer overhead, CPU identity and repeated samples.
Memory measurements need explicit cache, alignment and alias conditions.

<a id="instruction-costs-generating-and-comparing-metrics"></a>
### Generating and comparing metrics

Build the driver with `cargo build -p keel --release`. The LLVM utilities are
optional offline analysis dependencies; code generation and assembly use Keel.
On a keg-only Homebrew installation, pass their absolute paths:

```sh
python3 tools/instruction_metrics.py -o encodings.json catalog
python3 tools/instruction_metrics.py -o apple-m5.json catalog \
  --cpu apple-m5 \
  --llvm-mca /opt/homebrew/opt/llvm/bin/llvm-mca \
  --llvm-mc /opt/homebrew/opt/llvm/bin/llvm-mc

<a id="instruction-costs-exact-metrics-on-an-assembly-file-including-final-branchaddress-fixups"></a>
## Exact metrics on an assembly file, including final branch/address fixups:
target/release/keel asm-metrics input.s -o exact.json
python3 tools/instruction_metrics.py -o before.json analyze before.s
python3 tools/instruction_metrics.py -o after.json analyze after.s
python3 tools/instruction_metrics.py compare before.json after.json \
  --objective size

<a id="instruction-costs-for-a-small-straight-line-region-add-cpu-estimates-to-each-report"></a>
## For a small straight-line region, add CPU estimates to each report:
python3 tools/instruction_metrics.py -o before.json analyze before.s \
  --cpu apple-m5 --llvm-mca /opt/homebrew/opt/llvm/bin/llvm-mca
python3 tools/instruction_metrics.py -o after.json analyze after.s \
  --cpu apple-m5 --llvm-mca /opt/homebrew/opt/llvm/bin/llvm-mca
python3 tools/instruction_metrics.py compare before.json after.json \
  --objective speed
```

Assembler metrics record source instruction offsets and final words; alignment
padding, text data and other sections do not become source instructions. Exact
text size still includes text padding/data. CPU analysis rejects regions with
extra text bytes, and requires one MCA region with the same instruction count.
It uses `-noalias=false`, refuses CPU identity mismatches, preserves diagnostics,
and declines a speed score for warnings, unmodeled side effects or missing
throughput. The raw resource table is retained for diagnosis, including MCA's aggregate
row (instruction index equal to the instruction count); do not count that
row as another instruction. Timing is never
silently substituted with a guessed default. Out-of-order model estimates are
not proof of semantic equivalence or of hardware speedup.

<a id="instruction-costs-the-cost-function"></a>
### The cost function

Keep costs as separate quantities before choosing an objective:

- **Size:** final emitted text bytes, including encoding expansion and padding.
- **Speed within a region:** modeled cycles per iteration of that region,
  accounting for the dependency graph and execution resources.
- **Register pressure:** a constraint before allocation; actual spill/reload and
  move instructions are included when measuring code after allocation.
- **Compile time:** measured separately as a budget for the optimization itself.

The comparison tool implements two explicit objectives: minimize bytes for
`size`, or minimize estimated cycles per iteration for `speed`, requiring the
same CPU, tool binary and simulation iteration count. It never adds instruction
latencies to claim an execution time and does not collapse these dimensions
with arbitrary weights. Simulated iteration costs include warm-up effects;
use sufficiently many iterations and consistent regions when comparing them.

For whole-function profitability, the intended speed objective is
`sum(block_execution_count[b] * estimated_cycles[b])`, using measured execution
counts where available. This is an approximation: dependencies across blocks,
branch prediction, cache misses and calls require additional treatment. Loop
depth is not an execution count, and summing instruction reciprocal throughputs
is not a resource-contention model. Spills already present in the measured
sequence must not be charged twice.

IR transformations need a target-cost interface describing their prospective
lowering. One IR multiply may become one instruction, multiple instructions, or
a helper call; a constant may fold into an immediate or need several moves.
Instruction selection can fuse operations or encode addresses, and allocation
can erase apparent savings through spills. A single target-independent cost per
IR opcode cannot express these cases.

Use the reports to diagnose measured hot regions and compare actual lowerings.
A profitability decision that uses calibrated estimates (for example,
rematerialization versus keeping a value live) should come with correctness
tests and paired application benchmarks. The provisional LLVM timing tables are
diagnostic only; the Apple M5 tuning below is the one live decision based on
hardware calibration.

<a id="instruction-costs-native-apple-m5-tuning"></a>
### Native Apple M5 tuning

`--tune generic` is the default. `--tune apple-m5` selects measured profitability
choices for Apple arm64 without changing the target, ABI, enabled ISA features,
IR optimization tier or instruction-selection tier. For example:

```sh
keel cc -O --isel-tier full --tune apple-m5 input.i -o output.s
```

The first choice is deliberately small: i32/i64 multiplication by 3, 5 or 9
can use `add r, r, r, lsl #k`. It requires tree selection (`--isel trees`,
`--isel all`, or the corresponding tier). The selector preserves existing
multiply-add covers, shared constants and wrapping integer semantics. Narrow
integers, i128 and other constants retain their previous lowering. An M5 tuning
request for the x86 target is rejected by both the driver and backend APIs.

The calibration measures 32- and 64-bit dependency chains and eight
independent streams on an Apple M5 Max. Keel assembles the generated kernels and
a small C harness (built with the system toolchain) times and checks them; no
LLVM timing data is used. The shift-add form showed about one-third lower
elapsed time in dependency chains and neutral to slightly faster independent
streams. Results are nanoseconds per operation, not inferred cycles; cores and
clock frequencies are not pinned.

`tools/generate_m5_tuning.py` emits the compact Rust cost table from this report.
The rule requires the chain ratio's upper 95% bootstrap endpoint below 1.0 and
a parallel-stream median ratio no greater than 1.0. It applies only to the six
measured width/constant combinations and saves constant materialization where
that constant has no other uses. No general instruction-latency model is
inferred from these measurements. The earlier provisional LLVM catalog remains
an offline diagnostic source; this tuning rule does not depend on it.

Reproduce the calibration and check the generated costs:

Compare application behavior with the `keel-m5` and `keel-m5-fast` suite
presets, which use identical IR passes and full instruction selection and differ
only in M5 tuning. In the application comparison, runtime was neutral
(geometric-mean ratio 1.0013), compile time was neutral (0.9981) and combined
text shrank by 160 bytes. Individual results ranged from a 2.3% regression
(QuickJS Fibonacci, not yet profiled) to a 1.9% improvement (QuickJS string).
There is no overall application speedup, so the rule stays experimental and
opt-in; generic defaults are unchanged.

<a id="load-reuse-bounded-cross-block-load-reuse"></a>
## Bounded cross-block load reuse

`load-reuse` runs in Full's late memory/loop epilogue and requires explicit
scheduling in other tiers. Run it after GVN when independently computed
addresses need to become identical SSA values (the current Fast isort needs
this preparation; Full already performs both passes).

The pass never introduces or moves loads. An ordinary, successful load makes
its value available for an exact type and canonical address. Availability flows
only from a completed unique predecessor in reachable reverse postorder. Entry,
merges, unprocessed predecessors and backedges cannot inherit facts. Separate
exit snapshots isolate siblings. A loop header resets availability, while a
header's successful load may still be reused by its uniquely entered body.
Block arguments are not translated and no merge intersection is attempted.

Chains stop after eight edges; snapshots hold at most 128 facts. At the fact
limit the entire map is cleared deterministically. There is one CFG traversal,
no fixed point, and no path enumeration. Snapshot storage and alias checks per
write are bounded by the fact limit. Address analysis is shared with
`memory-forward`: exact resolved SSA or checked constant-i64 ptradd paths,
with circular byte footprints and existing eligible restrict contracts.
Unknown or overlapping writes kill facts; pointer-width writes conservatively
kill all facts. Stores do not establish new load facts. Volatile accesses,
atomics, fences, calls, inline asm, debugtrap and lifetime barriers clear facts
through the IR memory-effects model. Functions with source debug variables are
excluded entirely because their stack homes can be edited between source
locations. Alignment and notrap annotations never establish alias freedom.

The original first successful load remains pinned, including if otherwise
unused. Intervening faulting accesses retain their order. A later duplicate
can disappear only after that original access has succeeded and without a
possible memory mutation or lifetime barrier on the inherited path. No new
notrap, noalias or integer-overflow flags are introduced.

<a id="loop-memory-guarded-loop-memory-motion"></a>
## Guarded loop memory motion

`loop-memory` is an opt-in function pass that hoists an invariant ordinary load
out of a small, unrotated two-block loop through a cloned copy of the loop's
initial integer guard. No default pipeline runs it; schedule it explicitly with
`-p`.

<a id="loop-memory-motivation"></a>
### Motivation

C `while` and `for` loops lower to a condition header followed by the body, and
cleanup does not rotate them. A typical reload-style loop looks like:

```text
block0: jump block1(iv0, acc0)          ; entry / preheader
block1: cmp iv, bound; brif -> block2 / block4
block2: v = load.i64 p ; store q ... ; jump block1(...)
block4: return ...
```

A zero bound reaches `block4` without executing the load, so neither entry nor
the ordinary preheader is a legal destination for a potentially faulting load,
even with valid `restrict` contracts. `loop-memory` instead evaluates the first
header test before the loop, keeps the original zero-trip exit with exactly its
original arguments, and routes only the entered case through a dedicated load
preheader. The loop itself remains unrotated, and the original header test still
runs on every iteration.

<a id="loop-memory-accepted-shape"></a>
### Accepted shape

- The header has exactly two predecessors: one unconditional external jump and
  one body back edge.
- The header compares a unit-step i32/i64 induction variable against an
  invariant bound, using at most 16 nontrapping integer instructions. The only
  cloned operations are integer constants, nontrapping integer binary
  operations and integer comparisons.
- The body is a single straight-line block of at most 128 instructions with
  exactly one ordinary load, whose address already dominates entry, before any
  store or other pinned or trapping effect.
- Later ordinary stores must be proved disjoint by the shared function-local
  `AliasInfo` helper (see [memory-forward.md](optimizer.md)). Calls,
  volatile, atomic, lifetime and barrier effects and other traps reject the loop.
  Non-trapping annotations do not relax memory or lifetime dependencies, and
  `restrict` proves neither dereferenceability nor nonnull.
- The exit has exactly one incoming edge (from the header), ends in a return,
  and contains at most 32 instructions.

Header parameters and cloned header results must be their own resolved
identities. The `Function` API permits aliases on definitions still in layout;
such states are rejected conservatively so that a dead definition's clone cannot
overwrite the mapping of an invariant or another parameter. Clone-map keys use
raw definition identities, operands still resolve through aliases, and body
instructions are moved rather than cloned.

<a id="loop-memory-exit-repair"></a>
### Exit repair

Existing exit parameters and arguments are retained. Header values used
directly by exit instructions get additional typed exit parameters, in
deterministic operand traversal order. The original header false edge supplies
the original values; the cloned initial false edge supplies initial arguments or
cloned results. Only exit instruction operands are rewritten; there is no global
use replacement. Header-defined uses elsewhere reject the loop. Exit instruction
order and effects are unchanged.

Moving the load preserves its result identity and source location; cloned
instructions keep their original locations and scopes. Functions with
stack-variable debug descriptions are skipped.

<a id="loop-memory-limits"></a>
### Limits

Whole-function analysis is limited to 512 arena blocks and 8192 laid-out
instructions, with at most eight loop transformations. CFG, dominance, loop
depth and alias information are recomputed before each candidate. Multiple
loads, body block parameters, nested loops, conditional bodies, complex exits
and store sinking are out of scope.

<a id="loop-unroll-wrapping-integer-reduction-unrolling"></a>
## Wrapping integer reduction unrolling

`loop-unroll` is an explicitly named function pass. It is registered in the pass
catalog and runs in Full's late memory/loop epilogue, before
`loop-reduce-multi` and `loop-rotate`. Other tiers require explicit scheduling.
It has no dependency on `AliasInfo` and makes no IR, ABI,
calling-convention, memory-contract, or verifier changes.

The motivating shape is a `hot` benchmark kernel: unsigned
`sum += (p[i] ^ k) * (i + 1)`, which LLVM/Clang compiles with two independent
scalar accumulators. No speedup estimate is claimed for this pass alone.

<a id="loop-unroll-accepted-shape"></a>
### Accepted shape

A natural loop has a two-instruction header (`icmp.ult iv,bound`, or the equivalent
`icmp.ugt bound,iv`, then a true-body/false-exit branch), one straight-line body,
and one unconditional external preheader. Header arguments are exactly the IV
followed by one sum; each can independently be i32 or i64. Entry IV is constant
zero, bound and seed dominate the preheader, and the backedge increments IV by
one. The sum update is a Keel wrapping integer `iadd(sum, independent_term)` in
either operand order. Neither the sum nor its updated result may feed other body
instructions. The independent term can depend on IV and ordinary loads. Header parameters must
resolve to themselves; an aliased comparison result is rejected. Body operations
must each have exactly one result that resolves to itself: retained dead alias
definitions must never overwrite clone mappings for invariant/earlier values.

Body operations are restricted to integer literals, wrapping add/subtract/multiply,
bitwise operations, modulo-width shifts, pointer addition, integer width
conversions, and one or two ordinary i32/i64 loads. Result types are limited to
scalar i8/i16/i32/i64 and pointers. Memory flags are copied exactly. There are no
new alias, null, alignment, lifetime, or `notrap` inferences. Both bodies retain
all operations, including unused loads, in original order.

The pass rejects stores, calls, volatile accesses, atomics, lifetimes, trapping
integer division/remainder, floating point, conditional bodies, additional exits,
body parameters, aliased body results, body-defined external liveouts, dependent recurrences, nested
loops, and functions carrying debug return/location/variable/scope information.
It leaves all such candidates unchanged. Arbitrary invariant starts, signed IVs,
other predicates, additional carried values, and a general rotation framework
are outside this initial scope.

<a id="loop-unroll-transformation-and-safety-argument"></a>
### Transformation and safety argument

The new paired header carries `(iv, lane0, lane1)`, initialized to `(0, seed, 0)`.
It enters the twice-cloned body only when unsigned `bound - iv >= 2`. Entry
establishes `iv <= bound`. A paired step advances by exactly two and the guard
proves both unit increments remain at or below bound without wrapping, preserving
that invariant. In particular the guard never uses wrapping `iv + 1 < bound`.
The second iteration's loads execute only after proof that both original
iterations exist. No body load is moved into either header.

Each cloned body maps the original sum to its own lane; the second clone maps
IV to the first clone's updated IV. At paired exit, the two lanes combine once
with wrapping integer addition. Execution enters the original header with that
sum and the current IV. The original scalar loop handles zero or one remaining
iteration and all original exit arguments/direct header-parameter liveouts.
Zero-trip paths do not load. Modular associativity of integer addition justifies
the regrouping; no floating-point reassociation occurs. Original address
computations, including their modular behavior, are cloned without alteration.

The old header cannot be selected again because its new incoming IV is a dynamic
paired-header parameter, rather than literal zero. The paired header has three
parameters and cannot match. Idempotence is checked both immediately and after
default cleanup. Full runs its original vectorization/reduction rounds before
unrolling and the extended `loop-reduce-multi` pass afterward. These passes
retain their own eligibility checks and scalar fallbacks.

<a id="memory-builtins-explicit-c-memory-builtins"></a>
## Explicit C memory builtins

`__builtin_memcpy`, `__builtin_memmove`, and `__builtin_memset` now lower to
Keel's existing `memcpy`, `memmove`, and `memset` intrinsics. They need no
library declaration and are advertised by `__has_builtin`. Ordinary `memcpy`,
`memmove`, and `memset` identifiers keep ordinary function call semantics.
This does not add a freestanding runtime or a new driver option.

With an ordinary library declaration in scope, the explicit builtin name
also supports function-pointer initialization, address taking and
parenthesized calls such as `(__builtin_memcpy)(d,s,n)`. These designator
forms use ordinary library calls; directly spelled builtin calls retain
intrinsic lowering.

The frontend converts lengths to `size_t` (unsigned long on both supported
targets), and memset's fill to `int`, then truncates the fill to the IR's
`i8` byte. All three arguments are evaluated once, left to right (an order
C permits), including at zero length. The expression returns the saved
`void *` destination; the IR memory intrinsic itself returns no value.
Constant lengths remain visible constants, including zero, small and large
sizes. There is no new size-specific expansion: both existing backends
currently emit libc calls for these intrinsics. This is an IR visibility and
correctness change, without a performance claim.

Memcpy requires disjoint regions; memmove preserves overlap support. Zero
length does not add a runtime pointer check or make otherwise invalid C
pointers valid. Argument typing accepts integer constant expressions equal
to zero as null pointer constants, independently
of whether execution would satisfy the C library's pointer preconditions.
Non-pointer arguments (except null constants), function pointers, const
destinations and retained volatile pointee qualification are diagnosed.
Explicit pointer casts use the target type's qualifiers; `(void *)p` can
explicitly discard const or volatile qualification. This does not change
the qualifiers on actual argument loads, or permit writing objects defined
const. Volatile loads used to compute pointer values or lengths remain
explicit volatile loads.

Fortified `__builtin___memcpy_chk`, `__builtin___memmove_chk`, and
`__builtin___memset_chk` no longer discard their fourth argument. Their
canonical C prototypes are known independently of source declarations. Keel
emits `__memcpy_chk`, `__memmove_chk`, or `__memset_chk` external calls with
all four converted arguments, retaining bound evaluation and checking. Any
conflicting declaration in the translation unit is diagnosed. Even a constant
bound smaller than the length stays on the checked call; Keel does not
silently turn it into an unchecked intrinsic. Checked variants remain
unadvertised by `__has_builtin`.

Each canonical runtime declaration and definition is checked before merging,
so a later incompatible redeclaration cannot hide behind an earlier prototype.
`returns_twice` on a checked runtime used by a builtin is diagnosed as an
incompatible attribute, irrespective of source declaration or call order.

Both backends can emit these C ABI calls. Linking requires a runtime providing
the checked symbols; a freestanding or alternative libc must supply them.
The native differential test links and executes all three on macOS arm64.
Linux target lowering is covered by codegen tests; no Linux native execution
is claimed. The existing extension fixture is unchanged and exercises Clang's
fortified-header expansion without checked-function declarations.

The installed macOS SDK `secure/_string.h` gates its memory fortify macros on
`__has_builtin`; Keel's native preprocessor consequently leaves those names
as ordinary library calls. Native fortified-header compilation is tested.
This is not a claim of complete SDK or glibc fortify support. The legacy
fallback for unrelated fortified builtins remains unchanged and can still
discard bounds; object-size queries still report unknown sizes.

The audit found existing lowering for `bswap16/32/64`, `clz`, `ctz`, and
`popcount` (including `l` and `ll`), typed and generic overflow, alignment
hints, atomic operations, floating classification, varargs, stack allocation,
frame address, and traps. This change does not duplicate them.

`tests/memory_builtins.rs` covers intrinsic shape, constant lengths,
destination identity, argument diagnostics, ordinary calls, volatile argument
loads, checked bounds and feature queries for both target backends. Its native
differential fixture compares Keel with Clang at O0 and O2 using explicit
builtins under `-fno-builtin`, side-effecting arguments, lengths 0/1/7/32/257/4096,
both overlap directions, destination return values and fill conversions.
A separate child-process regression requires a finite too-small memset bound
to fail while the allocated buffer can hold the entire requested write; an
unchecked call would return successfully.

Semantics references: [C11 draft N1570](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf),
sections 6.5.2.2, 7.24.1, 7.24.2.1, 7.24.2.2 and 7.24.6.1;
[GCC library builtins](https://gcc.gnu.org/onlinedocs/gcc/Library-Builtins.html);
[GCC object size checking](https://gcc.gnu.org/onlinedocs/gcc/Object-Size-Checking.html).

<a id="memory-forward-local-memory-forwarding"></a>
## Local memory forwarding

The named `memory-forward` pass removes a same-type ordinary load when an
ordinary load or store in the same block already established its value.
The first potentially faulting access remains. Constant i64 `ptradd` chains
share a canonical root and checked signed byte offset; unknown derivations
remain opaque SSA addresses. Aliases are resolved before analysis. A retained
load with an aliased result is neither a forwarding fact nor a rewrite target.
Functions with debugger-visible variable homes are excluded so debugger writes
remain visible to subsequent reads.

Stores invalidate overlapping byte footprints even when their types differ.
Adjacent ranges on one unrestricted root can retain facts. Footprint comparison
uses circular modulo-2^64 distances, matching both supported targets: signed
interval ordering alone is unsound when pointer arithmetic wraps. Accumulating
constant offsets uses checked i64 addition; overflow abandons the derivation.
Pointer-valued accesses forward at an equivalent address but conservatively
invalidate facts on stores because their width is target-dependent.

Distinct roots disambiguate a modifying access only when **both** have valid
entry `restrict_params` contracts. Recognized constant derivations are allowed;
unknown derivations, comparisons, block argument passing, escapes, volatile
and atomic uses disqualify the affected root throughout the function. Read-only
restricted pointers may alias and are never equated. No null, pointer inequality,
alignment or `notrap` annotation supplies alias evidence. Calls, observable
accesses, unknown writes and lifetime operations remain memory barriers.

`AliasInfo` is private, function-local optimization analysis. Its `may_alias`
consumer must supply nonzero byte widths and at least one modifying access,
and handle barriers itself. Recompute after function changes. Cloned/inlined
callee parameters do not acquire entry contracts; the analysis does not
invent or persist provenance and changes no IR, ABI or calling convention.

The pass is opt-in; no optimization tier runs it. A suitable placement is
after inlining, address simplification/GVN and stack forwarding, before final
simplification and dead code cleanup. No performance claim is made for it.

Tests: `cargo test -p keel-opt --test memory_forward`.

The opt-in `call-load-reuse` pass also accepts explicit scoped call contracts.
Contract-based reuse requires no accessible writes, no freeing of existing
objects, no synchronization, contextual no-callback and explicit preservation
of existing object accessibility. The latter excludes permission/mapping changes
and is not inferred from LLVM nofree. Unknown and returns-twice
calls remain barriers. The first read and the call remain pinned. Fixed-stack
lifetime events invalidate all available reads, including across restarts at the
same address. See [LLVM contract design and implementation](../frontends/llvm.md).

<a id="ordered-reductions-ordered-integer-reductions"></a>
## Ordered integer reductions

`adjacent-diff` and `run-balance-predicate` are explicit experiments, available
through `keel opt -p` or an explicit `compile`/`cc` pipeline. They are not in
Fast or Full defaults. They do not change register allocation or weaken the
generic vectorizer's restrictions on multiple potentially faulting streams.

On ARM, neither pass
activated on any of the 44 Lua, QuickJS, and SQLite translation units. Both
activated on their separate kernel controls. Adding the passes produced
identical application IR, objects, and executables compared with the same
scalar-cleanup pipeline. Kernel speedups therefore do not establish a benefit
for these applications, and neither pass is promoted into Full.

This applicability result is not a requirement that every optimization benefit
these three applications. Improvements on other legitimate workloads are still
useful. Profile inclusion should consider correctness, compilation cost and
regression checks; lack of a hit in one corpus alone is not a removal criterion.

Recheck applicability with a built compiler and prepared application sources:

Use a fresh output directory. The validator discovers the compiler's actual
default schedule from its report and compares each pass, their combination,
and scalar cleanup alone. It checks kernel activation separately from real
application activation; it performs no native timing.

`adjacent-diff` recognizes a bounded rotated loop summing absolute differences
between consecutive signed i32 elements. It discovers the pointer and bound
through dataflow, including ordinary two-argument functions. Both the merged
body/latch shape emitted by C lowering and a separate latch are supported.
Other instruction sequences, extra memory effects and volatile accesses are
rejected. The first iteration reads a[1] before a[0]. Later iterations reuse
successful ordinary reads and load new samples in ascending order, exactly two
i32 elements at a time. A runtime guard on n*4 and base+n*4 retains the original
scalar loop if address arithmetic could wrap. Empty and single-element inputs
perform no reads; the scalar tail does not overread. Differences are widened
before subtraction, so INT32_MIN/MAX can produce magnitude 4294967295. Vector
and final sums use wrapping i64 arithmetic. Practical native tests cannot cover
a complete u64 sum overflow from a zero seed; reassociation additionally relies
on modular arithmetic and the existing vector-add semantics.

The window is expressed as an i64x2 shuffle [1,2] of previous/current values.
The backends retain this narrow shuffle natively as NEON EXT #8 or SSE2 SHUFPD
1. Equal-sized floating vector shuffles also use this bit-preserving path;
other shapes and lane lists retain general legalization. Exact-low64 load
covers read eight bytes directly into NEON/SSE2 registers only when the sole-use
splat/bitcast/widen chain cannot observe the upper eight bytes. Constraints,
distinct debug lines and shared values prevent those covers.

`run-balance-predicate` recognizes a scalar recurrence that credits a positive
i32 input through unsigned extension, otherwise debits 17 down to zero using
an unsigned comparison. Balance and total both remain loop-carried. Only pure
wrapping arithmetic moves, and the single input load stays in place. New
selects inherit their corresponding branch locations. General parity/multiply
recurrences such as Collatz remain excluded.

Full's separate `adjacent-abs` pass converts a private scalar absolute-value
triangle without moving memory. Its sign comparison, negation and select can
use a bounded ARM CMP/CSNEG cover for i32/i64. This preserves wrapping MIN,
custom allocation constraints, source boundaries and live inputs. Functions
containing inline assembly keep their original NZCV behavior.

<a id="sign-extend-idiom-unsigned-byte-sign-extension-recognition"></a>
## Unsigned byte sign-extension recognition

`sign-extend-idiom` is a named, opt-in function pass. It is absent from all
optimization tiers and default pipelines; run it on optimized C IR explicitly,
for example `keel opt -p sign-extend-idiom,dce optimized.keel -o out.keel`.

It recognizes the shape the C frontend and CFG simplifier produce for a manual
signed-byte conversion (fixture:
`crates/keel-opt/tests/fixtures/sign_extend_idiom/bridge_sext8.ir`, captured
from optimized C and identical on both targets): `state & 255`, unsigned
`< 128`, a direct merge edge for the positive result, and a single
`byte - 256` block for the negative result.

The matcher requires exactly four header definitions plus the conditional
branch, a three-instruction negative arm, one i64 merge parameter, exactly one
incoming arm edge and exactly two merge edges. It counts edges from the entire
layout, including unreachable blocks. The masked source must resolve to a block
parameter. The mask, comparison, constants and subtraction must have only their
expected uses. Only i64 arithmetic and byte width are supported. `ult 128` and
`uge 128` with the corresponding opposite branch polarity are supported;
signed comparisons, other bounds, widths and arithmetic are rejected.

For b = state & 255, b < 128 returns b. Otherwise b - 256 modulo 2^64 returns
the two's-complement representation of b's signed byte value. Consequently
`sext.i64(trunc.i8(state))` equals both paths for every input. Truncation and
sign extension cannot trap. The pass inserts these existing operations before
the branch, changes the branch to a jump with that value, and removes the
exclusive negative arm using existing Function APIs. It leaves the merge
parameter and surviving instruction identities intact. Dead header operations
can be removed by a separately requested existing DCE pass.

No loads, calls, effects or trapping operations can occur in either matched
region. Nothing is speculated or moved across the loop guard. Negative-arm
source locations cause rejection, preserving stepping points; the original
branch location is retained and copied to both conversions. Surviving header
locations and function debug metadata are unchanged. Operands resolve aliases;
aliased dead definitions cannot masquerade as live result owners, and no cloned
alias keys are published.

Regression tests (`cargo test -p keel-opt --test sign_extend_idiom`) cover the
captured C shape, all 256 bytes, high-bit edge seeds, zero and multiple
iterations, polarity and mask order, noncanonical widths/bounds/predicates,
extra predecessors and shared results. No performance claim is made for the
pass.

<a id="simplify-cfg-worklist-simplify-cfg-block-merging"></a>
## Simplify-CFG block merging

`simplify-cfg` merges a block into its predecessor when the predecessor ends in
an unconditional jump and that jump is the block's only incoming edge. This note
describes how `merge_blocks` (`crates/keel-opt/src/simplify_cfg.rs`) iterates
candidates without restarting a layout search after every merge.

<a id="simplify-cfg-worklist-algorithm"></a>
### Algorithm

The pass takes one branch-edge census and one snapshot of the block layout, then
visits each predecessor in layout order, retrying it immediately after a
successful merge.

This is sound because a merge removes only the jump edge into the removed block.
Moving that block's instructions preserves every outgoing edge, including
parallel edges and backedges, so incoming counts for surviving blocks do not
change. Only the absorbing predecessor gets a new terminator; previously
rejected surviving predecessors remain ineligible. Snapshot entries for blocks
that were already absorbed have empty instruction lists and are skipped.
Merged blocks leave the layout together at the end.

The result is the same first-eligible-layout merge sequence as a full rescan,
including backward layout edges, without a second census or membership
searches. Candidate checks are bounded by the snapshot length plus the number
of successful merges. The snapshot adds one allocation proportional to live
blocks; instruction movement and alias resolution keep their existing costs.
This does not make the whole merge operation linear.

<a id="simplify-cfg-worklist-invariants"></a>
### Invariants

- The entry block is never merged away, and a block never merges into itself.
- Block-parameter aliases are installed in parameter order before the jump is
  removed; instruction order and debug locations are unchanged.
- After each merge the removed destination's edge count is decremented and the
  absorbing predecessor's new terminator is re-examined.
- The outer cleanup rounds of `simplify-cfg` are unchanged.

No IR, calling, memory, canonicalization, verification or debug contract
changes. Unit tests in `simplify_cfg.rs` cover layout order, stale snapshot
entries, cycles and aliased arguments; `crates/keel-opt/tests/cfg.rs` covers
duplicate branch edges, switch dispatch, chains and invokes:

```sh
cargo test -p keel-opt --lib simplify_cfg
cargo test -p keel-opt --test cfg
```

<a id="small-memory-bounded-memory-intrinsic-expansion"></a>
## Bounded memory intrinsic expansion

The `small-memory` pass expands existing ordinary `memcpy`, `memmove`
and `memset` IR intrinsics with constant i64 lengths from 0 through 32 bytes.
Full optimization (`-O`) runs it immediately before `stack-forward`. Basic
and unoptimized compilation do not run it. Explicit `-p` pipelines continue
to select their own passes; for example,
`keel cc -O -p canonicalize,instcombine,small-memory,dce input.c`.
`memory-forward` and `loop-memory` remain opt-in. Full runs `loop-unroll`
in its late memory/loop epilogue.

The 32-byte cap bounds each expansion to at most six exact chunks. Greedy
8/4/2/1-byte integer accesses never reach outside the original extent. Loads
and stores use alignment 1, without notrap, volatile or alias promises. Both
supported targets are little endian and support these ordinary unaligned
accesses. Memcpy retains its intrinsic nonoverlap precondition. Every source
chunk is loaded before any destination store, including for memmove, so both
overlap directions, identical pointers and overlap within a chunk work.

Memset unsigned-extends its i8 fill and multiplies by the width's repeated
0x01 pattern. This repeats the low byte for constant and dynamic fills,
including frontend truncation from wider signed or unsigned integers. One
pattern is shared by all chunks of the same width. Original operands are
reused and their producers are never removed by this pass. Exact zero
removes only the intrinsic; calls and volatile argument evaluation remain.
Variable, negative and larger lengths retain the intrinsic unchanged.
Generated instructions inherit the intrinsic's source location and scope.
The normal pass manager still verifies after the pass.

This handles surviving intrinsics only. The C frontend already expands
aggregate copies and zeroing of up to 64 bytes in `copy_bytes_with_flags` and
`zero_bytes_with_flags`; it is unchanged. Ordinary library calls, checked
fortify calls, volatile operations, ABI, calling conventions and public IR
contracts are unchanged. No shared alias helper is needed by this pass.

<a id="small-memory-results"></a>
### Results

The measured 16-byte copy no longer calls `memcpy`; this does not by itself
establish an application speedup. A pipeline adding only `small-memory`
produced identical assembly for 44 measured translation units and identical
code in three full applications. Wider and dynamic copies still call the
library.

<a id="stack-promote-private-fixed-field-promotion"></a>
## Private fixed-field promotion

`stack-promote` runs in Full's late memory/loop epilogue and requires explicit
scheduling in other tiers. Allocator defaults are unchanged.
Run it after inlining, CFG cleanup and, for small intrinsic initialization,
`small-memory`; follow it with `phi-simplify,dce`.

The pass admits scalar i8/i16/i32/i64 and pointer fields reached solely through
fixed stack addresses and constant, nonnegative, in-bounds pointer offsets.
Accesses to an overlapping range must agree exactly in offset and type, except
that a fully covering ordinary integer store of a constant repeated byte may
initialize a smaller integer field. The byte-splat representation is independent
of endianness and field offset. Arbitrary nonuniform bits, pointer representations,
partial writes and overlapping reads are not reinterpreted. Disjoint fields are
independent. Unsupported uses escape the entire slot,
including ordinary calls, intrinsics, volatile/atomic accesses, address comparisons,
returning an address, storing it, and nonconstant pointer offsets. Debugger homes
are excluded. Inline assembly, frame observers, stack save/restore, returns-twice
calls (direct or indirect), invokes and landing pads exclude the function.
Unrelated ordinary calls remain in place and cannot observe an unescaped slot.

A greatest-fixed-point must-initialization analysis starts false at entry and
true elsewhere. Every load must be preceded by an exact store or a fully covering
byte-splat store on every path. Each covering store supplies a typed constant at
its original instruction position and source location, including loop-local
resets. The complete original store remains in memory if any overlapping read
is unpromoted; otherwise it is removed after promotion. This preserves all bytes
of padding and neighboring fields that remain observable.
This includes zero-trip and loop-entry paths; the pass never invents an undef,
zero, speculative read, libc contract or writeback. Initialized non-entry blocks
receive fresh parameters, every syntactic incoming edge receives its outgoing
field value, and loads/stores become SSA uses/definitions. Duplicate edges retain
independent argument lists. Existing aliased load definitions are excluded.
The initial implementation requires all blocks reachable and no entry backedge,
and caps functions at 256 blocks, 8192 instructions and 64 distinct fields. Parameters are initially
unpruned; `phi-simplify,dce` removes the redundant ones.

<a id="stack-promote-mixed-width-initialization"></a>
### Mixed-width initialization

Admission also handles `small-memory`'s i64 zero store at `acc + 16` together
with i32 counter updates, promoting the counter recurrence alongside two i64
accumulators. Dynamic or nonuniform wider initializers remain outside this
subset.

<a id="stack-promote-post-capture-promotion"></a>
### Post-capture promotion

Full also admits a bounded suffix after the entry block's final memory barrier.
A previously captured slot is eligible when subsequent accesses use exact,
disjoint fields and every other load derives from an incoming pointer parameter
through pointer addition. Incoming objects predate this activation's fixed slots.
Loaded pointers, initializer-returned pointers, globals, pointer casts and phi
pointers do not establish that disjointness. Calls, unknown writes, volatile and
atomic accesses, address observations, debug homes and frame/nonlocal observers
exclude the suffix. No edge can return to entry.

The initializer remains an ordinary call. Fresh, nontrapping field reads after
it seed the SSA values, including nonzero values and stable uninitialized bytes.
There is no assumption that `memset` writes zero or does not capture. All suffix
reads of a selected field use its current SSA value. Its stores can disappear
because no admitted observer can see them before this activation ends. The
additional seed reads stay inside their fixed slots and do not change the first
fault among external input reads. Unsupported aliases retain their stores.
The bounded implementation shares `stack-cache`'s SSA construction and limits.

<a id="stack-promote-post-capture-read-caching-opt-in"></a>
### Post-capture read caching (opt-in)

`stack-cache` complements private-slot promotion for an object passed to an
ordinary initializer. It seeds exact integer/pointer fields from memory after
the last entry-block effect and carries those values through bounded loop SSA.
It does not assume `memset` semantics, initialization to zero, or `nocapture`.
Keel's stable uninitialized-memory rule permits the seed read; its fixed-slot
bounds prove that this additional read cannot fault.

Every original store stays at its original location. An arbitrary read through
a pointer returned by the initializer can therefore still observe those writes.
The suffix rejects unknown writes, calls, volatile/atomic operations, and other
memory barriers. Overlapping field types stay in memory. Debug homes, frame
observers, exception continuations, dynamic stack operations and returns-twice
calls disable the transformation. No edge may return to the entry block.

Eligibility is bounded at 64 blocks, 2,048 instructions and eight cached fields;
address discovery makes at most eight scans. Only fields read inside a natural
loop are selected. The pass is available explicitly and is not part of the
Full or Fast schedules (`cargo test -p keel-opt --test stack_cache`).

## Bounded Fast pipeline

`keel cc --opt-tier fast input.c -o output.s` and
`keel compile --opt-tier fast input.keel -o output.s` select the opt-in Fast IR
pipeline. `keel opt --opt-tier fast input.keel` prints its optimized IR.
Library callers can use `keel_opt::OptimizationTier::Fast`.

The default remains None. Basic and Full retain their existing pipelines;
`-O` still selects Full and enables all selection passes. The last `-O` or
`--opt-tier` sets the IR tier; `-p` overrides the pipeline in either order.
An explicit Fast tier does not select the Fast register allocator. Full
allocation and baseline instruction selection remain the defaults.

Fast runs `fast-cfg,fast-local,fast-cfg,dce`. Each CFG pass folds branches,
forwards one jump-only edge per visited destination, removes unreachable
blocks with DFS, and merges straight-line chains up to 64 instructions.
There is no fixed-point loop. One forward reverse-postorder local pass shares
the existing instcombine rules for constant evaluation, integer identities,
casts, comparisons and power-of-two strength reduction. A table of at most
256 expressions reuses pure computations within each block; it clears when
full. A separately bounded table of 256 quotients reuses an already executed
same-block division when computing a matching remainder, retaining its trap
guard. Final mark-and-sweep DCE retains pinned operations, including traps,
ordinary and volatile memory accesses, calls, and terminators.

Fast builds no dominator tree or use graph, does no global value numbering,
reassociation, reciprocal division expansion, inlining or loop analysis.
It can miss simplifications exposed later in the pipeline and is not a
convergent canonicalizer. The budgets bound expression-table memory and
block movement; IR Vec removal and alias-chain costs prevent a strict linear
complexity claim for arbitrary input. The budgets are conservative engineering
limits, not experimentally optimal settings.

Arithmetic rewrites use the existing wrapping and trap rules. Division by zero
and signed MIN/-1 division remain traps. Signed MIN%-1 is defined as zero.
Kept and folded instructions retain their source locations; removed pure
instructions can disappear from debug stepping just as in other optimized tiers.

The goal is useful optimization at low compile cost. Runtime competitive with
Cranelift speed and compile cost competitive with Cranelift none are measurement
targets, not guarantees.
