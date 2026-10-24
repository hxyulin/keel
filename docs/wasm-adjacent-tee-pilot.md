# Adjacent `local.tee` emission

The `keel-wasm` backend rewrites an emitted `local.set L` that is immediately
followed by `local.get L` (the same raw local index) into a single `local.tee L`.
This is a pure encoding peephole in the instruction output stage; see the
[crate README](../crates/keel-wasm/README.md) for the overall backend contract.

## Rule

`InstructionOutput` wraps the function body encoder and holds at most one pending
`local.set`. If the very next operator is `local.get` of the same index, it emits
`local.tee` instead. Any other operator flushes the pending set first and is then
handled normally (a new set may become pending). `finish()` flushes a trailing
pending set. Instruction, control-flow and conversion lowering are unchanged.

**Why it is exact.** For any stack prefix S and value x, `local.set L; local.get L`
takes `S, x` to `S, x` and writes L = x. `local.tee L` does the same. Neither
operator touches memory, calls, traps or changes control flow, and buffering never
moves a set past any other emitted operator.

Every other operator is a barrier: all control operators (block, loop, if, else,
end, branches, return, unreachable), constants, calls, memory operations,
arithmetic, reads of other locals, and existing tees. There is no liveness
analysis, local reuse, scheduling, alias substitution or phi removal. Local
declarations, raw producer destinations, resolved operand reads, parallel edge
copies, stack-allocation checks and SP restoration are all preserved. The runtime
prologue's adjacent save/reload of the stack pointer is eligible, without removing
any check.

## Effect

On the `suite/wasm-comparison` C corpus, both plain and basic-optimized output
contained 73 eligible pairs, each saving two body bytes and one operator, with the
number of declared locals unchanged. This is an encoding-size observation, not a
claim about machine-code size or execution speed.

## Testing

- `crates/keel-wasm/tests/adjacent_tee.rs`: in all three profiles, checks that tees
  are actually emitted, local counts are unchanged, i32/i64 stack typing is correct,
  and values survive calls, aliasing and cyclic phi transport. Modules are
  validated with wasmparser (WebAssembly 1.0) and executed in Node.
- `tests/support/adjacent_tee_output.rs`: unit tests of the buffer itself (barriers,
  mismatched locals, repeated pairs, multi-byte indices, trailing set).
- `tests/support/adjacent_tee.rs`: an independent oracle that applies the same
  rewrite to decoded operator bytes. The frozen dispatcher snapshots in other tests
  are compared after applying only this peephole.

```sh
cargo test -p keel-wasm
```
