# Independent combined Wasm backend validation

On 2026-10-24, the integrated starting base `123f8505` passed independent
combined validation against `a45e1e96`. No blocking production defect or lost
integration seam was found. No production source was changed during validation.
This report covers the three already reviewed implementations: adjacent tee
(`43bc6134`), terminal-if (`0a66babf`) and low-bit casts (`a6ef4c8d`).
Validation tests and expectation corrections are committed as `afad55a03a3377974dfa17d6d95c661e46503303`.
Parent owns subsequent main integration and cleanup.

Read with the [backend contract](../crates/keel-wasm/README.md), the three
[tee](wasm-adjacent-tee-pilot.md), [terminal](wasm-terminal-if-pilot.md) and
[cast](wasm-low-bit-casts-pilot.md) pilot reports, their independent reviews,
and the [original comparison](wasm-clang-comparison-e3391b54.md).
The committed [machine-readable evidence](wasm-combined-validation-123f8505.json)
contains command arguments, hashes, per-function deltas and gate results.

## Combined source audit

The `InstructionOutput` wrapper plus its unit-test inclusion, `terminal_if`
selector, and `LowBitCasts` recognizer are each byte-identical to their respective
reviewed sources. Their range hashes are recorded in the evidence JSON. The
combined `lib.rs` SHA-256 remains
`f5bc82e650567b8a92c9916eff5291e7876ccc65de473a46827f1de5324354b0`, identical
to the starting base. Against `a45e1e96`, production changes are confined to
this file; runtime preparation, frontend, optimizer, native backend and CLI
sources are unchanged.

Both new Body fields and initializers survive. Terminal selection determines
whether the PC local exists, and saved-SP/frame-base indices use the same
dispatch-local count. Cast analysis still counts original laid-out operand
occurrences and aliases, before any emission skips. Every selected block uses
the same instruction loop: covered pure conversions are skipped, final cast
results read the original resolved source, and the ordinary raw destination
store remains. The terminal path visits entry/true/false while the fallback
keeps layout order and parallel phi copies. Every structural operator goes
through `Body::op` and the output wrapper, and `Body::emit` returns
`self.out.finish()` after the function End.

The terminal availability scan still sees original definitions; skipping pure
casts does not change the IR it checks or any local declaration. Pending sets
cannot cross an if/else/end, call, load/store, return, arithmetic operation or
trap. Cast removal can create new adjacency only where omitted conversions
were pure and exact. It does not move a source read across an effect. Raw alias
destinations, selected-arm execution, allocation guards, normal-return SP
restoration, admission and final MVP validation remain shared.

## Independent expectations and interaction regression

The initial optional frozen-corpus test failed because its tee-only expected
getter still contained the conversions removed by the reviewed cast pilot.
The correction preserves original frozen bytes for every unaffected body.
For the two getters and pointer_mutate, pre-tee expectations come directly from
the separately reviewed cast artifacts with their documented hashes. Only the
basic recursive helper uses the separately reviewed terminal artifact. The
literal bodies and provenance are in
`tests/support/combined_corpus.rs`. Their hashes were checked again before use.
The existing independent decoded byte-span tee oracle is applied afterward.
No expected body was captured from combined output.

All 22 bodies must match exactly, including declarations and every unaffected
operator. Both profiles have exactly 68 eligible pairs after sibling changes,
compared with 73 in the original tee-only pilot. Removing casts also removes
five pair opportunities, so standalone byte savings cannot simply be summed.
The test separately pins byte/operator reductions and the single removed basic
PC local. Existing direct-return and terminal fallback snapshot strings and
their decoded tee expectations remain unchanged; all pass.

One added regression closes the direct three-way interaction gap: a framed
terminal conditional contains exact cast chains in both arms, with reversed
layout, raw aliased signed division in the false arm, unaligned volatile stores
and loads, and a stack-address getter. Decoding requires no conversion
operators, no dispatch loop, one else, the retained division and actual tees.
Independent Node checks exercise both normal returns repeatedly, full 64-KiB
memory including frame canaries, an address deliberately overlapping the
frame's first byte, division-zero/overflow only on the selected arm, and OOB
stores without partial writes. A getter after every normal call pins restored
SP at 65520. Each trap uses a fresh instance and is then discarded.

The first version of this new fixture incorrectly assumed the frame value
remained seven after the deliberately overlapping write. Its independent byte
oracle now computes the result from the expected post-store bytes; production
behavior was correct. The initial failure is retained in the interaction log.
Existing tests additionally exercise duplicate edges, pointer/ordinary phi
swaps and cycles, calls, shared/intermediate aliases, retained faulting producers,
recursive frames, exhaustion before effects and original-input rejection.

## Gates

All final gates pass without Rust compiler warnings or ignored tests:

- Full keel-wasm debug and release: 78 integration tests and four unit tests
  in each profile, with the optional frozen corpus enabled in both.
- CLI debug and release: 68 tests each, comprising 38 binary tests, all 18
  wasm_output tests (including basic/plain trap, mutation, pointer-phi and
  recursion controls), two wasm_target_unavailable, seven system_link_args
  and three x86_cpu_level tests. Native controls check routing and generated
  outputs; this is not a native application execution campaign.
- Five keel-opt wasm32_basic release tests and 19 maintained comparison
  decoder/runner tests.
- `cargo check --all-targets -p keel-wasm -p keel -p keel-opt` and
  `cargo build --release -p keel-wasm -p keel`; whitespace checks also pass.

Every Rust invocation used the shared suite lock and exact prefix:

```text
python3 suite/fast_opt_locked.py /tmp/wasm-combined-123f-<gate>.jsonl
  cargo <test|check|build> --offline --locked -j2 ...
```

Backend test gates set `KEEL_TEE_FROZEN_CORPUS` to the frozen r3 directory;
release gates add `--release`. Existing pins/features, default shared cache and
checkout-managed target storage were retained. Command/output prefixes are
`/tmp/wasm-combined-123f-`, with `debug-final`, `release-final`, `cli-debug`,
`cli-release`, `opt-release`, `check`, `build-cli` and `build-release`
`.jsonl`/`.log` files. Initial `debug-initial` and `interaction` failures are
retained alongside passing focused corrections. No installations, remote work,
subagents, cache/toolchain changes, manual output deletion or cleanup occurred.

## Frozen release compiler and corpus

The release CLI was copied to `/tmp/wasm-combined-123f-corpus/keel` before
emission and made read-only/executable. SHA-256 before and after the final
release build agrees:
`648e2e5d82faa2e4d7a6081c833574bd3eeeedd1fcf26f2c34655d7b737dd488`.
It emitted the maintained corpus using the original plain/basic settings,
exports, one-page memory, data floor 1024 and stack interval [32768,65536).
Custom sections were stripped with the maintained decoder. The original Clang
O0/O2 modules were reused unchanged, rather than rebuilt.

Frozen reference:
`/tmp/keel-wasm-comparison-e3391b54-20261024-r3`. Its manifest SHA-256 remains
`a43403a5ad24b52ea120c11706def605500efda98923166d2a6953463559e01d`; corpus
SHA-256 remains
`6323eab334c63c49344cf2a792b9a435787d93002a229a7b7abd7ab7c6bd38cf`.
Both frozen IR hashes and all four original Wasm hashes were rechecked.
The e3391b54 and a45e1e96 backend production sources are byte-identical, so
the original binary/body baseline applies. A separate Python section/local/
operator walker agrees with the maintained decoder on all 88 baseline and
candidate bodies, including exact body hex. It does not reuse the maintained
decoder's section, LEB or operator iteration; Node supplies semantic validation.

Body bytes include declarations and exclude the enclosing body-size LEB:

| Profile | Body bytes old → combined | Operators old → combined | Locals old → combined |
| --- | ---: | ---: | ---: |
| Plain | 2777 → 2613 | 1321 → 1235 | 204 → 204 |
| Basic | 2665 → 2459 | 1257 → 1147 | 204 → 203 |
| Frozen Clang O0 | 1347 → 1347 | 601 → 601 | 20 → 20 |
| Frozen Clang O2 | 1231 → 1231 | 609 → 609 | 14 → 14 |

Plain saves 164 body bytes and 86 operators; basic saves 206 and 110.
The sole local reduction is the basic recursive helper's PC local. Stripped
module bytes change 3052 → 2887 plain and 2940 → 2733 basic; the additional
module-byte difference includes body-length prefixes. Candidate hashes are:

- Plain: `a6a31b7c99239cb4c65894c467dd82c2e843ad882ce14556327d6b6299b94bee`.
- Basic: `2f570e7c237b1c8bd84538a92de7ef2fa88781571a94cd5cb23f48ab26380310`.

Separately, the unchanged maintained harness ran with `--correctness-only`
under the suite lock. Node v24.22.1 with no engine flags validated and
instantiated all four modules and passed all 640 independent BigInt-result
and exact 1040-byte arena checks, including sixteen canary bytes: 320 fresh
Keel invocations and 320 frozen Clang controls. Records/output are
`/tmp/wasm-combined-123f-{corpus,static,node}.{jsonl,log}`. Emission/analysis
helpers share that prefix as `.py` files; candidate modules, decoded bodies,
deltas and hash evidence reside in `/tmp/wasm-combined-123f-corpus/`.
The small candidate manifest supplies harness emission metadata; independently
recorded hashes identify the actual candidate binaries.

These are static encoding observations and finite correctness checks. Clang
O2's 1231 body bytes remain a static reference with different frame/ABI and
optimization choices. No timing was requested or run, and no runtime, JIT,
compile-speed or application-performance claim follows.
