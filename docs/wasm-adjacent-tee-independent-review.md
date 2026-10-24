# Independent review of adjacent raw-local tee emission

Reviewed candidates: `19366ec6f1c21d998411ac48cc0d89446a25a15e` and
`11a1329e1ffde27eda50da3cea38639a5182407b`, originally based on
`a45e1e96723c6343206adf3634d43e5c627b6427`. They were inspected directly and
reapplied on that exact base as `963586972c58e92a55a92e85752115e8fc345d06`
and `69fd08c57a153224b33e45cd194304dca97ead3e`. No sibling candidate was applied.
The [pilot report](wasm-adjacent-tee-pilot.md), required gap audit, comparison
report and backend README were read before applying the implementation.

## Verdict and source boundary

No blocking defect was found in this bounded review. Production
`crates/keel-wasm/src/lib.rs` remains byte-identical to candidate `19366ec6`.
The review adds test coverage and documentation; no production fix was needed.
The only production seam remains the private `InstructionOutput` wrapper,
`Body` output field/construction, `op` forwarding and final `out.finish()`.
Parent owns any later integration, sibling conflict resolution and cleanup.

The buffer holds only one raw `LocalSet` index. A matching immediately next raw
`LocalGet` becomes `LocalTee`; every other operator emits the pending set first.
A second set flushes its predecessor before becoming pending. A matching get
clears pending state, so another get must still be emitted. Existing tees,
other local indices, all constants, calls, memory accesses, arithmetic and
structured control instructions are barriers. `finish()` retains a trailing
set even without a final End. The actual Body path still emits End and calls
finish. No resolved-alias equivalence is used to choose a match.

For stack prefix S and a correctly typed value x, both encodings leave S,x and
write x to exactly the same local. Local set/get do not trap or perform an
observable memory/call effect. No operation crosses another emitted instruction.
Declaration construction, raw producer destinations, resolved reads, phi-copy
ordering, control lowering, volatile widths/order, traps and normal-return SP
restoration remain unchanged. Existing original-input preflight, real WASM32
verification and final wasmparser WASM1 validation are unchanged. Public APIs,
frontend, optimizer, native backend and CLI are outside the diff.

## Strengthened independent checks

Two new buffer unit tests use literal expected bytes, independent of the
peephole oracle. One validates a complete MVP module containing both i32 and i64
tees above a live i64 stack prefix; Node executes it twenty times and checks
132n, including later reads of both written locals. The other checks consecutive
same/different sets, an extra get after folding, an existing tee, and a final
pending set. Partial fragments intentionally check encoding only; the complete
module receives MVP validation and runtime execution.

The unique `tests/adjacent_tee_review.rs` checks the decoded snapshot oracle
against hand-written byte expectations: mismatched locals and Nop barriers,
opcode-looking constant immediates, existing tees, declarations and valid padded
local-index LEBs. The oracle decodes operator spans, then copies original bytes;
it neither calls the production buffer nor repeats IR lowering. Matching is on
decoded raw indices while the set's original index bytes survive.

The original frozen dispatcher hex strings remain unchanged. Independent Python
decoding classified their eligible-pair counts as `[0,0,2,2,0]` for base profiles
and `[1,1,3,3,1]` for runtime. The maintained snapshot test now pins those counts
and explicitly requires original byte equality for all six nonmatching
scalar/fixed-memory comparisons, in addition to exact transformed-body equality
for every snapshot. This retains meaningful fallback evidence.

## Validation and provenance

On 2026-10-24, full focused keel-wasm debug and release runs each passed
59 integration tests plus four unit tests, zero failures or ignored tests.
Both profiles enabled the optional frozen corpus comparison, rather than merely
returning from that test without external inputs. Existing tests retain Node
trap/store observations, aliases, call/volatile order, exact memory widths,
cyclic phi transport, recursive frames and repeated normal SP restoration.
Locked check with `--all-targets` and build passed without compiler warnings.
Whitespace checks passed. No separate formatting tool was invoked.

Every Rust invocation used exactly the prescribed pattern:

```sh
python3 suite/fast_opt_locked.py /tmp/<unique>.jsonl \
  cargo test --offline --locked -j2 -p keel-wasm
```

Release added `--release`; check used `check` and `--all-targets`; build used
`build`. Corpus tests additionally set `KEEL_TEE_FROZEN_CORPUS` to the frozen r3
directory, and the first debug run set `KEEL_TEE_CORPUS_OUTPUT` to the review's
/tmp corpus directory. Existing pins/features, shared default cache and managed
checkout targets were retained. Logs and command records are
`/tmp/keel-wasm-tee-review-097ca30d-{debug,release,debug-final,release-final,check,check-final,build}.{log,jsonl}`.

The frozen manifest SHA-256 was rechecked as
`a43403a5ad24b52ea120c11706def605500efda98923166d2a6953463559e01d`.
Plain/basic IR hashes match the audit's separately pinned identities. All eleven
original body hex/operator/local records per profile agree with fresh Python
decoding and the frozen manifest. Original Wasm hashes and reused Clang control
hashes match that manifest. Fresh candidate hashes match the original pilot:
plain `807c584c0d4d79b2a857751cdaac405d2f62fa00d62ecfbdadf511b2f79cf8ea`,
basic `40cd05cc6402ac28398b9412ea66e4f5a82a307759750e4cc467f55a8a05aca1`.
The copied original manifest supplies harness admission metadata; its binary
hashes are not represented as candidate identities.

Static observations: plain bodies/operators change 2777/1321 to 2631/1248;
basic changes 2665/1257 to 2519/1184. Each saves 146 body bytes and 73 operators;
all 204 declared locals remain. Every candidate body exactly matches the decoded
snapshot transformation. Bodies include declarations and exclude their size
prefix. Candidate modules re-export every function from parsed frozen IR, so
these are backend body comparisons, not whole-module or new C pipeline results.

Separately, the unchanged Node v24.22.1 comparison harness passed 640 correctness
invocations across candidate plain/basic and frozen Clang O0/O2 controls, checking
results and each byte of the 1040-byte arena with sixteen canary bytes. Evidence:
`/tmp/keel-wasm-tee-review-097ca30d-node.{log,jsonl}`. No engine flags or timing
were used. These finite checks establish no runtime speed or native behavior
claim. No installation, cache/toolchain change, output deletion, subagents,
integration or worktree cleanup was performed.

Review modules and hash/count records are in
`/tmp/keel-wasm-tee-review-097ca30d-corpus/`. Its `review-provenance.json` SHA-256
is `792486df1e1fe45367dde2fd6fc554fab6516c460f197fa87ee401719f23dc90`.
Generated artifacts remain outside Git.
