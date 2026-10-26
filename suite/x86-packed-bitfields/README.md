# Packed bit-field checks

The native LP64 C frontend supports packed fields that occupy less than their declared
integer container or cross its boundary. Loads assemble the field from its
bytes. Stores update its boundary bytes without changing neighboring bits and
write whole interior bytes directly. No access extends outside the field's byte
span. Signed loads and expression results use field-width sign extension.
Volatile flags reach each access; the implementation does not promise a single
access suitable for a device register. Existing aggregate ABI and atomic limits
still apply.

`crates/keel-c/tests/packed_bitfields.rs` covers native target layout, static and
local initializers, volatile flags, IR text round-tripping, and text/typed/chunked
x86 object parity. Its native Clang check links an independent byte oracle that
places objects beside inaccessible pages, checks surrounding canaries, and tests
assignment and prefix/postfix results beyond signed and unsigned field ranges.
The fixture includes individual packed members, packed unions, zero-width
barriers, pragma packing, and signed/unsigned 63-bit boundaries.

Export the 48 explicit-V1 configurations:

```sh
KEEL_X86_PACKED_BITS_EXPORT=/tmp/packed-objects \
  cargo test -p keel-c --test packed_bitfields
```

Run them on Linux x86-64 with an independent GCC reference:

```sh
python3 suite/x86-packed-bitfields/run.py \
  --objects /tmp/packed-objects --work-dir /tmp/packed-native
```

The runner checks exact PASS transcripts. Five broken references test wrong
signed reads, wrong wide writes, clobbered neighbors, wrong initialization, and
wrong compound updates. Each must return its specified ordinary error code.
Faults and timeouts do not count as rejected controls.

The layout matrix generates 256 records from seed `0x8617a539`, using mixed
integer types, zero-width barriers, ordinary byte members, GNU packed records,
and pragma pack values 1, 2, 4 and 8. It compares object bytes and symbol sizes
for every record initializer and its sizeof/alignof pair against Clang, on both
x86-64 Linux and AArch64 Linux:

```sh
cargo build -p keel
python3 suite/x86-packed-bitfields/layout_matrix.py \
  --keel target/debug/keel --work-dir /tmp/packed-layouts
```

These are userspace and object-layout checks. They do not qualify kernel
code generation or device accesses.
