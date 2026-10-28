# Function policy repair

The optimizer now preserves pass order under mixed function policies. The pinned
64-bit Linux v6.12 core compiles, links with Keel, packages and passes the checked
QEMU fixture at O2, Os and O3. The [ledger](../suite/linux-kernel/audit-policy-fix.json)
records source identities, object/image hashes and validation results.

## Changes

- Cold retains the invocation's IR pipeline. Size-oriented native emission and
  ELF placement remain active; the hint no longer selects an inliner-free plan.
- Different supported optimization-effort options do not prohibit helper
  inlining. Explicit noinline, disabled-inline, optnone/O0 and ABI legality
  checks remain in place.
- The manager merges plan frontiers without changing the order within a
  function's schedule. Inline rounds remain interleaved with cleanup. Shared
  canonical restoration, verification and analysis-version tracking remain.
- Inlining changes only scheduled callers but reads every recursion edge.
  Constant-argument propagation changes scheduled callees and checks every
  caller. Empty-call removal changes scheduled callers. Reachability cleanup
  preserves unscheduled bodies and their outgoing references as roots.
- Unscoped hooks, including ABI rewrites, still require every current body to
  enable that pass. The new Pass scope hook permits finer guards without
  silently mutating protected bodies. Some mixed-module ABI optimizations
  therefore remain conservatively unavailable.

Disabled bodies are skipped before canonical restoration. Tests cover
noncanonical protected bodies, their outgoing calls, conflicting constants in
protected callers, observed/unobserved parity and recursion across scopes.

## O2 and Os cleanup

Balanced now has three bounded inline/cleanup rounds, without GCM or loop
expansion. Size has two bounded helper-inlining rounds. Optional Size cases
require a hint, a literal argument or a single nonescaping local call, and are
capped at 128 IR instructions. Shared unhinted helpers with variable arguments
can remain calls. Existing binding, recursion, stack and growth limits apply;
alwaysinline and flatten retain their separate legality and budget rules.

Both presets run DCE after empty-call removal. This removes unused address
arguments such as disabled mitigation-table boundaries. Their declarations do
not require linker definitions once no emitted instruction references them.

This is not a general target byte-cost model. Size still omits GCM, memory
expansion, unrolling and automatic vectorization. Full's pass sequence is
unchanged. No instruction encodings, register allocators or compiler intrinsic
semantics changed. Naked/interrupt lowering remains unsupported.

## Qualification

All 484 C inputs and 22 handwritten assembly inputs compiled at each preset:
1,518 Keel objects in total. Ordered archive payloads were verified, objtool
returned zero for every invocation, and all three cores linked with Keel.
The existing objtool warning backlog remains. Packaging used the pinned
external boot/setup/decompressor stages and vDSO preparation.

All three images passed VFS, memory/protection, fork/COW, wait and pipe checks
under QEMU 11.1.2 TCG, with exact success markers and exit 33. Final-source
objects were freshly regenerated and matched every boot-qualified object.
Fresh final-source links also matched the qualified cores byte for byte.

The isolated compiler snapshot starts at the recorded base revision with the
optimizer source hashes in the ledger. Concurrent frontend edits were excluded.
This qualifies the captured minimal core, not raw-source Kbuild, arbitrary
configurations, stack-unwinding fidelity or native bare-metal execution.

The workspace run passed **3,039 tests**, zero failures and 15 ignored across
579 groups. All 14 kernel-fixture Python tests, eight explicit reduced probes
and twelve focused optimizer-policy tests passed. Workspace all-target/all-feature
checking and documentation builds passed. Focused Clippy passed with existing
warnings; strict workspace Clippy remains blocked by existing keel-link/keel-ir
findings. The pass registry preserves existing positions and appends inline-size.

Fresh [compiler and runtime measurements](linux-kernel-repaired-benchmarks.md)
replace the failed checkpoint for performance comparisons. The original
[investigation](function-policy-investigation.md) remains a historical record.
