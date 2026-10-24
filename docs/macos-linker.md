# Native macOS arm64 linker

`keel link` links arm64 Mach-O objects and archives into a signed, runnable
macOS executable without invoking Apple's `ld`, `lld` or `codesign`. It is an
explicit, opt-in path: `keel cc`/`keel compile` output and the suite's Clang
link path are unchanged, and native errors never silently switch linkers.

## Usage

Build the CLI with Cargo and link objects produced by Keel, Clang or Keel's
assembler:

```sh
cargo build --release -p keel
target/release/keel cc -O --emit-object main.c -o main.o
target/release/keel link main.o libhelpers.a -lc++ -o program
./program
```

On macOS the default link target is `aarch64-apple-darwin`; libSystem is always
linked. Supported options:

| Option | Meaning |
| --- | --- |
| `-o PATH` | Output executable (default `a.out`). |
| `-l NAME`, `-lNAME` | Link `usr/lib/libNAME.tbd` from the SDK. |
| `--sysroot SDK`, `-syslibroot SDK` | macOS SDK to read interfaces from. |
| `-mmacosx-version-min VERSION` | Minimum deployment version (raised to the greatest input minimum). |
| `--dead-strip`, `-dead_strip` | Opt-in input-section garbage collection. |
| `--require-export SYMBOL` | Extra dead-strip root for symbols used only at runtime. |
| `--debug-map DIR` | Retain object snapshots in `DIR` for LLDB source debugging. |
| `--dsym PATH` | Build a dSYM with `/usr/bin/dsymutil` after signing. |
| `--system ...` | Forward the remaining arguments to the Clang driver instead. |

Any other option is rejected with a pointer to `--system`. Place `--system`
before Clang-only options so they are forwarded opaquely, including Clang's own
target, response-file, config and environment handling; see the
[system-link compatibility boundary](wasm-target-foundation.md#system-link-compatibility-boundary-revised-contract).
External Clang Wasm selection is not Keel Wasm support.

SDK discovery order is `--sysroot`, `SDKROOT`, then
`xcrun --sdk macosx --show-sdk-path`. Linking works on any host given an SDK;
running the output requires macOS arm64.

### Library API

The `keel-link` crate takes named object/archive bytes and explicit `Dylib`
interfaces. `link` returns a **not yet signed** Mach-O image plus warnings.
`write_signed` and the streaming `link_signed` write a sibling temporary file,
sign it in process, and atomically install the executable only after signing
succeeds. `Input.name` is only a diagnostic label and is never opened.

## Code signing

The in-process signer implements the narrow linker ad-hoc contract used by
LLVM's [lld CodeSignatureSection](https://github.com/llvm/llvm-project/blob/main/lld/MachO/SyntheticSections.cpp):
`LC_CODE_SIGNATURE` points to an embedded SuperBlob containing one version
0x20400 CodeDirectory, SHA-256 hashes of 4 KiB file pages (the last page hashes
only its actual bytes), identifier `keel.native`, `CS_ADHOC | CS_LINKER_SIGNED`,
and the `__TEXT` executable segment extent with `CS_EXECSEG_MAIN_BINARY`. There
are no special slots, entitlements, requirements, CMS identity, certificate
chain or timestamp. This supports local execution and integrity checking; it
provides no Developer ID identity, notarization or Gatekeeper trust, and it is
not a general-purpose signer for arbitrary Mach-O files.

The writer finalizes its content-derived UUID before signing; the signer then
finalizes the signature load command and `__LINKEDIT` sizes before hashing any
page. The code limit excludes the signature itself. Signed files are limited to
32-bit Mach-O file bounds, with overflow rejected before patching. Signing reads
at most 64 KiB at a time and keeps at most 16 page hashes in memory.

Output is never memory mapped while being written or signed. A fresh temporary
inode is closed before atomic replacement, avoiding premature Darwin signature
cache entries and keeping running executables intact. Failed writes or
signatures leave the destination unchanged and remove the temporary file.

## Output format

- One thin `MH_EXECUTE`, arm64 ALL, PIE/two-level namespace, base 4 GiB, 16 KiB
  segment pages, inaccessible `__PAGEZERO`, RX `__TEXT`, RW `__DATA` and R
  `__LINKEDIT`. File-backed data precedes zero-fill data. Section alignment up
  to 64 KiB is retained. Individual and merged sections are limited to 512 MiB;
  out-of-range branches are diagnosed rather than given branch islands.
- `LC_MAIN` enters `_main` through dyld's normal libSystem startup and return
  convention; there is no synthetic crt trampoline. `_main` may be in any
  file-backed `__TEXT` section marked `S_ATTR_PURE_INSTRUCTIONS` or
  `S_ATTR_SOME_INSTRUCTIONS`, including a custom code section, but not in
  zerofill.
- `S_MOD_INIT_FUNC_POINTERS` sections are retained for dyld initializers. C
  destructors use private initializer thunks calling libSystem `atexit`; see
  [C lifecycle](c-lifecycle.md). Clang destructor thunks that need
  `___dso_handle` are diagnosed as undefined.
- Imports use eager non-lazy binding, one GOT slot per referenced symbol and
  12-byte `ADRP/LDR/BR x16` stubs for external calls. Imported pointers and
  local PIE pointers are described by dyld bind/rebase streams, and indirect
  symbol tables describe GOT and stubs. Exports use a deterministic trie, so
  applications can look up their own symbols with `dlsym`.
- Fixups use `LC_DYLD_INFO_ONLY` opcode streams rather than
  `LC_DYLD_CHAINED_FIXUPS`. Load commands include `LC_BUILD_VERSION`, `LC_MAIN`,
  a content-derived SHA-256 `LC_UUID` (current dyld refuses to launch without
  one) and `LC_CODE_SIGNATURE`.
- The linker builds a version-1 `__unwind_info` with regular pages, explicit
  no-unwind gaps, a three-slot personality table pointing at GOT slots, LSDA
  indexes and DWARF FDE section offsets. Without dead stripping, whole
  `__eh_frame` sections keep their CIE/FDE relative layout.

Output favors correctness and diagnostics over size: uncompressed exports and
unwind tables, whole-section retention, eager binding and generous header
padding make executables larger than Apple's linker produces. No link-speed or
startup performance claim is made.

## Input contract

Objects must be thin little-endian arm64 `MH_OBJECT` files, as produced by
`keel-asm` or Clang.

- Relocations: UNSIGNED (64-bit pointers; 32/64-bit differences paired with
  SUBTRACTOR), BRANCH26, PAGE21/PAGEOFF12, GOT_LOAD_PAGE21/PAGEOFF12,
  POINTER_TO_GOT, TLVP_PAGE21/PAGEOFF12, and signed 24-bit ADDEND prefix pairs,
  each with range/alignment checks. Local UNSIGNED section references keep their
  object-address addends. POINTER_TO_GOT fields are replaced without treating
  embedded bytes as an addend, matching
  [ld64](https://github.com/apple-oss-distributions/ld64/blob/main/src/ld/parsers/macho_relocatable_file.cpp)
  (Apple's assembler can leave nonzero placeholders there).
- Data: text constants, writable data and pointer constants, BSS and Darwin TLS.
  Commons are coalesced by maximum size/alignment; strong definitions override
  tentative commons; duplicate strong definitions are errors.
- Unwind: ARM64 compact encodings, DWARF-mode compact entries plus `__eh_frame`
  with explicit pointer-difference relocations, `__gcc_except_tab`, and
  personality/typeinfo references. Personality and typeinfo symbols must resolve
  through selected dylib interfaces, e.g. `-lc++`. Foreign implicit FDE fixups
  are rejected and need the system linker.
- `LC_LINKER_OPTIMIZATION_HINT` is accepted and ignored (no instruction
  sequences are relaxed). Data-in-code payloads are rejected.
- Regular custom code/data sections and `N_NO_DEAD_STRIP` symbols are accepted;
  see [C sections and retain](c-sections-retain.md).

## Dylib interfaces

Libraries are read from `usr/lib/libNAME.tbd` in the SDK. Only TAPI v4 is
accepted. YAML is parsed structurally; only arm64-macos and compatible
arm64e-macos groups are admitted, and embedded or external re-exported libraries
are traversed. Runtime paths come from install names, and alias interfaces are
deduplicated. Unknown imports fail before any output is written. TAPI
availability directives, Objective-C synthesis and deployment-conditioned
symbol substitutions are not implemented. Lua, QuickJS and SQLite need libc
data imports and pointer rebasing as well as function calls; libm and pthread
resolve to libSystem.

## Archives

BSD/Darwin archives use demand extraction to a fixed point, ignoring the archive
symbol index. Every member is validated in input order, including unused ones.
Extraction keeps symbol and section summaries while borrowing member bytes;
only selected members receive owned section payloads, relocation arrays and
debug snapshots. Extraction is whole-member. Thin and GNU long-name archives,
LTO/bitcode and universal archives require the system linker. See
[static archives](static-archives.md) for creating archives with `keel archive`.

## Thread-local storage

Darwin TLS supports `S_THREAD_LOCAL_REGULAR` data, `S_THREAD_LOCAL_ZEROFILL` BSS
and 24-byte `S_THREAD_LOCAL_VARIABLES` descriptors from Keel and Clang objects
and archive members. Initialized template sections precede TLS BSS in one
contiguous VM range; the first template section uses the greatest
template-section alignment. Descriptor bootstrap pointers bind to libSystem's
`__tlv_bootstrap`; the third descriptor word is relocated to a template-relative
offset with no rebase. Descriptor sections are aligned to eight bytes even when
a Clang input advertises byte alignment. `MH_HAS_TLV_DESCRIPTORS` asks dyld to
initialize descriptor keys and thunks. TLVP page relocations address eager GOT
slots; local descriptor pointers rebase and dylib descriptor imports bind
normally. Export trie entries carry the thread-local kind, and TAPI
`thread-local-symbols` are imported.

There is no instruction relaxation or synthesized TLS runtime; per-thread
allocation follows dyld's TLV implementation, with no extra guarantee for
over-aligned data. Legacy `S_THREAD_LOCAL_INIT_FUNCTION_POINTERS` is rejected
(on the tested OS neither Apple-linked nor native-linked probes called its
callback); use compiler guard/wrapper initialization instead.

## Weak symbols

- External section-based `N_WEAK_DEF` symbols coalesce to the first weak
  contribution; any strong definition wins regardless of object order. Losing
  sections stay allocated because atoms are not split. Private-extern weak
  definitions coalesce only statically.
- Weak dylib exports keep their TAPI identity. A strong interface is preferred
  over a weak one. Weak exports carry the export-trie flag and `N_WEAK_DEF`;
  coalescible calls go through stubs/GOT, and pointer slots participate in
  sorted dyld weak-binding streams. Strong executable overrides get
  `NON_WEAK_DEFINITION` records and `MH_WEAK_DEFINES`; weak binding locations
  set `MH_BINDS_TO_WEAK`. Direct page references that cannot take part in
  dynamic coalescing are diagnosed.
- Available weak imports use their dylib ordinal and the weak-import bind flag.
  A symbol referenced **only** with `N_WEAK_REF` may be absent from every
  interface; explicit weak flat lookup then lets dyld write null. (Apple's
  linker needs `-Wl,-U,_absent` for the same result.) Any strong reference
  still requires a definition or interface.
- Legacy coalesced sections, weak local/absolute definitions, aliases and
  resolvers are errors. Dynamic behavior follows dyld's image and namespace
  rules; this does not imply general interposition.

## Source debugging

```sh
keel link main.o libhelpers.a --debug-map "$PWD/program.objects" -o program
# LLDB reads the retained object snapshots directly; keep the directory.

keel link main.o libhelpers.a --dsym program.dSYM -o program
# Runs /usr/bin/dsymutil after signing; snapshots may then be removed.
lldb program
```

`__DWARF` is retained only with `--debug-map DIR` or `--dsym PATH`; otherwise a
DWARF-omission warning is printed (runtime unwind is handled independently).
The linker writes exact, content-named object snapshots (including selected
archive members) to the debug directory and emits `N_SO`, `N_OSO`, `N_FUN`,
`N_GSYM` and `N_STSYM` debug-map records mapping object addresses to executable
addresses, including tentative `-fcommon` globals at their final addresses.
`--dsym` defaults the snapshot directory to the output path with extension
`debug-objects`; `--debug-map` overrides it. Use a distinct directory per
executable. Snapshot files may remain after a failed signing or dsymutil run.

In the API, set `Options.debug_directory` (an absolute UTF-8 path); `link`
returns `Executable.debug_objects` without touching the filesystem, and
`write_signed` materializes them. Callers writing the image themselves must also
write those snapshots.

LLDB source-line breakpoints, argument/local values and backtraces are tested
for unoptimized Clang and Keel `-g` C objects, both through the debug map alone
and through a standalone dSYM. Optimized variable recovery, split DWARF, debug
type archives, Swift/Objective-C debugging and arbitrary producer DWARF
extensions are outside the tested contract.

## Section boundary symbols

Raw assembler names `section$start$SEG$SECT` and `section$end$SEG$SECT` resolve
to the start and one past the end of an ordinary merged regular or zerofill
section. C callers can use these exact names through assembler labels (no extra
underscore). Boundaries include inter-contribution padding but not padding after
the final contribution. They are private-extern, resolved after layout, and
have no dyld export, import or stub; GOT slots and pointer rebases work
normally. Selected input definitions (including weak and common) take
precedence over synthesis, and a name exported by a selected dylib resolves as
an import first, as in ld64.

Only `__TEXT`, `__DATA` and `__DATA_CONST` segment names are accepted.
`__DATA_CONST` maps into writable `__DATA`, so either spelling addresses the
merged section. TLS and other special or reserved sections are excluded. A
request for an absent ordinary section creates an empty section with equal
start and end, even for a weak reference, and does not extract archive members.
This follows [ld64's boundary atoms](https://github.com/apple-oss-distributions/ld64/blob/main/src/ld/Resolver.cpp)
and [LLD's empty-section handling](https://github.com/llvm/llvm-project/blob/main/lld/MachO/SymbolTable.cpp);
the ELF linker's absent-section policy differs.

## Dead stripping

`--dead-strip` (or `keel_link::Options::dead_strip = true`) enables
**whole input-section** collection, even when an object carries
`MH_SUBSECTIONS_VIA_SYMBOLS`: two functions in the same input text section stay
together. This is deliberately more conservative than ld64/LLD symbol
subdivision. The default is off.

Roots are `_main`, symbols marked `N_NO_DEAD_STRIP`, sections marked
`S_ATTR_NO_DEAD_STRIP`, module-init sections, and `--require-export` symbols.
C `retain` produces a linker root; compiler-only `used` does not. A live TLS
descriptor keeps its template contribution and bootstrap import.

Use `--require-export _name` (or `Options::required_exports`) for executable
definitions reached only through `dlsym`, runtime-loaded DSOs or callbacks with
no static relocation edge. Names use exact Mach-O spelling, including the
leading underscore. These are roots, not an export allowlist. Private-extern or
absent required exports are errors, and required exports do not extract archive
members.

Other rules:

- Archive extraction still runs to its fixed point first, so a dead reference
  can select a member. Retained sections inside unselected members, and
  section-boundary references, do not extract members.
- Edges come from branch, page, GOT, TLVP and pointer relocations, both
  operands of SUBTRACTOR/UNSIGNED pairs, and decoded ADDEND prefixes. Dead
  imports, GOT slots and stubs disappear; live undefined references still fail.
  Dylib load commands are kept (no `-dead_strip_dylibs`).
- Live code keeps its compact unwind records and FDEs; live FDEs keep their CIE
  and personality/LSDA targets. Metadata does not root code. Unwind tables are
  rebuilt from surviving ranges. Unwind ownership must be unambiguous (one code
  contribution per record, symbolic code targets, local anchors); foreign
  implicit FDEs, external symbols in moving unwind metadata, and relocations
  crossing record boundaries are diagnosed. A live indivisible contribution
  containing a losing weak definition's unwind owner is an error.
- Boundary symbols describe only surviving contributions; a boundary reference
  does not retain a section. Mark registration records `retain`/`.no_dead_strip`
  to keep a table and its callback edges.
- Debug maps omit discarded ranges and keep exact original object snapshots.

References: [ld64 manual](https://github.com/apple-oss-distributions/ld64/blob/main/doc/man/man1/ld-classic.1),
[LLD mark-live](https://github.com/llvm/llvm-project/blob/main/lld/MachO/MarkLive.cpp),
[LLD unwind ownership](https://github.com/llvm/llvm-project/blob/main/lld/MachO/UnwindInfoSection.cpp),
[LLD subsection granularity](https://lld.llvm.org/MachO/ld64-vs-lld.html#subsections-via-symbols-and-warn-missing-subsections-via-symbols).

## Memory use

The CLI maps stable regular input files (reading unmappable ones) and streams
the executable through `link_signed`, so it never holds a complete output
buffer. Relocation patches each owned input payload in place; ordinary payloads
are released after merging, while relocated `__eh_frame` and `__compact_unwind`
payloads live until unwind generation. DWARF bounds are validated even when
debugging is omitted, but DWARF payloads are only snapshotted when requested.
`link_to` writes directly into an empty `Write + Seek` sink. The byte-returning
API reserves and keeps the full image.

This reduces copies but is not out-of-core linking: input section copies,
symbol/relocation metadata and merged output sections still use memory. Inputs
must stay unchanged until linking completes. Outputs and debug snapshots are
installed by atomic replacement, so using an input path as the output does not
truncate a mapped inode.

## Not supported

No general ld64 compatibility is claimed. Not supported: dylib, bundle or static
executable output; framework search; arbitrary dylib parsing; rpaths; custom
entry points; legacy `S_COALESCED` sections (other than `__eh_frame`);
local/absolute weak definitions; symbol aliases and resolvers; general
interposition; Objective-C metadata; legacy TLS initializer sections;
arm64e/PAC; chained fixups; function-starts/data-in-code synthesis; symbol-level
atom splitting; ICF; relaxation; LOH optimization; branch islands; linker
scripts and order files. Unsupported options and forms are diagnosed.

Only macOS 27 with the macOS 27 SDK has been tested; an older minimum version in
`LC_BUILD_VERSION` is not evidence of compatibility with older releases.

## Testing

```sh
cargo test -p keel-link -p keel
cargo test -p keel --test macho_link
```

The Rust tests cover signing (buffered vs streaming equivalence, independent
SuperBlob/CodeDirectory decoding, page hashes, short reads/writes, injected I/O
failures, re-signing rejection, overflow bounds), CLI output with an explicit
minimal SDK and empty `PATH`, input-as-output replacement, and dead-strip graph
rules. Portable tests run on any host; runtime and `codesign --verify --strict`
assertions run only on macOS arm64, where tampered fresh inodes are also checked
to fail verification and be killed at launch.

Native validation scripts compare Keel-linked and Apple-linked executables built
from **identical object files**:

```sh
cargo build --release -p keel
python3 suite/validate_native_linker_fixtures.py --keel target/release/keel --out target/native-fixtures
python3 suite/validate_native_linker.py --keel target/release/keel \
  --cached-root target/suite --out target/native-optimized --compile
python3 suite/validate_native_linker.py --keel target/release/keel \
  --cached-root target/suite --out target/native-plain --compile --plain
python3 suite/audit_native_linker_objects.py target/native-optimized
python3 suite/validate_native_linker_compat.py --keel target/release/keel --out target/linker-compat
```

`--cached-root` points at the suite cache populated by the ordinary suite
runner (read only). Omit `--compile` to assemble the cache's Keel `.s` files
instead. The fixtures cover mutable/BSS data, local and imported pointers, libc
calls, `dlsym`, archive extraction, addends, backtraces, C++
catch/cleanup/resume, compact exception handling restoring integer and FP
registers, initializers and ASLR. The compatibility script covers TLS (main
thread plus pthreads, alignment, archive and dylib TLS, C++ dynamic TLS init),
weak symbols against competing dylibs and LLDB debug maps/dSYMs. With these, Lua
5.4.7 `all.lua`, the nine configured QuickJS tests and the repository's SQLite
query suite pass for both plain and `-O` objects, with SQLite output identical
to Apple's link.

`suite/link_memory_compare.py --baseline OLD --candidate NEW --corpus
<raw.json> --out DIR` replays inputs generated by `suite/streaming_memory.py`
against two `keel` binaries and checks byte-identical, runnable output.

## Format references

The implementation follows the SDK's `mach-o/loader.h`, `nlist.h`,
`arm64/reloc.h` and `compact_unwind_encoding.h`, and:
[ld64 design](https://github.com/apple-oss-distributions/ld64/blob/main/doc/design/linker.html),
[dyld sources](https://github.com/apple-oss-distributions/dyld),
[dyld TLV initialization](https://github.com/apple-oss-distributions/dyld/blob/main/libdyld/ThreadLocalVariables.cpp),
[chained-fixup definitions](https://github.com/apple-oss-distributions/dyld/blob/main/include/mach-o/fixup-chains.h),
[nlist weak-symbol definitions](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/nlist.h).
