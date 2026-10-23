# Compile-cost refresh at fef1af78: quiet gate deferred

Research base: `fef1af784a2179e08fcab49a4995f34a2cea29fe`, 2026-10-23.
This work owns this document and the external compiler-build/preflight artifacts.
It changes no production source, optimizer schedule, feature selection, ABI,
floating-point semantics or memory footprint. No subagents were spawned, sibling
changes integrated, main changed or other worktrees cleaned.

**No current compile-cost timings or hot-phase ranking were established.** The
bounded LinuxHost preflight failed before any timed compiler invocation. Historical
`bfa86ed7` phase numbers, including the roughly 213 ms optimizer observation,
are not measurements of this revision. The canonical-cache pilot and aggregate
cleanup have no established current compile-cost payoff; this refresh does not
propose another generic analysis/cache manager.

## Frozen compiler and ownership

An exact Git archive of the base was extracted into the private LinuxHost directory
`/tmp/keel-compile-fef1-r5/source`. All 2,007 archived regular files were compared
with the extracted build source after building: zero mismatches. Archive SHA-256:
`98877ad82562193cc233531ab29b3c6eccfd59106244bb198bf62c324c68702c`.

The shared release/default-feature compiler is the read-only copy
`/tmp/keel-compile-fef1-r5/frozen/keel`, SHA-256
`28ccf147d9cc46a4df93bd1eed6684c3b105fd397f63dc32c6d4de30b204e33d`.
It was built with the existing LinuxHost rustc 1.99.0 (`b940084d7`), preserving the
lockfile, toolchain selection and profile:

```sh
python3 suite/fast_opt_locked.py \
  /tmp/keel-compile-fef1-r5/build.jsonl \
  cargo build --offline --locked -j2 --release -p keel
```

The build succeeded using the shared default cargo cache and this checkout's
managed target. Its cache hits and build wall time establish no compiler
throughput result. No remote cache, compiler wrapper, target-directory override,
toolchain installation or output deletion was introduced.

The native runtime/profile worker received the immutable compiler path and
hash and owns all application builds, current correctness and execution,
runtime timings and profiles under `/tmp/keel-runtime-fef1-r5`. Its 44-TU raw
distribution/generated-QuickJS/header qualification is separate evidence. This
document does not claim that worker's pending checks passed. Compile research
launched no application executable and no duplicate runtime campaign.

## Bounded current quiet evidence

The shared-lock, CPU-2-affined preflight at **2026-10-23 15:57:18 UTC** recorded
one-minute load **0.442871**, with **0% busy** on CPU 2 and its SMT sibling 18
over a 250 ms probe. Nevertheless, the process snapshot showed the unrelated
`trilio_node` (PID 3590516) and `trilio_play` (PID 3590517) ROS sentry workload
under `/home/hxyulin/dev/trilio-node-20261023T152527Z/`. The no-competing-campaign
gate failed. A preceding read-only snapshot also recorded load **1.02**.

`campaign.json` retains `complete:false`, the process snapshot, failure and an
empty `runs` array. No warmup, paired compile measurement, RSS collection,
allocation counter run or diagnostic compiler execution followed. The initial
preflight wrapper returned zero because that draft harness caught its failure;
the report's explicit failure and incomplete state are authoritative. The
retained harness now returns nonzero on failure; it has not been rerun to spend
another contended campaign. The runtime worker was notified and retains its
serialized build/correctness/profile slot. No Apple timing control was attempted.

## Phase boundaries and remaining measurement gate

The existing tools were inspected before preparing instrumentation:

- `crates/keel/examples/profile_compile.rs` rejects Linux object output and
  uses default library selection/feature context; it is not the requested
  strict-v3, Full-selection, Full-allocation CLI object comparison.
- `suite/profile_backend.py` and its example target AArch64. The existing
  backend scopes are not a complete x86 selection/lowering budget.
- The C frontend profile's `parse_ms` includes lexing; `lex_ms` is a separate
  extra lexer pass. Those values cannot be added. Report-enabled optimizer
  pass observations add scans, allocations, event collection and serialization;
  they are diagnostic, not uninstrumented CLI wall time.

An **unbuilt, unvalidated**, external diagnostic overlay was prepared from the
same archive. Only its private `main.rs` and `x86_64.rs` add clocks/stderr records.
It preserves ordinary preprocessing, input verification, optimizer policy,
allocation validation and release emission legality. Planned boundaries are
preprocessing, frontend parse/IR, initial verification, optimization, inclusive
chunked codegen, module destruction and object finalization. Selection and
allocation-plus-validation are nested within codegen, as are assembler chunk
callbacks. They must not be summed into a second additive budget. No independent
x86 instruction-lowering time is currently available from this overlay.

After the runtime worker releases its slot and the host passes the quiet gate,
the prepared campaign would use exact raw `quickjs.c`, Lua `lvm.c` and SQLite
`sqlite3.c`, original source directories and the qualified `apps.py` definitions.
It fixes generic tuning, strict v3, Full selection and Full allocation while
varying only Full/Fast optimization. It requests one warmup and seven seeded
adjacent paired rounds, CPU 2 affinity, CPU 2/18 and host-load checks, source and
compiler identity checks, and stable object hashes per arm. Diagnostic objects
must match their ordinary counterpart. Full/Fast objects need not match each
other. Application correctness for the exact configurations is an additional
runtime-owner gate, not implied by object stability.

Separate paired diagnostic/ordinary wall observations must quantify overhead
before interpreting phase ranks; otherwise the overhead remains unbounded.
The three-TU screen is not a complete-application compilation score. A Clang
`-O2` reference would require the same raw input/configuration/ISA boundary and
explicit preprocessing accounting; no such reference was measured here.
Allocation counts and cumulative requested bytes, if collected with the existing
counting allocator, must remain separate from process peak RSS. Neither was
measured in this refresh.

## One conditional implementation investigation

There are **zero proven current hot compile phases** in this refresh. Do not
promote an implementation based on the deferred screen. If a later current
profile establishes meaningful optimizer allocation cost in
`instcombine::reuse_quotients`, one small investigation is to reuse a
function-local instruction snapshot buffer between blocks instead of allocating
`block_insts(block).to_vec()` for every block. The current remainder-presence
guard, block order, quotient table clearing, original-instruction visitation and
earlier-division requirement must remain intact. This proposes no new cache,
analysis lifetime or pass-manager scheduling.

Its proof gate is identical per-block instruction snapshots despite inserted
products, unchanged signed-overflow/zero-divisor trapping behavior, and exact
optimized IR/object parity on admitted tests and unchanged application inputs.
Its compile gate is a quiet seven-pair unchanged-input campaign, including
small functions without remainders; allocation reductions alone are insufficient.
Its application-quality gate belongs to the runtime worker and requires unchanged
correctness, code/stack size and runtime quality. Release legality, ABI checks,
FP semantics and exact load/store footprints remain enforced. This is a
source-inspection hypothesis, not a measured hotspot, implemented pilot or gain.

## External evidence and limits

LinuxHost `/tmp/keel-compile-fef1-r5/` contains `compiler-provenance.json`,
`build.log`, `build.jsonl`, `preflight-lock.jsonl`, incomplete `campaign.json`,
`campaign.py`, `setup_diagnostic.py`, `diagnostic-source/` and the frozen compiler.
The archive is `/tmp/keel-compile-fef1-r5-source.tar`. A local evidence copy and
the two external scripts are under `/tmp/keel-compile-fef1-r5/` on the originating
Mac. Script syntax checks passed; the diagnostic overlay and timing paths were
not built or exercised. Generated files remain outside Git.

The established results are compiler/source provenance, a successful current
release build and an explicit current timing deferral. They do not establish
frontend acceptance for every raw TU, lowering/encoding parity, native
application correctness, strict-level execution or compile/application speedup.
The [userspace ISA contract](userspace-isa-status.md), [C intrinsics](c-x86-intrinsics.md)
and [GNU vector ABI/semantics](gnu-vector-mode.md) remain the maintained boundaries.
