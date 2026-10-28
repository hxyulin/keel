# LLVM importer port

The Rust importer preserves the Python importer's supported domain and rejects
unsupported contracts. It uses LLVM 23's real parser, verifier and TargetData;
it does not parse LLVM text with a replacement grammar.

## API mapping

| Python | Rust |
| --- | --- |
| `LLVM(path)` | `import::Importer::load(path)`, private `ffi::Api` |
| `import_module(llvm, source, localize_odr)` | `Importer::translate(source, Options)` |
| `Importer` translation state | Private per-call `Translation` |
| `signature`, `abi_signature` | Signature checks in `import/mod.rs` |
| `instruction`, `value`, `edge`, aggregate/EH handling | `import/instructions.rs` and translation helpers |
| `Globals` | `import/globals.rs` |
| `local_copies` | `import/linkage.rs` |
| `Unsupported` | `import::ImportError` |
| `llvm_import.py` CLI | `keel-llvm-import` |

`ffi.rs` confines dynamic symbol loading, C calls, pointer handling and resource
disposal. A session owns its context, module and target data and disposes them
in dependency order on success or error. Handles never leave the private import
implementation. The library remains loaded for the importer's lifetime.

## Test mapping

| Original tests | Port verification |
| --- | --- |
| Scalar/CFG, boolean, poison, alloca checks | Rust `scalar_flags_and_boolean_refinement`, `fixture_output_matches_python_reference`, base and bool validators |
| Flags, FP, memory, globals | Existing flags, fp, memory and globals validators through `test_llvm_import_rust.py` |
| Calls, aggregate ABI, fastcc, EH | Existing calls, aggregate, fast and eh validators through the same runner |
| Linkage, ODR and hidden/tagged RTTI | Existing linkage and odr validators, plus all 15 `test_llvm_import_odr.py` tests through `--validator policy` |
| CLI buffering, atomic publication and external-verifier failure | Existing validator rejection/preservation tests run against both CLIs |

The parity runner compares success/failure and successful Keel output byte for
byte, then returns the Rust result to the original validator. Thus the original
native execution checks exercise Rust-imported output. The policy tests compare
in-process Python translation with the Rust CLI and keep the original assertions.
Use a fresh output directory for each validator:

```sh
KEEL_TEST_LIBLLVM="$(rustc --print sysroot)/lib/libLLVM.dylib" \
  cargo test -p keel-llvm
python3 suite/test_llvm_import_rust.py \
  --importer target/debug/keel-llvm-import --validator flags \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel target/debug/keel --out target/llvm-rust-flags
```

Other validator names are `base`, `fp`, `globals`, `calls`, `bool`, `memory`,
`aggregate`, `fast`, `eh`, `linkage` and `odr`. `policy` takes `--libllvm`, optional
`--keel` and `--verify-weak-odr`, without `--out`.

## Deliberate differences

- The Rust API always verifies the resulting Keel module in-process. Python's
  `import_module` returns text and leaves Keel verification to its CLI.
- `--keel` is optional in the Rust CLI. When supplied, it still runs the external
  verifier before publication. The harness uses the API and avoids that process.
- Errors retain their rejection category and context; exact diagnostic wording
  is not an API compatibility promise.
- Rust uses bounded integer arithmetic for sizes and addends. Products are
  checked in u128/i128 before narrowing. LLVM's u32 object/addend bounds remain.
- Diagnostic strings use lossy UTF-8 decoding. Supported symbols still require
  ASCII letters, digits, `_`, `.` or `$`, matching the Python symbol policy.

## Preserved quirks

Overflow flags, GEP constraints and optimizer metadata are omitted rather than
invented as Keel facts. Signed division guards MIN/-1 for masked poison paths.
Boolean i1 values become canonical i8 values. FP constants retain raw bits.
Memcpy becomes memmove to preserve LLVM's identical-pointer case. Fixed-slot
lifetime markers are omitted. ODR copies, hidden RTTI, aggregate parameters and
sret retain the original bounded acceptance checks.

## Verification

The 12 existing validator suites passed against Rust with 605 CLI comparisons
to Python. The 15 policy tests passed with 94 additional import comparisons.
Successful outputs matched byte for byte. Native execution, rejection and
publication checks in those suites passed on Apple arm64 with LLVM 23.1.1.

`cargo test -p keel-llvm` (with `KEEL_TEST_LIBLLVM`) and the full isolated
Cranelift test suite passed. All three input formats passed the execution and
round-trip harness using the in-process importer. Formatting, documentation and
Clippy with `--no-deps --all-targets --all-features -- -D warnings` passed for
both affected crates. The Clippy run including workspace dependencies stopped
on six existing warnings in `keel-ir` (neon, sections, vector docs, aggregate
va_arg verification and rflags regions); those files were not changed.
