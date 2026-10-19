# Native macOS arm64 linker

`keel link input.o ... -o program` is an opt-in native link path. Existing
`keel cc`/`compile` output behavior and the suite's Clang link path remain intact.
`keel link --system ...` explicitly selects the Clang driver fallback. Native
errors never silently switch linkers.

The reusable `keel-link` crate takes named object/archive bytes and explicit
`Dylib` interfaces. `link` returns a **not yet signed** Mach-O image and warnings;
`write_signed` writes a sibling temporary file, runs `/usr/bin/codesign` with an
ad-hoc identity, and installs the executable only after signing succeeds.
It does not invoke Clang, ld, or lld. macOS signing is an explicit platform tool
dependency, not a linker fallback. The in-memory API can run on another host,
but those bytes must be signed on macOS before execution on Apple Silicon.

## Source audit and decisions

Audit baseline: the compiler/assembler sources on `feature/macos-linker`.
Runtime and object-tool evidence is recorded below. This first section describes
the producer contract and the implementation decisions.

* `keel-asm/src/macho.rs` serializes thin little-endian `MH_OBJECT`, arm64 subtype
  ALL, with unnamed segment sections, `LC_SYMTAB`, `LC_DYSYMTAB`,
  `LC_BUILD_VERSION`, optional `LC_LINKER_OPTIMIZATION_HINT`, and
  `MH_SUBSECTIONS_VIA_SYMBOLS`. It groups local, external defined, and undefined
  symbols, and converts some temporary local pointer targets to section ordinals.
* Relocations emitted: UNSIGNED (64-bit pointers; 32/64-bit differences paired
  with SUBTRACTOR), BRANCH26, PAGE21/PAGEOFF12, GOT_LOAD_PAGE21/PAGEOFF12,
  POINTER_TO_GOT, TLVP_PAGE21/PAGEOFF12, and signed 24-bit ADDEND prefix pairs.
  The native linker implements these forms with range/alignment checks; TLVP
  accesses use eager GOT slots containing descriptor addresses.
  Local UNSIGNED section references preserve their object-address addends.
  POINTER_TO_GOT fields are replaced, without treating embedded bytes as an
  addend: Apple's assembler can leave nonzero placeholders in these words.
  This follows [Apple's relocation handling](https://github.com/apple-oss-distributions/ld64/blob/main/src/ld/parsers/macho_relocatable_file.cpp)
  and is covered by both a placeholder regression test and cleanup propagation
  into a Clang-compiled C++ catch handler.
* `keel-codegen/src/data.rs` emits text constants, writable data and pointer
  constants, BSS, and Darwin thread-local descriptors. TLS templates and Darwin
  descriptors are supported as described below. Commons are coalesced by maximum
  size/alignment; strong definitions override tentative commons. Duplicate strong definitions are errors.
* `keel-asm/src/parser/debug.rs` selects ARM64 compact encodings when possible;
  other CFI uses DWARF-mode compact entries plus `__eh_frame` with explicit
  pointer-difference relocations. Runtime unwind is needed even for plain C
  backtraces. The linker builds a version-1 `__unwind_info` with regular pages,
  explicit no-unwind gaps, three-slot personality table pointing at GOT slots,
  LSDA indexes, and DWARF FDE section offsets. Whole sections retain CIE/FDE
  relative layout. Foreign implicit FDE fixups are rejected.
* Keel IR supports invoke/landingpad and emits `__gcc_except_tab`, personality
  and typeinfo references. Those references must resolve through selected dylib
  interfaces, e.g. `-lc++`; language-level C++ compilation is not implied.
* `__DWARF` stays in retained object snapshots when explicitly requested with
  `--debug-map DIR` or `--dsym PATH`. Native STABS debug-map records map object
  addresses to executable addresses. Without either option, DWARF omission still
  produces a warning. Runtime unwind sections are always handled independently.
* Lua, QuickJS and SQLite suite manifests use libm and pthread interfaces (plus
  Lua's macOS POSIX/dlopen interfaces). macOS aliases these to libSystem. They
  need libc data imports as well as function calls, and real pointer rebasing;
  resolving only branch imports would be insufficient.

## Supported output design

* One thin `MH_EXECUTE`, arm64 ALL, PIE/two-level namespace, base 4 GiB, 16 KiB
  segment pages, inaccessible `__PAGEZERO`, RX `__TEXT`, RW `__DATA`, and R
  `__LINKEDIT`. File-backed data precedes zero-fill data. Section alignment up to
  64 KiB is retained. The basic implementation limits individual/merged sections
  to 512 MiB and diagnoses branch ranges rather than creating branch islands.
* `LC_MAIN` enters `_main` using dyld's normal libSystem startup and return
  convention. No synthetic crt trampoline or raw kernel entry is used.
  `S_MOD_INIT_FUNC_POINTERS` sections are retained for dyld initializers. The
  initializer fixture uses an Apple-assembled section because Keel's assembler
  does not expose this section kind yet.
* Imports use eager non-lazy binding, one GOT slot per referenced symbol and
  12-byte `ADRP/LDR/BR x16` stubs for external calls. Both ordinary imported
  pointers and local PIE pointers are represented in dyld bind/rebase streams.
  Indirect symbol tables describe GOT and stubs. Exports use a deterministic
  trie and external definitions, allowing dynamic symbol lookup by applications.
* This first writer deliberately uses `LC_DYLD_INFO_ONLY` opcode streams rather
  than `LC_DYLD_CHAINED_FIXUPS`. Modern load commands include `LC_BUILD_VERSION`
  and `LC_MAIN`, plus a content-derived SHA-256 UUID in `LC_UUID`; `codesign` adds
  `LC_CODE_SIGNATURE`. The current dyld aborted an early prototype without
  `LC_UUID`, so this is validated as a launch requirement. Chained fixups,
  authenticated pointers, arm64e output, and function-start/data-in-code synthesis
  remain gaps.
  Acceptance by the current OS must be established by executing signed outputs,
  not inferred from format inspection alone.
* SDK discovery: `--sysroot`, `SDKROOT`, then `xcrun --sdk macosx --show-sdk-path`.
  Libraries use `usr/lib/libNAME.tbd`. Only TAPI v4 is accepted. YAML is parsed
  structurally; only arm64-macos/compatible arm64e-macos groups are admitted,
  and embedded or external reexported libraries are traversed. Runtime paths
  come from install names. Alias interfaces are deduplicated. Unknown imports
  fail before writing output, rather than binding arbitrary names at runtime.
  This is a bounded interface reader: TAPI availability directives, Objective-C
  synthesis, and deployment-conditioned symbol substitutions are not implemented.
* BSD/Darwin archives use demand extraction to a fixed point, ignoring the
  archive symbol index. All parsed members must meet the basic object contract;
  thin/GNU long-name archives, LTO/bitcode and universal archives require the
  system linker. Extraction is whole-member, without atom splitting/dead strip.

## TLS, weak symbols, and source debugging

Darwin TLS supports `S_THREAD_LOCAL_REGULAR` data, `S_THREAD_LOCAL_ZEROFILL`
BSS, and 24-byte `S_THREAD_LOCAL_VARIABLES` descriptors, including compiler
and Clang objects and demand-extracted archive members. Initialized template
sections precede TLS BSS in a contiguous VM range. Descriptor bootstrap pointers
bind to libSystem's `__tlv_bootstrap`; the third descriptor word is relocated to
a template-relative byte offset, with no pointer rebase. Descriptor sections
are aligned to eight bytes even when a Clang input advertises byte alignment.
`MH_HAS_TLV_DESCRIPTORS` asks dyld to initialize descriptor keys/thunks. TLVP
page relocations address eager GOT slots; local descriptor pointers rebase and
dylib descriptor imports bind normally. Export trie terminals carry the
thread-local kind, and TAPI `thread-local-symbols` are included in interfaces.
There is no instruction relaxation or synthesized TLS runtime.

Tests exercise independent initial values and mutations in the main thread and
two actual pthreads, repeated accesses, multiple objects, TLS BSS, archive
extraction, imported TLS from a dylib, and Clang C++ guarded dynamic TLS
initialization. Template alignment is preserved; runtime allocation follows the
OS's dyld TLV implementation. No extra guarantee for over-aligned per-thread
allocation is made. Legacy `S_THREAD_LOCAL_INIT_FUNCTION_POINTERS` is rejected:
on the tested OS Apple ld changes this section to ordinary data, and neither
Apple-linked nor native-linked probes called its callback. Use modern compiler
guard/wrapper initialization instead.

External section-based `N_WEAK_DEF` symbols coalesce to the first weak
contribution. Any strong definition overrides weak contributions, independently
of object order. References through external symbols resolve to that winner;
losing sections remain allocated because this linker does not split/discard
atoms. Private external weak definitions coalesce only statically. Legacy
coalesced sections, weak local/absolute definitions, and aliases/resolvers remain
precise errors. Ordinary strong duplicate definitions remain errors.

Weak dylib export identity is retained from TAPI rather than flattened away.
A strong interface is selected over a weak interface when both provide an
undefined symbol. Weak exports carry the export-trie flag and `N_WEAK_DEF`;
coalescible calls go through stubs/GOT, and pointer slots participate in sorted
dyld weak-binding streams. Strong executable overrides have
`NON_WEAK_DEFINITION` records and `MH_WEAK_DEFINES`; actual weak binding locations
set `MH_BINDS_TO_WEAK`. Direct page references that cannot participate safely in
dynamic coalescing are diagnosed. Available weak imports use their dylib ordinal
and the weak-import bind flag. A symbol referenced **only** with `N_WEAK_REF`
can be absent from all selected interfaces: explicit weak flat lookup then lets
dyld write null when no loaded image supplies it. Any strong reference still
requires a definition/interface. Apple's equivalent absent-name test needs
`-Wl,-U,_absent`. Dynamic behavior follows dyld's image/namespace rules; a strong
symbol in an arbitrary later dylib does not imply universal interposition.
Function/data values and pointer identity are compared with Apple-linked output,
including executable weak/strong/private definitions and competing dylibs.

Source debugging is explicit:

```sh
keel link input.o libhelpers.a --debug-map /absolute/path/program.objects -o program
# LLDB can use the retained objects directly; keep this directory in place.
keel link input.o libhelpers.a --dsym program.dSYM -o program
# --dsym invokes /usr/bin/dsymutil explicitly after signing.
lldb program
```

The host-independent API uses `Options.debug_directory: Option<PathBuf>` (an
absolute UTF-8 directory). `link` returns `Executable.debug_objects`, each with
its required output path and exact object bytes. It performs no filesystem I/O.
`write_signed` materializes these content-named snapshots, then signs/installs the
executable. Callers writing image bytes themselves must also materialize those
artifacts before using a debug map or invoking dsymutil. `Input.name` is only a
diagnostic label: the implementation never opens it, including for archive
members. No persistent original object is required. Use a distinct debug
directory per executable. `--dsym` defaults the snapshot directory to the output
path with extension `debug-objects`; `--debug-map` overrides it. Successful dSYM
construction allows removing snapshots; a debug-map-only executable still needs
them. Snapshot files may remain after a failed signing/dsymutil operation.

Native `N_SO`, `N_OSO`, `N_FUN`, `N_GSYM`, and `N_STSYM` records describe selected
function/data addresses and function extents. dsymutil handles DWARF relocation
and dSYM construction; this is an explicit platform-tool dependency, never a
call to Apple's linker. codesign remains the explicit signer. Real LLDB tests
on unoptimized Clang and Keel `-g` C objects, including multiple compilation
units, static functions, archived debug input, and input bytes whose original
object has been deleted, prove source-line breakpoints, argument/local values,
and backtraces through callers. They pass using the debug map alone and using a
standalone dSYM after deleting snapshots. `dwarfdump --verify` passes. Optimized
variable recovery, split DWARF, debug type archives, Swift/Objective-C debugging,
and arbitrary producer DWARF extensions are outside this tested contract.
Runtime unwind/LSDA and C++ exception support are preserved.

The implementation follows the SDK headers and primary sources:
[dyld TLV initialization and allocation](https://github.com/apple-oss-distributions/dyld/blob/main/libdyld/ThreadLocalVariables.cpp),
[LLVM Darwin debug-map emission](https://github.com/llvm/llvm-project/blob/main/lld/MachO/SyntheticSections.cpp),
and [Apple nlist weak-symbol definitions](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/nlist.h).

## Remaining boundaries

No general ld64 compatibility claim: no dylib/bundle/static executable output,
framework search, arbitrary dylib parsing, rpaths, custom entry points, legacy
`S_COALESCED` sections (except existing `__eh_frame` handling), local/absolute weak
definitions, symbol aliases/resolvers, general interposition, Objective-C metadata
linking, legacy TLS initializer-pointer sections, arm64e/PAC, chained fixups,
dead stripping, ICF, relaxation, LOH optimization, branch islands, or
linker-script/order-file support.
Unsupported options/forms are diagnosed. LOH hints can be ignored safely because
this implementation does not relax their instruction sequences. Data-in-code
payloads are rejected. Foreign implicit `__eh_frame` processing requires the
system linker. Deployment versions are raised to the greatest input minimum.

## Format references

The implementation follows the installed SDK's `mach-o/loader.h`, `nlist.h`,
`arm64/reloc.h`, and `compact_unwind_encoding.h`. Primary upstream references:
[Apple ld64 design](https://github.com/apple-oss-distributions/ld64/blob/main/doc/design/linker.html),
[Apple dyld sources](https://github.com/apple-oss-distributions/dyld), and
[Apple chained-fixup definitions](https://github.com/apple-oss-distributions/dyld/blob/main/include/mach-o/fixup-chains.h).

## Validation

Validated on MacHost, Apple M5 Max, macOS 27.0.1 (26A434), macOS 27.0 SDK,
2026-10-15. All builds and tests started after the parent opened
`/tmp/keel-compilation-profile-256a6e5-done`. No compiler behavior was changed.
Only this OS was tested: an older minimum in `LC_BUILD_VERSION` is not evidence
of compatibility with older macOS releases.

Machine-readable results and object audit
record both modes, symbol/section/load-command forms, relocation counts, input
undefined symbols, and executable sizes. The 44 application inputs were compiled
with this branch's `keel cc --emit-object`, both plain and `-O`. Each set linked
natively and separately with Apple Clang/ld using **identical object files**.
Both links passed Lua 5.4.7's portable `all.lua` suite, all nine configured QuickJS
2024-01-13 tests (including workers/BigNum/qjscalc), and SQLite 3.46.1's configured
query suite. SQLite stdout matched Apple's link exactly. This is the repository's
SQLite query suite, not SQLite's entire upstream TCL/TH3 testing infrastructure.

The audit found 6,136 compact-unwind input records per mode: optimized output has
1,770 frame-mode records and 4,366 DWARF-mode records; plain output has 1,852 and
4,284 respectively. The application objects exercise external branch/page/GOT
relocations, local section-pointer relocations, and native-width pointer
differences. Additional fixtures exercise ADDEND pairs (including page crossing
and negative addends), 32-bit differences, POINTER_TO_GOT personality/typeinfo
references, and compact personality/LSDA tables.

The fixture harness compiles simple C and Keel IR to native objects and assembles
explicit relocation/compact-unwind fixtures with Keel. It compares native and
Apple-linked behavior over five native launches each: mutable/BSS data, local and
imported function/data pointers, malloc/libc calls, argc/argv, exported-symbol
`dlsym`, BSD archive extraction, pointer addends, C backtraces, Keel-generated C++
catch/cleanup/resume metadata, propagation across ordinary calls, and compact exception handling restoring integer and FP
registers. A separate startup test confirms dyld runs an initializer before main.
An ASLR fixture observed five distinct main addresses in five launches. The
initial debug probe only exercised execution with a DWARF-omission warning;
the compatibility validation below supersedes that debugging boundary. Explicit
`--system` fallback was exercised with an extra `-Wl` option.

`codesign --verify --strict`, `otool -l`, `nm -nm`,
`dyld_info -imports -exports -fixups`, and `dwarfdump --eh-frame` inspected the
fixture outputs. LLVM `llvm-objdump --macho --unwind-info` independently decoded
the generated runtime indexes, including compact personalities and LSDA indexes.
Application outputs passed signing verification and load-command/symbol/dyld
inspection as well.

`cargo test -p keel-link -p keel` passed (12 linker tests and 22 driver tests).
`cargo clippy -p keel-link -p keel --all-targets --no-deps -- -D warnings`,
`cargo fmt --all -- --check`, and `git diff --check` passed. A broader strict
Clippy invocation including dependencies initially stopped on existing
`keel-asm-ir` style lints; those unrelated files were not changed.

Reproduce with a release `keel`, and the suite cache populated by the ordinary
suite runner (the cache is only read; new artifacts use a separate directory):

```sh
cargo build -p keel --release
cargo test -p keel-link -p keel
python3 suite/validate_native_linker_fixtures.py --keel target/release/keel --out /tmp/native-fixtures
python3 suite/validate_native_linker.py --keel target/release/keel --cached-root /path/to/target/suite --out /tmp/native-optimized --compile
python3 suite/validate_native_linker.py --keel target/release/keel --cached-root /path/to/target/suite --out /tmp/native-plain --compile --plain
python3 suite/audit_native_linker_objects.py /tmp/native-optimized
```

Omit `--compile` to assemble the cache's Keel `.s` files instead. Both validation
scripts accept an optional `--profile-gate PATH` for coordination with profiling.
The initial writer prioritizes correctness and diagnostics: uncompressed exports
and unwind tables, whole-section retention, eager binding and generous header
padding produce larger executables than Apple's linker. No link-speed or startup
performance claim is made.

## Compatibility validation — 2026-10-15

The separate TLS/weak/debug report
records the new differential probes and regression suites. This work started
from main `426d6fc` in an isolated MacHost worktree. macOS 27.0.1 and the macOS 27.0
SDK are the only tested platform versions. All runtime comparisons use identical
objects with native and Apple links and signed executable launches; test dylibs
are built explicitly with Apple Clang because native dylib output is out of scope.
No link-speed benchmark or older-OS compatibility claim is added.

All Lua, QuickJS, and SQLite suites passed again in plain and optimized modes
from freshly emitted native objects (44 per mode), with fixture trees copied to
the isolated output directory. Existing pointer, addend, archive, backtrace,
exception, compact-unwind and ASLR fixtures passed. `cargo test -p keel-link -p
keel` passed 40 tests (16 linker, 24 driver); strict Clippy with `--no-deps`,
formatting and diff checks passed. No x86 implementation or shared benchmark
report was modified.

```sh
cargo build -p keel --release
python3 suite/validate_native_linker_compat.py --keel target/release/keel --out target/linker-compat/compat
```

The existing regression/application commands above apply unchanged; the new
validator creates its own SDK-interface overlay for test dylibs, retained debug
objects, debugger transcripts, and JSON results inside `--out`.


### Independent review fixes

Review of `b313ac1` found two correctness issues that the initial probes did not
cover. The first TLS template section now uses the greatest template-section
alignment, preserving aligned offsets when initialized data needs less alignment
than TLS BSS. New Clang/Keel probes check actual 4-, 8-, and 16-byte addresses in
main and a fresh pthread, as well as independent values. Allocation guarantees
beyond the OS TLV allocator's alignment remain outside this contract.

Debug maps now include tentative `-fcommon` globals at their final resolved
addresses, including commons overridden by strong definitions. LLDB checks
recover `(int) global = 42` both from debug maps alone and from standalone dSYMs
after deleting object snapshots; `dwarfdump --verify` passes.

The expanded compatibility validator passes 33 cases and 150 signed native
launches, plus two additional LLDB launches of common-global debug maps. The
40 Rust tests and strict driver/linker Clippy checks pass. The report's
`review_followup` section records the new results and debugger transcripts,
while retaining the original application/regression evidence separately.

## Memory ownership

The byte-returning linker API retains input section metadata and merged output
sections during layout. Relocation patches each owned input payload without
cloning the section; ordinary input payloads are released after merging their
patched bytes. Relocated `__eh_frame` and `__compact_unwind` payloads stay alive
until unwind generation. The final image reserves its complete known size to
avoid another whole-image allocation when appending linkedit.

DWARF section bounds and metadata are validated even when source debugging is
omitted. DWARF payloads are not copied into individual section buffers. With
`debug_directory`, exact object snapshots are retained and transferred to
`Executable.debug_objects`, including selected archive members. Without it,
those snapshots are not allocated; the existing omitted-debug warning remains.

This reduces avoidable copies but does not provide bounded-memory linking for
inputs larger than RAM. The CLI still reads input files into owned buffers, and
symbol/relocation metadata, merged sections and the final byte image consume
memory. `keel link --system` remains an explicit alternative. Native errors do
not trigger automatic fallback. A future mapped-input or file-output API can
reduce these remaining terms without changing the current in-memory API.

The bounded replay harness compares two binaries against inputs generated by
`suite/streaming_memory.py`, checks executable byte equality, and verifies and
executes native Mach-O results:

```sh
python3 suite/link_memory_compare.py --baseline /path/to/before/keel \
  --candidate /path/to/after/keel \
  --corpus target/streaming-investigation-20261019/raw.json \
  --out target/link-memory-comparison
```
