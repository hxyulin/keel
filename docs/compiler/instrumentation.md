# Instrumentation and IR transport support

`keel-toolchain` is an experimental library with two independently usable
foundations: memory-access instrumentation and a target-tagged IR transport.
It is included in the workspace but does not change ordinary compilation,
optimization, linking, or the CLI flag surface. Neither ASan nor an LTO link
is implemented by these APIs.

## Implemented instrumentation boundary

`instrument::insert_memory_checks` validates an entire module and inserts
calls immediately before its explicit memory accesses. The callback ABI is:

```c
void __keel_check_memory_v1(void *address, uint64_t bytes, uint32_t flags);
```

Flags are READ=1, WRITE=2, ATOMIC=4, VOLATILE=8. Loads, stores, scalar/vector
access widths, pointer accesses, atomic loads/stores/RMW/CAS, and both ranges
of memcpy/memmove/memcmp are covered. Memset checks its destination. Lengths
of bulk operations remain dynamic. CAS checks a potentially written range;
the callback does not report whether a write actually occurred. Original
instructions, memory flags, atomic orderings and source locations are retained.
All supported targets currently have eight-byte pointers.

The library supplies no callback runtime. A consumer must provide an
uninstrumented runtime that preserves program memory and synchronization,
handles zero-length ranges without dereferencing their pointers, and stops
before performing an invalid access. No shadow-memory layout, compiler-rt ABI,
stack/global poisoning, heap interception, red zones, allocation registry or
use-after-free checking is supplied. Calls can touch memory in their callees;
the pass does not instrument external library internals. ABI-generated stack
traffic/spills are outside explicit IR accesses. This API is not a memory-safety
guarantee or ThreadSanitizer implementation.

Run this pass after normal optimization and before code generation. Running
it early can inhibit optimization and can leave checks for operations removed
later. Do not run an ordinary optimizer after instrumentation until the check
placement policy is defined. Existing hook symbols/calls are errors, preventing
recursive or accidental double instrumentation. Unsupported inline assembly,
variadic accesses and structured NEON memory operations reject the module
before any mutation. Additional access descriptions can extend this boundary.

The runtime integration test checks a valid store/load and then rejects an
invalid logical range before the access executes. Both code generators accept
the instrumented module; native execution links a small C oracle runtime with
Clang. Other tests cover atomic/volatile flags, pointer widths, dynamic bulk
ranges, reinstrumentation rejection and transactional failure.

## Implemented IR transport boundary

`artifact::encode` writes verified textual Keel IR inside a versioned envelope (currently `KEEL-IR-UNIT 4`)
with an explicit target triple and exact UTF-8 payload length. `decode` requires
the expected target, rejects incompatible versions/targets and malformed lengths,
and parses/verifies the complete payload. Value IDs are canonicalized through
the existing textual parser. This is an experimental Keel artifact, not LLVM
bitcode, a native object, a stable distribution format or a cache-key scheme.

Version 2 added ordinary weak symbol syntax, version 3 added executable
lifecycle and compiler-used function flags, and version 4 adds typed custom
section placement and linker-retention metadata. Older versions are rejected;
weak declarations and definitions survive encode/decode. Native symbol binding
is documented in [C weak symbols](../frontends/c/extensions.md).

The envelope version covers the IR dialect as well as its header and must be
bumped for incompatible IR changes. It records neither CPU feature requirements
nor build flags; consumers must not assume arbitrary artifacts are ABI-compatible
or cache-equivalent merely because their target triples match. Production LTO
must extend the manifest with explicit ABI/features, frontend semantic options,
sanitizer configuration and producer/IR compatibility before artifact reuse.

To exercise the preparation API without changing compiler flags:

```sh
cargo run -p keel-toolchain --example prepare -- \
  aarch64-apple-darwin input.keel output.kir --memory-checks
```

## Support limits

Memory-check insertion is available as a library API, with a consumer-supplied
runtime. The versioned IR envelope supports checked transport. Neither is a
complete ASan/UBSan runtime or production LTO link pipeline. Ordinary compilation
and linking do not enable them implicitly.
