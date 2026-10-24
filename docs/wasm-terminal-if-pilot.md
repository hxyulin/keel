# Terminal `if/else` emission

The `keel-wasm` backend emits one common CFG shape, an entry block that branches to
two returning arms, as a structured MVP `if/else` instead of the general
program-counter dispatcher. See the [crate README](../crates/keel-wasm/README.md)
for the overall backend contract and the
[scalar backend](wasm-scalar-backend.md) for the dispatcher.

## Eligible shape

Selection happens after normal whole-module admission. A function is eligible when:

- its layout has exactly three blocks, with the entry first;
- the entry ends in `brif` to the two other, distinct blocks;
- neither arm has block parameters, and the edges pass no arguments;
- each arm ends in an ordinary `Return` (no earlier terminators);
- every operand in an arm refers to an entry parameter, an earlier entry definition
  or an earlier definition in the same arm (no cross-arm values).

Removed blocks do not count; any extra laid-out block disables selection. Invalid
or non-dominating input is still rejected by the verifier rather than passed to the
fallback.

## Emission

The entry block is emitted, then an empty-typed `if` containing the true arm and an
`else` containing the false arm, regardless of the arms' layout order. Each arm uses
the existing instruction and `Return` emitters. A trailing `unreachable` provides
the function's continuation after the empty-typed `if`, keeping MVP result typing
without transporting a result value.

The program-counter local, its comparisons, the dispatch loop and edge copies are
omitted. All other locals keep their layout-based indices (runtime saved-SP and
frame-base locals shift down by the omitted PC slot). Stack-allocation checks and SP
restoration on every return are unchanged. Calls, recursion, memory accesses,
volatile order and faulting instructions are neither removed nor reordered, and
only the selected arm executes.

This is not a general CFG structurizer: there is no loop transformation, `select`
speculation or stackification. The single-return-block direct emission path is
unchanged.

## Effect

On the `suite/wasm-comparison` C corpus, only the basic-optimized
`recursive_helper` matched; its body shrank from 246 to 204 bytes and 116 to 92
operators, with one fewer local. The plain build had no eligible functions. This is
an encoding observation, not a runtime-speed claim.

## Testing

`crates/keel-wasm/tests/terminal_if.rs` validates modules with wasmparser
(WebAssembly 1.0) and runs them in Node. It covers void, i32 and i64 returns, i64
conditions with only high bits set, reversed arm layout, aliases and retained
producers, division traps only on the selected arm, exact-width volatile and
ordinary loads with full-memory canaries, recursive address-taken frames and
repeated returns, stack exhaustion before any store, and rejected cross-arm
definitions. Byte snapshots of 18 non-matching shapes (parameter arms, duplicate
successors, a non-terminal arm, four blocks, switch, loop phi swaps) confirm that
they still use the dispatcher unchanged.

```sh
cargo test -p keel-wasm --test terminal_if --test terminal_if_review
```
