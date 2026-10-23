# C custom sections and linker retention

Keel supports GNU `section("name")` and no-argument `retain`, including
`__section__` and `__retain__`, on function definitions and
static-storage object definitions (including `_Thread_local` / `__thread`) on both native targets. File-scope objects,
tentative definitions, and static locals are supported. Attributes merge over
the entire translation unit, including declarations following definitions.
Conflicting section redeclarations diagnose. An attributed declaration must
have a definition in the same translation unit. Automatic locals, parameters,
members, typedefs, type positions and COMDAT/`weak_odr` combinations diagnose.
Ordinary `weak` definitions retain their binding and symbol identity.

`section` specifies placement and does not itself force compiler emission.
`used` remains a compiler emission root only, on both targets. `retain` is both
a compiler emission root and a request to prevent system-linker garbage
collection of that definition. It preserves a function's ordinary C calling
convention. Optimization, IR text roundtrips and both native object paths
preserve placement and retention. The compiler does not infer an archive
extraction request from either attribute: a member still needs ordinary symbol
demand or a linker whole-archive option. Weak definitions can still be
superseded by strong definitions. Retention does not strengthen binding.

## ELF x86-64

An explicit name contains 1–128 ASCII letters/digits or `._-$`. Flags, section
types, group arguments, injected assembly and segment syntax are not accepted
in the C string. Safe arbitrary code, writable initialized data, readonly data,
and writable all-zero sections are supported. Code uses `SHF_ALLOC|SHF_EXECINSTR`;
writable data uses `SHF_ALLOC|SHF_WRITE`; readonly bytes use `SHF_ALLOC`.
All-zero writable data uses `SHT_NOBITS`, except conventional `.data`, `.data1`
and `.data.*` placement, which remains `SHT_PROGBITS`; other storage also uses
`SHT_PROGBITS`.
Readonly data containing addresses uses writable relocatable-constant storage,
matching the existing `.data.rel.ro` policy. A custom name does not itself
arrange a system linker's RELRO segment policy.

Section names with standard placement implications (`.text`, `.data`, `.rodata`,
`.bss` and their dotted suffixes) must match the storage class. Runtime arrays,
notes, debug/unwind metadata, dynamic-loader sections, groups and GNU
special sections (including `.gnu.*` and `.gnu_*`) are reserved and rejected. C cannot request merge/string,
COMDAT, link-order, compressed or other special section forms. The
`.keel.retain.` namespace is reserved for compiler-generated names.

Retained input sections add `SHF_GNU_RETAIN` (`0x200000`), spelled `R` in assembly.
The object writer marks GNU OSABI, which GNU ld requires to interpret the
OS-specific flag. Unplaced retained definitions receive separate
`.text/.data/.bss/.rodata/.data.rel.ro.keel.retain.<symbol>` sections so retention
does not accidentally root unrelated definitions in a default input section.
ELF assembly distinguishes retained and ordinary input contributions with the
same name, matching GNU as. Explicitly sharing a retained section retains
that whole retained input section, not
individual bytes or symbols. A compiler-only `used` definition can therefore
survive GC when it shares an input section with a live root; the attribute
itself still makes no linker-retention request.

This first milestone requires all definitions explicitly sharing a named
section in one translation unit to have the same storage class, including
zero-fill versus initialized data, and the same retention request on ELF.
Conflicting requests diagnose before pruning, including unused definitions.
This avoids silently broadening retention or changing section type.
Data alignment must be a power of two up to 4096 bytes; sections preserve the
maximum alignment requested by their members. Function entries keep the
backend's normal alignment.

The native ELF linker accepts and preserves `SHF_GNU_RETAIN` and GNU OSABI in
its executable section headers. Native `--gc-sections` now collects whole input
sections and roots retained contributions; without that option, selected supported
sections survive. See [native GC policy](elf-linker.md#opt-in-section-garbage-collection)
for roots, unwind handling and limits. ELF COMDAT selection is
a separate earlier step: supported assembler-authored groups coalesce before
layout, while C attributes and IR `weak_odr` plus section/retain combinations
continue to diagnose. See [native COMDAT scope](elf-linker.md#comdat-selection-before-native-garbage-collection).

## Darwin arm64

The spelling is exactly `"segment,section"`, with each component containing
1–16 ASCII letters/digits/underscores. Only `__TEXT` and `__DATA` are supported.
Functions require `__TEXT` and emit regular sections with instruction attributes.
Mutable objects and readonly objects containing relocations require `__DATA`;
readonly objects without relocations may use `__TEXT` or `__DATA`. Conventional
`__text`, `__data` and `__const` names must match their storage class. Special
runtime, Objective-C, unwind, literal, stub/pointer and ordinary zero-fill section
forms are rejected. Custom all-zero objects use regular data with actual zero
bytes, rather than claiming arbitrary Darwin zerofill support. The same-class
sharing and data alignment restrictions above also apply.

`retain` emits symbol-level `.no_dead_strip`, represented by `N_NO_DEAD_STRIP`
(`0x20`) in the MH_OBJECT nlist descriptor, preserving `N_WEAK_DEF` (`0x80`)
independently. It does not set `S_ATTR_NO_DEAD_STRIP` on a shared default section.
The object-only descriptor is consumed at link time and is not copied into an
executable nlist. Native `--dead-strip` / `-dead_strip` roots the containing
input section; symbol atomization remains deferred, so unrelated definitions
sharing that section survive together. The option is off by default. See
[native dead stripping](macos-linker.md) for root and unwind policies.

## IR and validation

`FunctionAttrs` records `retain` and an optional typed `SectionName`;
`DataObject` records both separately from its initializer. Function text prints
`retain section "name"` with the function attributes; data text prints it before
`const align`. `SectionName` validates the bounded safe character set; native
codegen and target-specific artifact transport additionally validate placement,
sharing and supported storage. Artifact dialect `KEEL-IR-UNIT 4` preserves the
new metadata and rejects versions 1–3. LLVM and Cranelift exports explicitly
reject custom sections/retention, rather than silently dropping their contracts.
Feature-query macros remain conservative (zero).

Debug compilation units spanning custom function sections or separate retained
ELF code sections use DWARF range lists rather than a subtraction across input
sections. Per-function ranges remain section-local.

`crates/keel-c/tests/sections.rs` checks malformed/late attributes and invalid
placement before pruning, optimization and IR roundtrips, equivalent direct/text
objects on both targets, section flags/types/alignment, retained readonly/data/
zero/code definitions, GNU GC and Darwin dead stripping, weak overrides and
ordinary archive extraction. Darwin custom-section programs execute through
system and native links with debug and optimization on/off. ELF objects and
native images are inspected on the arm64 host; x86-64 execution is not claimed.
Assembler tests compare GNU `as` flags/OSABI and Clang Mach-O nlist descriptors.
IR, artifact and export tests check the typed transport and rejection boundaries.

Primary ABI/producer references checked for this implementation:

- [GNU assembler ELF section flags and types](https://sourceware.org/binutils/docs/as/Section.html),
  including `R`/`SHF_GNU_RETAIN` and `@nobits`.
- [Apple Mach-O nlist definitions](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/nlist.h),
  including the MH_OBJECT-only scope of `N_NO_DEAD_STRIP`, and independent weak bits.
- [Apple Mach-O section definitions](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/loader.h),
  including 16-byte segment/section fields and instruction/section attributes.
- [Clang attribute reference](https://clang.llvm.org/docs/AttributeReference.html#retain),
  including ELF linker support requirements. Keel intentionally provides compiler
  emission for `retain` alone and keeps `used` compiler-only on Darwin as well;
  these are the explicit Keel contracts, not a claim of identical Clang semantics.

Remaining work: broader named-section type merging, arbitrary Mach-O zerofill
and segments, explicit section flags/types, COMDAT placement and retention,
LLVM/Cranelift export, linker GC, selective native retention, DSO lifecycle,
startup policy and general lifecycle priorities. Aliases remain unimplemented.

Validation commands on the macOS arm64 host (GNU cross-binutils installed in
`/opt/homebrew/bin`):

```sh
KEEL_GNU_AS=/opt/homebrew/bin/x86_64-elf-as \
KEEL_TEST_GNU_AS=/opt/homebrew/bin/x86_64-elf-as \
cargo test --no-fail-fast -p keel-c -p keel-ir -p keel-codegen \
  -p keel-asm -p keel-asm-ir -p keel-toolchain -p keel-llvm -p keel-opt -p keel-link
cargo test -p keel-c
cargo test -p keel-c --test sections
cargo test -p keel-asm --test retention --test x86_sections --test x86_elf_directives
cargo test -p keel-ir --test sections -p keel-toolchain --test foundations \
  -p keel-llvm --test export
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked \
  --target-dir target/keel-cranelift
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked \
  --target-dir target/keel-cranelift --test translate
cargo check --workspace
```

The ELF integration tests use `KEEL_TEST_GNU_LD` and `KEEL_TEST_GNU_AR` overrides
when supplied; otherwise they use Homebrew cross-tools on macOS and `ld`/`ar`
on Linux. The assembler oracle accepts `KEEL_TEST_GNU_AS`. Missing required
system tools fail these integration tests rather than silently skipping them.

The broad nine-crate regression run passed 1,883 tests. After the final
conventional-section hardening, the full frontend run passed 230 tests and the
focused assembler/section/IR/artifact/LLVM commands above passed. The separate
Cranelift suite passed 17 tests, with its final translate-only rerun passing 10;
workspace checking also passed. These runs preserve the existing K&R, complex,
cleanup, VM/VLA and lifecycle regressions. No x86-64 execution on the arm64 host,
LLVM/Cranelift retention export, or selective native-linker GC is claimed.

## Independent integration review

The combined C registration-table regression in `crates/keel-c/tests/sections.rs`
compiles three independent translation units and links them in interleaved order.
It covers a late-attributed static record, a retained static-local record in an
uncalled function, an asm-renamed weak callback, a constructor in custom code,
32-byte record alignment, function/data pointer relocations, and separate
unplaced retained data outside the table. Both object routes run with debug and
optimization independently enabled/disabled. Darwin executes the iterator and
constructor through both native and Apple dead-strip links. ELF native and GNU
GC images are inspected for exact bounds and relocated record contents; the
GNU link uses `-z start-stop-gc` to ensure boundary references do not implicitly
retain the table. No x86-64 execution is claimed.

Darwin boundary names (`section$start$...` / `section$end$...`) are raw assembler
labels without a C ABI underscore. The current C declaration asm-label grammar
requires that underscore, so this test obtains their addresses with supported
inline assembly. Direct C declaration aliases for these raw Darwin labels
remain outside the supported grammar. The 32-byte record size matches the
strongest input alignment: section bounds include inter-input padding, and
bounds alone do not turn a table with alignment holes into a dense C array.

Sharing diagnostics for C storage classes apply within a translation unit.
Native ELF identifier-named registration groups additionally reject mismatched
input types or flags across translation units, including zero-fill versus
initialized data, readonly versus writable storage, code versus data, and
retention mismatches; the review adds C-generated regressions for both object
routes. Dotted ELF input sections keep separate identities. Mach-O regular
sections do not encode every original C storage distinction, such as mutable
versus relocatable-constant data in `__DATA`, or custom zero versus initialized
data. Object linking cannot reconstruct those distinctions to enforce the
frontend's complete storage-class policy across translation units.

Review of attribute merging, pruning, static locals, weak/asm identities,
section flags, debug ranges, imports/lifecycle roots, and typed/text/artifact
transport found one reserved-name validation gap: `.gnu.*` was reserved, but
`.gnu_debuglink`, `.gnu_debugaltlink` and `.gnu_debugdata` were accepted as
ordinary custom storage. The shared ELF validator now reserves `.gnu_*` as
well, with frontend-before-pruning and direct-IR regressions. These names have
special debug metadata consumers; see the [GDB separate-debug-file format](https://www.sourceware.org/gdb/current/onlinedocs/gdb.html/Separate-Debug-Files.html)
and [GNU binutils debug-section handling](https://www.sourceware.org/binutils/docs/binutils.html).
No further confirmed section/retain semantic defect remained. Another new
regression checks that validating discarded sectioned pointer/string
initializers does not emit their otherwise unused referents.

The independent Cranelift check initially failed to compile because its type
and instruction matches predated the F80 IR additions. Explicit unsupported-F80
diagnostics restore the diagnostic exporter build without changing F80 native
semantics or adding Cranelift support. Its full suite then passed 18 tests,
including section/retention rejection and the new F80 rejection coverage.

Independent validation on macOS arm64 passed the six-crate
`keel-c/keel-ir/keel-asm/keel-toolchain/keel-llvm/keel-link` batch (894 tests)
and the `keel-codegen/keel-opt/keel-asm-ir` batch (1,029 tests). After the final
reserved-name fix, the complete C/IR rerun passed 318 tests (233 C and 85 IR);
the focused section/IR/artifact/LLVM rerun passed, and the final artifact suite
passed all nine tests. Workspace checking and `git diff --check` passed.
`xcrun dwarfdump --verify` reported no errors for the generated ELF registration
objects and ELF/Mach-O custom-section objects; it still reports empty-child-DIE
warnings from the existing debug emitter. These general debug warnings were
not changed by the section/retain review.

## TLS custom placement and retention milestone

TLS uses the existing `DataObject.thread_local`, `section`, `retain` and
initializer fields together; no artifact dialect bump is needed. C declarations
merge attributes across the translation unit, including after definitions.
Every declaration of a given object must agree on TLS storage. Block-scope TLS
requires `static` or `extern`; functions cannot have TLS storage. Attributed TLS
imports without a definition in the same TU, weak TLS, and TLS COMDAT remain
diagnosed. An ordinary TLS import remains supported by object emission.

`section` always places **template storage**, never the public Darwin TLV
descriptor. The initializer determines initialized versus zero-fill placement,
including an explicitly initialized all-zero object. Named TLS sections cannot
share with ordinary storage, or mix initialized and zero-fill templates in a TU.
The earlier same-section ELF retention consistency rule also applies. The
existing safe-name syntax, reserved metadata namespaces and alignment limits
remain in force. Conventional `.tdata` / `.tdata.*` require nonzero initialized
TLS, and `.tbss` / `.tbss.*` require all-zero TLS; ordinary `.data`, `.bss`,
`.text` and `.rodata` names reject TLS. Arbitrary safe ELF names are supported.

ELF emits `SHF_ALLOC | SHF_WRITE | SHF_TLS` (`0x403`) with `SHT_PROGBITS` for
initialized templates and `SHT_NOBITS` for zero-fill. Retention adds
`SHF_GNU_RETAIN`, keeping the existing GNU OSABI convention. Symbols remain
`STT_TLS` with their original identity, size and alignment. Unplaced retained
TLS uses separate `.tdata.keel.retain.<symbol>` or
`.tbss.keel.retain.<symbol>` input sections. Defined accesses remain local-exec
`R_X86_64_TPOFF32`; imported accesses remain initial-exec
`R_X86_64_GOTTPOFF`. This contract targets executable TLS, not general-dynamic
TLS or arbitrary shared-library placement.

The native ELF linker still rejects TLS. Both defined and undefined `STT_TLS`
symbols, and allocated `SHF_TLS` storage without symbols, receive a diagnostic
requiring external ELF linking: native `PT_TLS` layout and TLS relocation
processing are unsupported. Accepting the section flag alone would omit the
loader's TLS image and misresolve thread-relative accesses. No bounded native
path was added because existing executable layout, GOT imports and relocation
processing do not implement those contracts together. Actual GNU system-link
regressions resolve both relocation forms, produce a `PT_TLS` program header,
exercise `--gc-sections`, and diagnose TLS/non-TLS cross-TU references. These
x86-64 images are inspected on the arm64 host; execution is not claimed.

Darwin TLS templates require `__DATA`. Arbitrary safe names receive
`S_THREAD_LOCAL_REGULAR` (`0x11`) or `S_THREAD_LOCAL_ZEROFILL` (`0x12`);
custom zero-fill consumes virtual size without file bytes. Conventional
`__thread_data` and `__thread_bss` require their corresponding template class.
Other `__thread*` names, including descriptor/pointer/init-function sections,
remain reserved. Ordinary `__data`, `__const` and `__text` cannot host TLS.
The public symbol stays in compiler-owned `__DATA,__thread_vars`
(`S_THREAD_LOCAL_VARIABLES`, `0x13`), containing the bootstrap pointer, key and
relocation to the original `<symbol>$tlv$init` template. Accesses still use
TLVP relocations and the descriptor getter. Retain marks **both** descriptor
and template symbols `N_NO_DEAD_STRIP`; `used` remains the existing compiler-only
emission root. System `-dead_strip` tests prove an unreferenced retained pair
survives and a separate compiler-only TLS object disappears. Native Darwin
linking already lays TLS template sections contiguously, computes descriptor
offsets and preserves alignment relative to the template base. Native dead
stripping now preserves live descriptor/template pairs through its section graph.

Requested TLS template alignment is preserved in objects and linked template
layout. Darwin runtime over-alignment above the allocator's ordinary guarantee
is an existing limitation affecting Clang too: dyld allocates per-thread copies
with malloc/calloc and adds descriptor offsets. Object section alignment cannot
force allocation-base alignment. The first TLS32/64 execution fixture reproduced
this boundary; it was not treated as a passing alignment test. See the existing
[`tools/tls-alignment` investigation](../tools/tls-alignment/README.md).
Object tests retain 32/64-byte requests; passing execution tests use 8/16-byte
alignment and assert addresses, initialization, repeated mutation, static locals,
Clang-to-Keel cross-TU access and isolation across four concurrent pthreads plus
the main thread. Debug/optimization independently on/off and direct/text object
routes execute through Apple dead-strip and native links. An independent Clang
producer at `-O0`/`-O2` also executes through both linkers with the same harness.
Clang requires explicit `thread_local_regular` / `thread_local_zerofill` in its
custom C section spelling; bare arbitrary names produce regular template
storage and can fail system linking. Keel derives the TLS section types itself.
Keel's late-attribute merging and compiler-only `used` contract are intentional
and are not claims of identical Clang behavior.

Cross-TU Darwin TLVP references to ordinary definitions are rejected by the
native linker. The installed Apple linker accepts that incompatible combination;
the regression records acceptance without executing its invalid getter call.
Mach-O nlists do not encode a TLS type on undefined symbols, so an
ordinary non-TLS address reference to a descriptor cannot in general be
recognized as an incompatible C declaration; that direction still requires
consistent source declarations. Neither format carries all frontend
same-section class distinctions between TUs. Artifact encode/decode repeats
target placement validation; LLVM and Cranelift reject the combined TLS/custom
section/retain contract explicitly. Ordinary TLS, weak, bounds, lifecycle,
cleanup and K&R regression suites remain part of broad validation.

Primary ABI details checked for TLS:

- [ELF TLS ABI, sections 2–4 and x86-64 access models](https://www.akkadia.org/drepper/tls.pdf):
  TLS section types/flags, `STT_TLS`, `PT_TLS`, thread-relative values and local/
  initial-exec relocation requirements.
- [Apple Mach-O section type definitions](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/loader.h):
  distinct initialized template, zero-fill template and descriptor section types.
- [Apple dyld TLV implementation](https://github.com/apple-oss-distributions/dyld/blob/main/libdyld/ThreadLocalVariables.cpp):
  `findInitialContent` consumes contiguous sections by **type**,
  `initializeThunksFromDisk` uses template-relative offsets, and per-thread
  instantiation allocates and copies the template. This explains why arbitrary
  regular sections cannot implement named TLS storage.

Focused validation:

```sh
cargo test -p keel-c --test sections
cargo test -p keel-ir --test sections -p keel-toolchain --test foundations \
  -p keel-llvm --test export
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked \
  --target-dir target/keel-cranelift
```

No native ELF TLS, native dead stripping, TLS weak/COMDAT, over-aligned Darwin
runtime guarantee, general-dynamic TLS or exporter support is claimed.

Milestone evidence on macOS 27.0.1 (26A434), Apple Clang 21.0.0
(clang-2100.3.34.2), GNU cross-binutils 2.47.20260726: the final focused
C section/cleanup, IR section, artifact and LLVM exporter batch passed 52 tests;
the separate Cranelift suite passed 18 tests; workspace checking passed.
The broad regression caught a cleanup diagnostic-precedence regression in the
new automatic-TLS check. Moving that check after the existing TLS/cleanup
rejection preserves the original diagnostic; the entire cleanup and section
suites pass after the correction. TLS template pointer initializers also run
through both linkers, preserving relocations to ordinary static storage.
The final placement/IR/artifact rerun passed 30 tests after tightening Darwin
reserved-section diagnostics, and `dwarfdump --verify` found no errors in the
TLS Mach-O and ELF debug objects. Retained fixture objects and system ELF TLS
images are written below `target/tmp/tls-arm64` and `target/tmp/tls-elf` by the
focused tests; the expected incompatible ELF link uses a separate output path.

The completed nine-crate broad run recorded 1,936 passing tests and two failing
tests: the cleanup precedence issue above and the old ELF test's expectation
of the generic `unsupported flags` TLS error. The ELF test now requires the
precise external-linking / `PT_TLS` boundary. Both initially failing targets
were rerun in full after correction: all 18 cleanup and ELF-link tests passed.
The final focused runs cover changes made while the broad run was in progress;
no full clean rerun of all nine crates is claimed after those corrections.

### Independent TLS review with native COMDAT integrated

Review of the integrated TLS milestone (`8e811314` / `963a2ec1`) on top of
native COMDAT selection (`33374922`) found and fixed two issues:

- Darwin's generated `<symbol>$tlv$init` template label could collide with a
  user asm label. Definitions failed with a duplicate label; imports could bind
  to the private template instead of remaining external. The producer now
  appends `$` until the template name avoids all declared data and function
  symbols. The descriptor keeps its public identity, and its template relocation
  and retention directive use the chosen private label. Regressions occupy two
  successive candidate names, preserve matching data/function imports, compare
  both object routes, and execute through Apple dead stripping and native
  linking with debug and optimization independently on/off.
- Native ELF rejected `STT_TLS` symbols throughout selected input objects, but
  checked allocated `SHF_TLS` only after discarding losing COMDAT sections.
  Symbol-less TLS in a losing group therefore escaped the stated diagnostic
  boundary. Both checks now apply to the whole selected object. Tests require
  the external-linking / `PT_TLS` diagnostic for initialized and zero-fill TLS,
  with and without symbols or retention, in both winning and losing groups.
  An undefined TLS import referenced only from a discarded group also remains
  rejected. An unextracted TLS archive member remains outside this contract;
  archive extraction policy and ordinary COMDAT selection are unchanged.

Additional coverage checks const/volatile TLS with late placement, retention
and alignment declarations. The ARM64 multithread fixture now combines TLS
with a constructor, destructor and cleanup callback on every iteration; teardown
checks the main thread's final TLS state. All 20 producer/linker combinations
execute with four worker threads and the main thread. Clang uses
`-fno-use-cxa-atexit` for this combined fixture to match Keel's executable
`atexit` teardown policy. Its default destructor thunk still needs the unsupported
`___dso_handle`, as documented in [C lifecycle](c-lifecycle.md); this review does
not extend DSO lifecycle support.

DWARF inspection confirms descriptor-address plus `DW_OP_form_tls_address` on
Darwin and `R_X86_64_DTPOFF64` plus `DW_OP_GNU_push_tls_address` on ELF. Verification
of ELF, Darwin TLS and the generated-name collision objects reports no errors;
the existing empty-child-DIE warnings remain. Review also covered section-class
sharing, template initialization/alignment, retained descriptor/template pairs,
late attributes before pruning, typed/text/artifact transport and explicit
LLVM/Cranelift exporter rejection. No additional confirmed defect in those
contracts was found.

Limits remain native ELF TLS images/relocations, TLS weak/COMDAT/export,
over-aligned Darwin runtime allocation, general-dynamic TLS, x86 execution on
this ARM64 host, and native GC/dead stripping. The COMDAT checks here cover TLS
interaction and preservation of the existing selection/archive/unwind tests;
they are not a separate full COMDAT audit.

Final review validation on the macOS ARM64 host:

```sh
KEEL_GNU_AS=/opt/homebrew/bin/x86_64-elf-as \
KEEL_TEST_GNU_AS=/opt/homebrew/bin/x86_64-elf-as \
cargo test --no-fail-fast -p keel-c -p keel-ir -p keel-codegen \
  -p keel-asm -p keel-asm-ir -p keel-toolchain -p keel-llvm -p keel-opt -p keel-link
cargo test -p keel-c
cargo test -p keel-link
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked \
  --target-dir target/keel-cranelift
cargo check --workspace
xcrun dwarfdump --verify target/tmp/tls-arm64/input.o \
  target/tmp/tls-elf/input.o target/tmp/tls-label-collision/input.o
git diff --check
```

The complete final nine-crate run passed **1,968 tests**, with zero failures and
two pre-existing ignored optimizer tests: `native_clang_differential` requires
a built small-memory binary and exclusive native lease;
`retained_quickjs_ss493_exact_reload_activation` requires a sealed optimized
QuickJS profile. Neither is counted as passing. This run includes all 240 C
tests (19 section tests), all 163 linker tests (21 COMDAT tests), IR placement,
artifact and LLVM export coverage. The separately completed full C and linker
runs passed the same 240 and 163 tests. Cranelift passed 18 tests, workspace
checking passed, and DWARF verification reported no errors in all three objects.
The complete broad run was performed after the final production fixes and
regressions, including preservation of the existing malformed-unwind diagnostic
precedence.
