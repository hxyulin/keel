# LLVM ODR and runtime-data transport

The importer transports verified LLVM 23 modules targeting little-endian Apple
AArch64. It does not construct C++ ASTs or synthesize vtables/typeinfo. Original
LLVM input bytes are copied into LLVM's parser and never rewritten. All module
functions, including unused ODR definitions, retain their supported raw names.
The CLI buffers the entire translation, requires the real Keel verifier, and
publishes only after verification; existing destinations survive every failure.

## Required core syntax and semantics

LLVM `linkonce_odr` and `weak_odr` **definitions** map to Keel `weak_odr`:

```text
func @_Z1fv() -> ptr cc(c) weak_odr { ... }
data @value = weak_odr align 8 { i64 7 }
data @_ZTS1B = weak_odr hidden const align 1 { i8 49, i8 66, i8 0 }
data @_ZTI1B = weak_odr hidden const align 8 { ptr @_ZTVN10__cxxabiv117__class_type_infoE + 16, rtti_name @_ZTS1B }
```

`hidden` must preserve hidden weak data visibility (Mach-O private extern).
`rtti_name @symbol` must represent precisely `symbol + 0x8000000000000000`,
including the high bit, without passing through the ordinary unsigned-u32
relocation addend. These are explicit dependencies on the parallel core work;
the baseline verifier at a367d868 rejects this syntax. The importer does not
claim that translation-only checks establish native linker/runtime behavior.

The C API linkage constants come from the official
[llvm-c/Core.h LLVMLinkage enum](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Core.h),
not the C++ enum: external 0, linkonce_odr 3, weak_odr 6, internal 8, private 9.
The focused tests also check both ODR values against the installed real C API.

Weak-any, extern-weak, linkonce-any, common, appending, unsupported visibility,
TLS, externally initialized objects, sections, aliases, ifuncs and COMDAT
remain rejected. Available-externally remains rejected by default. The explicit
`--localize-odr-functions` mode retains its bounded direct-call-only local-copy
policy for linkonce_odr/available-externally functions; it can still reject
address-taking, storage dependencies and outside/indirect calls. Weak-ODR
definitions use true ODR linkage even when that legacy flag is present.

## Narrow hidden RTTI and tagged-name eligibility

Hidden globals qualify only as validated Itanium class or single-inheritance
RTTI records (`_ZTI<suffix>`) and the matching `_ZTS<suffix>` byte strings:

- The record is an ODR constant definition, an unpacked struct of two or three
  address-space-zero pointers, with exact size 16/24 and offsets 0/8[/16].
- The first field is the external default-visibility runtime vtable symbol for
  `__class_type_info` or `__si_class_type_info`, respectively, with addend 16.
- The second field refers to its matching ODR constant byte-array definition,
  at least two bytes long and ending in NUL. Its initializer is either the
  direct symbol or exactly the i64 `ptrtoint(symbol) + 2**63` then `inttoptr`
  expression emitted by Apple Clang. Integer add wrap flags are rejected here.
- The third field, when present, is a named `_ZTI*` global reference. That
  object's own linkage/visibility/layout/initializer must also pass validation.
- Only the tagged expression in that recognized record's field at offset 8
  emits `rtti_name`. Reuse in another global or function operand is rejected.

Names alone grant no eligibility. Hidden arbitrary functions/data, malformed
RTTI-shaped records, mismatched type-name symbols and other integer expressions
remain rejected. The importer validates transport, not the C++ runtime's logical
well-formedness of arbitrary user-authored typeinfo.

Every initializer still passes TargetData layout, padding, size and alignment
checks. Ordinary relocations require eight-byte alignment and unsigned-u32
addends, reject function addends and offsets past a defined object's end, and
retain exact symbol identity. External `[0 x ptr]` declarations allocate no
storage and are allowed for runtime vtables; zero-sized definitions still fail.
No bound is invented for external storage. Defined catch typeinfo is accepted
only after the corresponding global definitions have passed transport checks.

## Returned and GEP contracts

The `returned` parameter promise is omitted only for a parameter whose LLVM type
exactly matches the result type. It creates no Keel return/alias optimization
facts. The real LLVM verifier checks placement and consistency; unsupported
types and all other ABI guards still apply to definitions, declarations and
calls. Return-position use and unsupported type conversions are rejected.

GEP `inrange`, like already accepted inbounds/no-wrap promises, constrains
LLVM-defined executions without changing their numeric addresses. Dropping the
promise is a semantic refinement on that domain, including its pointer escape,
comparison and memory-access restrictions; this is not a promise to reproduce
LLVM poison/undefined behavior outside that domain. TargetData-derived GEP
arithmetic is unchanged. Constant GEPs in initializers or instruction operands,
including Clang's vtable `inrange(-16, 8)` forms, retain every existing relocation,
type, address-space and bounds guard. Instruction GEPs retain exact ordinary
arithmetic without inventing alias/escape facts. LLVM itself controls which
flag forms are legal on constants versus instructions.

## Evidence and parent integration

Run focused tests directly (no native work), or with a real baseline verifier:

```sh
python3 suite/test_llvm_import_odr.py --libllvm /path/to/libLLVM.dylib -v
python3 suite/fast_opt_locked.py /tmp/odr-policy-lease.jsonl python3 suite/test_llvm_import_odr.py --libllvm /path/to/libLLVM.dylib --keel /path/to/keel -v
```

Without `--verify-weak-odr`, the CLI tests explicitly require baseline rejection
of both ordinary and hidden/tagged ODR output. Strong defined RTTI is positively
verified, and real CLI file/stdin input preservation and destination/stdout
atomicity are checked. No mock verifier is used. After integrating core syntax:

```sh
python3 suite/fast_opt_locked.py /tmp/keel-odr-integrated-lease.jsonl env CARGO_TARGET_DIR=/tmp/keel-odr-integrated-target cargo build --offline --locked -j2 -p keel
python3 suite/fast_opt_locked.py /tmp/keel-odr-integrated-lease.jsonl python3 suite/test_llvm_import_odr.py --libllvm ~/.rustup/toolchains/stable-aarch64-apple-darwin/lib/libLLVM.dylib --keel /tmp/keel-odr-integrated-target/debug/keel --verify-weak-odr -v
python3 suite/fast_opt_locked.py /tmp/keel-odr-integrated-lease.jsonl python3 suite/llvm_import_odr_validate.py --libllvm ~/.rustup/toolchains/stable-aarch64-apple-darwin/lib/libLLVM.dylib --keel /tmp/keel-odr-integrated-target/debug/keel --out /tmp/keel-odr-integrated-native
python3 suite/frontend_validate.py --out /tmp/keel-odr-integrated-frontend --jobs 2 --require-llvm
```

The mixed-TU validator is supplied by the separate interoperability task.
`frontend_validate.py` owns its lease: do not wrap it in fast_opt_locked.py.
Output directories must be fresh. Parent integration must reconcile these
existing expectations without dropping retained rejection coverage:

- `llvm_import_linkage_validate.py`: default-strict left/right/constexpr now
  accept true ODR definitions; weak-odr is no longer a negative even with the
  legacy flag. Legacy local-copy address/storage/closure rejections remain.
- `llvm_import_eh_validate.py`: defined-rtti becomes positive; weak-any RTTI
  stays negative. Its deliberately synthetic null typeinfo is transportable
  data, not evidence of a functioning C++ runtime typeinfo record.
- `llvm_import_globals_validate.py`: `rejections.json` cases `linkonce` (actually
  linkonce_odr) and `inrange` now accept; all other existing global negatives
  still reject in a real-LLVM translation sweep. External zero-sized
  declarations accept; zero-sized definitions still reject.
- `cpp_coverage.py` and shared docs/harness: virtual/eh/constexpr-runtime probes
  can become positive subject to actual native checks; ordinary hidden symbols
  remain negative. The shared README's blanket inrange rejection is obsolete.

No foundational Rust, shared existing validators, harness files or shared docs
are changed by this importer implementation.
