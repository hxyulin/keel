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

The parity runner compares success/failure and successful Keel output after
removing the function attributes and load/store alignment newly retained by
Rust. It leaves global and stack alignment, volatility and all other text
unchanged. Focused Rust tests check the exact retained facts, and Python tests
bound the normalization. The runner returns the Rust result to the original validator. Thus the original
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
- Rust now preserves load/store alignment and existing function/declaration
  `inlinehint`, `alwaysinline`, `noinline`, `noreturn`, `nounwind`, `cold`, `hot`
  and `optnone` flags. Python remains the earlier baseline without these facts.
  Alignment is independent of volatility and does not infer `notrap`.
  Call-site-only attributes are still omitted, rather than strengthened into
  function-wide promises. LLVM requires `optnone` functions to be `noinline`;
  both flags map to Keel's corresponding optimization policy. Neither this map
  nor hot/cold hints promise the same optimization decisions in both compilers.
  LLVM accepts simultaneous hot/cold hints, while Keel requires them to be
  exclusive. Both contradictory hints are omitted rather than rejecting a
  previously accepted LLVM module.

## Preserved quirks

Overflow flags, GEP constraints and optimizer metadata are omitted rather than
invented as Keel facts. Signed division guards MIN/-1 for masked poison paths.
Boolean i1 values become canonical i8 values. FP constants retain raw bits.
Memcpy becomes memmove to preserve LLVM's identical-pointer case. Fixed-slot
lifetime markers are now retained as explicit pinned intrinsics. ODR copies, hidden RTTI, aggregate parameters and
sret retain the original bounded acceptance checks.

## Original port verification

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

## Fact preservation validation

The full `keel-llvm` test suite passed with `KEEL_TEST_LIBLLVM` set to LLVM 23.
The focused import tests also passed after adding the simultaneous cold/hot
case. They check exact alignment and attribute values, default absence of
promises, call-site isolation and alignment retained through LLVM re-export.
Targeted Clippy with warnings denied, formatting and documentation passed.
The base validator passed 53 Rust/Python comparisons and 18 native builds using
the bounded fact normalization. Both normalization tests passed.

## Contract-preservation follow-up

The Rust importer now retains typed memory summaries, nofree/nosync, bounded
pointer facts and independently scoped call contracts. It preserves contextual
nocallback only for known external targets; LLVM treats the hint as ineffective
on local definitions. Pointer promises that can produce poison without an
established definedness boundary remain omitted. Definition-only parameter
noalias maps to Keel restrict contracts and back to definition parameters.
Full capture components, fresh-allocation return noalias and poison semantics
remain unrepresented.

The parity runner strips only the newly transported contract clauses,
definition restrict clauses and direct fixed-slot lifetime intrinsic statements
when comparing against the earlier Python baseline. It leaves stack/global
alignment, ordinary memory accesses, volatile/notrap properties, symbols and
quoted strings intact. Focused Rust tests assert the retained fields, scope,
conservative region widening, masked-value boundaries and re-export behavior.
The existing native contract fixture checks legal read-only aliases, disjoint
writes, pointer equality and masked overflow/non-null inputs.

The native contract consumer additionally requires Keel-only `preserves_access`;
LLVM import leaves it unknown. A native page-permission regression confirms that
without this promise the later potentially faulting read remains. LLVM export
still has a separately reproduced ordinary-load fault loss, documented in
`docs/llvm-contracts.md`; this port does not establish arbitrary fault parity.
