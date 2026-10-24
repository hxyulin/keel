# Low-bit cast chain emission

The `keel-wasm` backend skips two exact chains of pure conversions whose net effect
on the 32-bit Wasm representation is the identity. See the
[crate README](../crates/keel-wasm/README.md) for the overall backend contract.

## Recognized chains

Only adjacent conversions within one layout block are matched:

- i32 source → `uext.i64` → `trunc.i32` → `uext.i64` → `inttoptr.ptr`
- ptr source → `ptrtoint.i64` → `trunc.i32`

Conditions:

- Every intermediate has exactly one use (counted over all laid-out operands after
  alias resolution, including calls, returns and edge arguments), feeds the next
  conversion directly, is not an alias, and has no alias pointing to it.
- Each conversion has exactly the opcode and result type above.
- At most four instructions are inspected per candidate; there is no recursive
  expression matching, CSE or control-flow analysis.

Aliases and shared uses of the *final* result are allowed, since that result is
still materialized. Cross-block chains, multi-use intermediates and other cast
shapes are emitted literally.

## Why it is exact

For any i32 bit pattern x, the first chain computes
`low32(zext(low32(zext(x)))) = x`. For the second, a Wasm pointer's i32 address p
becomes `low32(zext(p)) = p`. These are identities of the backend's
representation; no native pointer identity is assumed. A general
`inttoptr(i64) → ptrtoint.i64` still discards the upper 32 bits and is emitted as
written, and `ptradd` still wraps before a zero-offset memory access.

Only the intermediate conversions and their local stores are omitted. The final
instruction reads the original source and writes its own raw destination; all
local declarations and indices are unchanged. Because the skipped instructions are
adjacent pure conversions, deferring the source read cannot cross a call, fault,
volatile access, block edge or write to the source local.

## Effect

On the `suite/wasm-comparison` C corpus, three functions changed
(`object_offset`, `buffer_offset`, `pointer_mutate`), saving 28 body bytes and 18
operators in each of plain and basic output, with declared locals unchanged. All
other function bodies were byte-identical. This is not a runtime-speed claim.

## Testing

`crates/keel-wasm/tests/low_bit_casts.rs` validates modules with wasmparser
(WebAssembly 1.0) and executes them in Node against BigInt expectations for zero,
negative values, signed extremes, `0x80000000`, `0xffffffff`, 2^32 and full-width
i64 inputs. Decoded-operator assertions confirm both that matches fire and that
non-matching chains (calls, multi-use intermediates, phi transport, cross-block
chains, aliased intermediates) are left alone. Memory tests compare all 64 KiB of
memory around unaligned volatile accesses, wrapping offsets and the last valid
address, and check OOB stores, call ordering and store/division/store ordering on
fresh instances.

```sh
cargo test -p keel-wasm --test low_bit_casts
```
