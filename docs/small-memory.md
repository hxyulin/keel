# Bounded memory intrinsic expansion

The opt-in `small-memory` pass expands existing ordinary `memcpy`, `memmove`
and `memset` IR intrinsics with constant i64 lengths from 0 through 32 bytes.
It is registered by name and is absent from the default and Basic pipelines.
For example, `keel cc -O -p canonicalize,instcombine,small-memory,dce input.c`
selects it explicitly. Run it before allocation. Pipeline activation belongs
to the integration owner after combined correctness and cost gates.

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

`crates/keel-opt/tests/small_memory.rs` checks every length through 32,
fallbacks, source locations, flags, ordering, determinism and zero-length
side effects. Its opt-in native differential test generates C wrappers for
lengths 0 through 33 and a variable length route. It compares complete
buffers against Clang O0 and O2 with all address residues modulo 8, ten fill
values and every memmove displacement from -33 through 33, including zero.
Separate exact-extent source/destination tests abut protected pages at both
ends. The native test also checks C argument evaluation at zero length,
smokes ARM and x86 codegen and requires no memcpy call in the native 16-byte
wrapper. Run serially under the native validation lease:

```sh
cargo build --offline -j2 -p keel
KEEL_SMALL_MEMORY_BIN="$PWD/target/debug/keel" cargo test --offline -j2 \
  -p keel-opt --test small_memory native_clang_differential -- --ignored --nocapture
cargo test --offline -j2 -p keel-opt -p keel-ir
cargo test --offline -j2 -p keel-c --test memory --test memory_builtins
```

Native artifacts stay under ignored `target/`; campaign logs stay under
ignored `suite/reports/`. This pass has no measured runtime, application,
code-size or compile-cost claim. The audited frontend/LLVM kernel contrasts
motivate investigation and do not predict this patch's speedup. Root owns
later serial performance, application, code-size and 44-TU compile-cost gates,
plus independent correctness review of the committed checkpoint.

Integration note: registering a pass changes the unknown-pass diagnostic.
The shared `crates/keel-opt/tests/golden/unknown_pass.out` fixture needs a
combined registry update by root. This branch leaves that file outside its
ownership unchanged; the full optimizer run currently fails only that
expected diagnostic comparison. The remaining optimizer tests pass with
`cargo test --offline -j2 -p keel-opt -- --skip golden --test-threads=1`.
