# Exact adjacent raw-local tee emission pilot

Base: `a45e1e96723c6343206adf3634d43e5c627b6427`.
Implementation and regressions: `19366ec6f1c21d998411ac48cc0d89446a25a15e`.
This is the first bounded backend pilot from the
[gap audit](wasm-clang-gap-audit-e3391b54.md), read alongside the
[comparison](wasm-clang-comparison-e3391b54.md) and
[backend contract](../crates/keel-wasm/README.md).
Parent owns independent review, combination with the other pilots and integration.

## Rule and implementation seam

Only adjacent emitted `local.set L; local.get L` becomes `local.tee L`, with
exactly the same raw local index. `InstructionOutput`, immediately before
`Body`, wraps the existing wasm-encoder function. It holds at most one pending
raw `LocalSet`. A matching immediate `LocalGet` emits a tee; every other
operator flushes the set before handling that operator. A subsequent set can
become the next pending instruction. `finish()` flushes a trailing pending set.
`Body` changes only its output field, construction, `op` delegation and final
`self.out.finish()` return. The instruction/control/conversion lowering remains
shared and unchanged.

For any stack prefix S and correctly typed value x, the original pair takes
`S,x` to `S` while writing local L=x, then pushes that same x. The tee writes
L=x and leaves `S,x`. Both local state and stack value/type agree, including
when S is nonempty. Neither operator can access memory, call, trap or change
control. Buffering changes only encoding; it never delays a set past any
intervening operator in the emitted program.

Every control operator is a barrier, including block/loop/if/else/end, branch,
return and unreachable. So are constants, calls, memory operations, arithmetic,
other-index local reads, and existing tees. This has no liveness, local reuse,
IR scheduling, alias substitution or phi deletion. All declarations, raw
producer destinations, resolved operand reads, parallel edge copies, allocation
checks and normal-return SP restoration are preserved. In particular, the
runtime prologue's adjacent save/reload is eligible without removing any guard.
Original-input admission, actual WASM32 verification and final MVP validation
are untouched, as are public APIs, optimizer, frontend, native code and CLI.

## Maintained regression evidence

`tests/adjacent_tee.rs` requires wasmparser WASM1 validation and Node execution.
All three profiles exercise actual tee activation, unchanged local counts,
i32/i64 stack typing, live values across calls, retained raw alias destinations
and cyclic phi transport. `tests/support/adjacent_tee_output.rs`, included only
for unit testing, directly checks buffer barriers, mismatched raw locals,
repeated pairs, multi-byte indices and a trailing set at finish. Partial control
fragments are compared byte-for-byte; full emitted integration modules receive
MVP semantic validation.

`tests/support/adjacent_tee.rs` is an independent decoded byte-span oracle. It
copies declarations and all original operator bytes except a decoded immediate
same-index set/get pair, whose opcode becomes tee with the identical index LEB.
The old `direct_return.rs` frozen dispatcher hex remains unchanged. Its
assertion applies only this decoded peephole to the original expected bodies;
nonmatching expectations remain exactly the original bytes. This is not broad
snapshot regeneration or relaxation of CFG expectations.

The complete existing suites additionally cover unused volatile and ordinary
loads, retained aliased division producers (zero and signed-overflow traps),
observable stores before faults, call/volatile order, memory widths and pointer
normalization, removed layout blocks, switch/duplicate-edge phi copies,
recursive frames, repeated normal returns, and allocation failure before stores.
Trapping instances are discarded. Admission/preflight rejection controls pass.

On 2026-10-24, Node v24.22.1 ran locally. Final debug and release each passed
58 integration tests plus two output-buffer unit tests, zero failures/ignored.
Locked check with `--all-targets` and build passed without warnings.
Whitespace checks passed. An earlier unit fixture tried decoding an isolated
`else` with no enclosing if and was correctly rejected by wasmparser; the
fixture now checks partial-control output bytes directly. Production code
needed no correction. Append-only command records retain that failed attempt.

All compilation used the prescribed shared lock, disabled link caching,
offline/locked dependencies, two jobs, existing toolchain/features and cargo's
default shared cache with checkout-managed targets. Exact command patterns:

```sh
KEEL_TEE_FROZEN_CORPUS=/tmp/keel-wasm-comparison-e3391b54-20261024-r3 \
  KEEL_TEE_CORPUS_OUTPUT=/tmp/keel-wasm-tee-9d54e6cf-corpus \
  python3 suite/fast_opt_locked.py /tmp/keel-wasm-tee-9d54e6cf-debug-final.jsonl \
  cargo test --offline --locked -j2 -p keel-wasm -- --nocapture
KEEL_TEE_FROZEN_CORPUS=/tmp/keel-wasm-comparison-e3391b54-20261024-r3 \
  python3 suite/fast_opt_locked.py /tmp/keel-wasm-tee-9d54e6cf-release.jsonl \
  cargo test --offline --locked -j2 -p keel-wasm --release -- --nocapture
python3 suite/fast_opt_locked.py /tmp/keel-wasm-tee-9d54e6cf-check.jsonl \
  cargo check --offline --locked -j2 -p keel-wasm --all-targets
python3 suite/fast_opt_locked.py /tmp/keel-wasm-tee-9d54e6cf-build.jsonl \
  cargo build --offline --locked -j2 -p keel-wasm
```

Corresponding output is beside each command record as `.log`. No installation,
toolchain/cache configuration change, manual output deletion, subagents,
integration or worktree cleanup was performed.

## Static corpus changes, separate from execution

The optional `KEEL_TEE_FROZEN_CORPUS` test input uses the frozen plain/basic IR
and Wasm in `/tmp/keel-wasm-comparison-e3391b54-20261024-r3`. Without it, ordinary
maintained tests have no dependency on temporary artifacts. With it, all eleven
function bodies in each profile must exactly equal the independent byte-span
transformation of the frozen bodies, including local declarations and all
nonmatching operators. Counts are asserted rather than inferred from size.

Both plain and basic have exactly 73 pairs, saving 146 body bytes and 73
operators, with all 204 declared locals unchanged. Plain totals change from
2777 body bytes / 1321 operators to 2631 / 1248. Basic changes from
2665 / 1257 to 2519 / 1184. Body bytes include declarations but exclude the
body-size prefix. For example, scalar changes 114/50/10 to 108/47/10, and
basic pointer_mutate changes 353/163/32 to 321/147/32. These are encoding
observations, not machine-code size, compilation latency or execution speed.

The optional output directory writes only into `/tmp`. Candidate modules are
reemitted from frozen parsed IR and export every function for test convenience;
therefore no whole-module byte delta or newly rerun C frontend/optimizer result
is claimed. Candidate body equality is checked against the frozen original
backend output, rather than a second implementation of IR lowering. Static
statistics and hashes are in
`/tmp/keel-wasm-tee-9d54e6cf-corpus/candidate-metrics.json`.

## Finite runtime evidence

The unchanged comparison `harness.cjs` ran with `--correctness-only`, candidate
plain/basic modules and frozen Clang O0/O2 control modules. It passed all 640
invocations, checking return values and every byte of the 1040-byte arena,
including sixteen canary bytes, using its independent BigInt oracle. The input
manifest copied into the temporary directory supplies profile admission metadata;
its original binary hashes are not candidate identities. Candidate hashes and
body statistics are recorded separately in `candidate-metrics.json` above.
Node reports no engine flags. Exact evidence:

- `/tmp/keel-wasm-tee-9d54e6cf-corpus-node.jsonl`
- `/tmp/keel-wasm-tee-9d54e6cf-corpus-node.log`

This finite corpus and the backend trap regressions support correctness only.
No timing or runtime performance claim was made under contention. Native
execution, general liveness/stackification and larger CFG transformations remain
outside this pilot. When combining siblings, parent must retain `out.finish()`
and apply the same decoded peephole to their frozen unmatched snapshots; the
terminal-if pilot independently changes eligibility of the existing branch
snapshot and owns that separate expectation.
