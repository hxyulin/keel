# x86-64 ELF executable linker

`keel link` has a native, host-independent ELF64 executable path for
x86-64 Linux. It consumes relocatable objects from Keel or Clang, regular
archives and ELF shared-library interfaces, resolves symbols and relocations,
and writes an `ET_EXEC` image without invoking a
system linker. Linux is the default linker target on Linux; select it
explicitly when cross-linking from macOS:

```sh
cargo build -p keel
cat > start.s <<'ASM'
.text
.globl _start
_start:
mov eax, 60
mov edi, 42
syscall
ASM
target/debug/keel asm --target x86_64-unknown-linux-gnu start.s -o start.o
target/debug/keel link --target x86_64-unknown-linux-gnu start.o -o program
# On x86-64 Linux:
./program; echo $?  # 42
```

The entry symbol defaults to `_start`; `-e symbol` or `--entry symbol` selects
another defined symbol inside executable code. The kernel's initial stack is
passed through unchanged, including argc, argv, environment and auxiliary
vectors. Supply your own startup and syscall code. Calling `main`, exiting,
and any runtime initialization are the startup code's responsibility.
`--static`/`-static` force static library search and reject shared inputs. `--` ends
option parsing. `--target aarch64-apple-darwin` selects the existing Mach-O
linker. `--system` continues to select the explicit Clang fallback.

## Supported scope

- Little-endian x86-64 ELF64 `ET_REL` objects with System V/Linux OS ABI.
- Allocated PROGBITS code, read-only constants, writable data and NOBITS BSS.
  Input sections are retained independently, in input order within each
  permission/zero-fill class. Mergeable strings/constants are retained without
  deduplication. Alignment is preserved up to 1 MiB.
- Local, global, absolute, common and weak symbols. Strong definitions override
  common and weak definitions; commons combine maximum size and alignment;
  the first weak definition wins. Undefined weak references resolve to zero.
  Strong duplicate and unresolved definitions are errors.
- Explicit-addend RELA relocations: `R_X86_64_NONE`, `64`, `PC32`, `PLT32`,
  `GOTPCREL`, `32`, `32S`, `PC64`, `GOTPCRELX` and `REX_GOTPCRELX`.
  Static calls bind directly; GOT loads retain their original instructions and
  use resolved read-only GOT slots. Locals with matching names in different
  objects retain distinct identities. Narrow relocations are checked for
  overflow; full-width absolute relocation arithmetic wraps to 64 bits.
- Fixed base `0x400000`, 4 KiB segment alignment, separate R/RX/RW LOAD mappings,
  zero-filled BSS and a non-executable GNU_STACK program header. There is no
  interpreter or dynamic relocation processing for static output. This output is not PIE.
- ELF section headers and local/global symbol tables for `readelf`/`nm` and
  debugger function bounds. Named local symbols in retained sections and
  absolute locals preserve their input identity; omitted metadata stays omitted.
  Input debug sections are omitted with warnings. Read-only allocated `.eh_frame`
  inputs accept both PROGBITS and x86-64 `SHT_X86_64_UNWIND`. Multiple contributions
  merge into one PROGBITS section for debugger discovery, preserving input
  alignment, symbol targets, CFI instructions and supported pointer relocations.
  A generated read-only
  `.eh_frame_hdr` and `PT_GNU_EH_FRAME` advertise a sorted FDE search index to
  the runtime unwinder. Dynamic output includes a writable `DT_DEBUG` rendezvous
  slot for the loader and debugger.

The library API is `keel_link::elf::link(&[Input], elf::Options)`, returning
image bytes and warnings without filesystem access. The CLI reads all inputs,
links in memory, writes a temporary file beside the destination, sets executable
permissions on Unix, and renames it into place. Failed linking preserves an
existing output. Allocated image size is bounded to 512 MiB.

Automatic libc/CRT startup, static libc, PIE, shared-object output, native TLS
relocations, native IFUNC/GNU-unique definitions, REL relocations,
other allocated notes, linker scripts, identical-code folding, relaxation and source debugging remain outside
the supported scope. Runtime-loader IFUNC resolution and constructors inside
input shared libraries are supported.
Unsupported allocated sections, flags, relocations and native options are
errors. Nonallocated metadata is omitted. This is not a general GNU ld
replacement or a static-libc linker.

## Opt-in section garbage collection

`elf::Options::gc_sections = true` and native ELF CLI `--gc-sections` enable
collection. It is **off by default**; `--no-gc-sections` disables it again, with
the last option winning. Existing default links retain their input sections.
This is an ELF-only option; it does not change the Mach-O implementation.

The order is ordinary archive extraction, COMDAT selection, symbol winner
resolution, GC, ownership-based unwind filtering, unwind merging, common/import/
GOT/stub sizing, then layout and relocation. Extraction sees ordinary undefined
demand even from sections that GC later discards. RETAIN never extracts an
archive member; an admitted member's retained sections can become roots.
Duplicate strong definitions and unsupported input contracts still diagnose
before GC. TLS remains unsupported even in dead sections or losing groups.

Reachability uses original selected input contribution IDs and `(object,
symbol-index)` local identities. Global references follow the winning definition,
including strong-over-weak and first-weak selection. Ordinary input sections are
indivisible: a live function keeps unrelated functions sharing its section;
mergeable strings and constants are also indivisible, without fragment folding.
Use `-ffunction-sections -fdata-sections` when compiling for finer collection.
Selected COMDAT groups are atomic liveness units. Selection of the first group
and later liveness of its members are separate decisions. Commons are reached
by individual resolved symbol, then only live commons enter `.bss.common`;
their selected maximum size/alignment remains unchanged.

Roots are:

- The selected entry definition (`_start` or `-e`/`--entry`).
- Selected allocated sections carrying `SHF_GNU_RETAIN`. Compiler `used` alone
  only forces object emission and does **not** create a linker root.
- Every admitted preinit/init/fini array, `.init`, `.fini`, and the currently
  admitted empty `.tm_clone_table` CRT pieces. Their relocations retain callees
  and data. The selected `.note.ABI-tag` contract remains present.
- With any selected DSO, all selected default/protected-visible global
  executable definitions, including commons. This deliberately preserves the
  existing policy of exporting them all, and therefore also retains visible
  providers of selected DSO required symbols, weak lookups and preemption.
  Hidden/internal definitions are reached only by native references or other
  roots; GC does not introduce a new export-dynamic option or runtime lookup API.

A **live** undefined `__start_NAME`/`__stop_NAME` relocation retains every
selected contribution named `NAME`, then follows their relocations. Boundary
synthesis retains its existing C-identifier-only policy, matching GNU ld's
conservative default group-root behavior. There is no `-z start-stop-gc` mode.
A weak reference to an unsynthesizable boundary (for example,
`__start_bad.name`) resolves normally without retaining `bad.name`.
A dead boundary reference creates no root. Existing definitions of these names
continue to override synthesis. Dead undefined references do not diagnose or
allocate imports, GOT entries, stubs or dynamic relocations; live strong
undefined references still diagnose, and live weak references retain their
ordinary zero/import semantics. Selected DSOs remain `DT_NEEDED`; GC is not
`--as-needed` library elimination.

Unwind GC associates each FDE with the original code contribution addressed by
its initial-location relocation. Live code retains its FDE, the referenced CIE,
CIE personality references and FDE LSDA references, recursively including their
ordinary relocation targets. Dead FDEs and unused CIEs are compacted **before**
unwind contributions merge; CIE backreferences, relocation offsets and retained
record symbols are adjusted. Unwind metadata never roots otherwise dead code.
A shared CIE survives when at least one owner survives. A shared LSDA input
section is indivisible, so its other relocations can retain additional data/code.
CFI instructions and LSDA bytes retain the existing opaque supported ABI contract.

Bounded rewrite limits are explicit: every FDE needs a selected section owner
and an initial-location relocation. A live FDE whose global PC symbol rebinds
to a different contribution diagnoses; a dead weak loser is removed. Live
or dead FDEs must address an executable owner and fit its original section:
symbol values, signed initial-location addends and range arithmetic are checked
before pruning, so GC cannot hide malformed code ranges. Live
native references or dynamic exports into rewritten `.eh_frame` diagnose rather
than risk stale interior addends. Unrelocated nonzero relative encoded pointers
in retained rewritten records remain unsupported. Earlier COMDAT rewrite limits
continue to apply. These restrictions are stricter than a general GNU linker.

SFrame supports GC for one admitted AMD64 v3 producer. Before collection,
the linker binds original relocation identities and selected global winners,
and validates every record's executable range, including dead records and
available losing definitions. SFrame never roots its code. Complete records
survive only when their selected code contribution survives; exact FRE blocks
are preserved independently of index order. A zero-live result retains a valid
28-byte empty header and its exact `PT_GNU_SFRAME` span. Producer counting still
happens before GC, so a second selected producer (even empty) still rejects.
Live references or dynamic exports into rewritten `.sframe` diagnose; section
symbols are reset and other symbols into the rewritten table are removed.
Ordinary non-GC ordering behavior remains unchanged.
GNU property reduction continues over all selected objects, including objects
whose ordinary code/data is collected; GC does not weaken their note contracts.

```sh
# Cross-link without executing the resulting x86-64 image:
target/debug/keel link --target x86_64-unknown-linux-gnu --gc-sections start.o -o program
cargo test -p keel-link --test elf_gc --test elf_sframe
cargo test -p keel --test elf_link
python3 suite/validate_elf_gc.py --keel target/debug/keel \
  --as x86_64-linux-gnu-as --ld x86_64-linux-gnu-ld \
  --lld ld.lld --clang clang --readelf x86_64-linux-gnu-readelf --out /tmp/elf-gc
# Omit --lld when unavailable. Add --run only on x86-64 Linux.
# Runtime unwind comparison on x86-64 Linux with the documented libraries:
python3 suite/validate_elf_unwind.py --keel target/debug/keel \
  --gc-sections --out /tmp/elf-gc-unwind
# Intact modern CRT objects: link/execute GC replay on x86-64 Linux:
python3 suite/validate_elf_crt.py --keel target/debug/keel \
  --gc-sections --out /tmp/elf-gc-crt
```

Initial GC validation on macOS arm64 compared actual GNU-as input images linked
by Keel and GNU cross-ld: live pointer chains, shared-CIE FDE pruning, retained
registration boundaries, dead imports and unreachable cycles. Real Clang C
function/data-section objects also verified compiler `used` versus `retain`. It inspected
symbol tables, frame records and program/section headers and verified CLI
failure preserves an existing output. LLD was unavailable on that host; no
Linux x86-64 execution, actual exception execution or installed Linux CRT GC
execution is claimed by that cross-host run. The runtime unwind validator and
intact-CRT rejection mode above are provided for Linux validation.

## COMDAT selection before native garbage collection

ELF `SHT_GROUP` with exactly `GRP_COMDAT` selects the first included group for
an equal signature name, regardless of local/global/weak signature binding.
Selection is independent of ordinary global symbol resolution: a strong symbol
can override a weak definition in a selected group without discarding that
group's other members. Distinct signatures do not make duplicate strong
symbols legal. Unnamed `STT_SECTION` signatures use the referenced section name,
matching GNU ld/LLD. A signature alone creates no undefined-symbol demand.
Signatures defined in `SHT_GROUP` itself (including global signatures produced
by GAS/Clang) are metadata, not definitions for global resolution or archive
demand. They do not conflict with another group signature or a real definition.

Selection is atomic across ordinary code/data/BSS, supported lifecycle arrays,
and their RELA attachments. Group headers, symbol-table association, member
indices, SHF_GROUP ownership, relocation-table membership and RELA record sizes
are validated, including tables in losing groups;
unknown group flags, non-COMDAT groups, structural/special metadata members and
orphan membership diagnose. References from retained ordinary or grouped
sections to discarded named global/weak definitions use normal resolution.
References to discarded locals or section symbols diagnose; they are never
matched to a similarly named local in another object. Retained locals preserve
their input identity. Unreferenced losing definitions and undefined references
used only by losing sections do not become duplicate definitions or imports.

Archive demand uses this same selection plan. Only extracted members reserve
signatures; each archive is rescanned against current selections. Losing group
definitions cannot satisfy demand, and relocations solely in losing groups
cannot extract extra members. Existing left-to-right and weak-demand rules
remain in force. Property-note participation still follows included objects;
COMDAT is not permission to omit an object's processor/ABI contract.

Selection precedes unwind merging, global resolution, imports, lifecycle
planning, section bounds and layout. In a shared `.eh_frame`, an FDE whose
initial-location relocation targets discarded code is removed together with its
relocations; CIEs used only by removed FDEs are removed as well. Retained CFI
bytes stay unchanged, CIE links and relocation offsets are rebased, and only
retained FDEs enter the runtime index. References into a contribution requiring
this rewrite, global anchors in rewritten contributions, symbols spanning
discarded records, and unrelocated relative
pointers requiring rebasing diagnose. Grouped notes/SFrame are unsupported;
SFrame references to discarded code diagnose rather than retaining an invalid
range. Existing single-producer SFrame and property-note policies remain intact.
Only a real initial-location relocation with the expected pointer encoding can
select an FDE for removal. `R_X86_64_NONE` entries are ignored, including their
symbol indices and entries sharing a real pointer's offset.

On x86-64 ELF, native IR `weak_odr` function/data definitions produce separate
real COMDAT groups with weak signature definitions, through both text and typed
object routes. Ordinary C `weak` remains ordinary weak binding. There is no GNU
C COMDAT attribute or C++ frontend. IR `weak_odr` plus custom section/retain
continues to diagnose; assembler-authored supported groups may use custom names
and SHF_GNU_RETAIN, which survives selection without forcing extraction or
preserving duplicate groups. Mach-O has no ELF groups: its existing AArch64
`weak_odr` uses weak symbol atoms and does not promise arbitrary coalesced
section semantics.

COMDAT selection precedes optional native section GC. Without `--gc-sections`,
selected supported ordinary sections remain present even when a selected weak
symbol is overridden. With GC enabled, group reachability and unwind ownership
follow the opt-in GC policy above; native ELF TLS remains unsupported.
Validation: `cargo test -p keel-link --test elf_comdat`, the full linker suite,
and `cargo test -p keel-codegen --test weak_odr`. Real freestanding C++ object
fixtures and tool versions live in `crates/keel-link/tests/fixtures/comdat`.

ABI references: [System V section groups](https://www.sco.com/developers/gabi/latest/ch4.sheader.html#section_groups),
[signature binding clarification](https://groups.google.com/g/generic-abi/c/2X6mR-s2zoc),
and [LLD ELF input selection](https://github.com/llvm/llvm-project/blob/main/lld/ELF/InputFiles.cpp).

## Linker-owned anchors and CRT clone-table bounds

Undefined global `_GLOBAL_OFFSET_TABLE_` declarations resolve to Keel's `.got`
anchor, including declarations without relocations. For dynamic output,
`_DYNAMIC` resolves to the generated `.dynamic` section. These names do not
cause archive extraction or DSO import lookup and appear as hidden symbols in
`.symtab`, not exported dynamic symbols. Selected input definitions of either
reserved name are rejected. A strong `_DYNAMIC` reference requires dynamic
output; an undefined weak reference in static output remains zero. A shared
library requiring either hidden executable anchor is explicitly rejected when
its interface is selected; existing same-SONAME interface deduplication still
keeps the first equivalent export interface.
Supported absolute/GOTPCREL references use the normal relocation checks;
this does not enable unsupported GOT-base relocation families such as
`R_X86_64_GOTPC32`. The address anchor does not establish the conventional
reserved GOT-entry protocol (such as GOT[0] pointing to `_DYNAMIC`), lazy PLT
binding, or general GOT ABI compatibility.

Empty writable PROGBITS `.tm_clone_table` contributions with alignment eight
are grouped together while retaining their input symbol identities. This keeps
CRT list/end bounds equal despite intervening arrays or data. Nonempty clone
tables and other clone-table layouts remain explicit errors. These foundations
alone do not establish installed CRT or full compiler-driver compatibility.

## Bounded GNU note metadata

Selected relocatable objects admit allocated `.note.gnu.property` and one
Linux `.note.ABI-tag`. Both require `SHT_NOTE`, `SHF_ALLOC` alone, zero
entry size/link/info, and exact alignment eight/four respectively. Unknown
allocated notes, malformed bounds, unexpected owners/types and note relocations
are errors. ABI-tag bytes and original section/symbol address identities are
preserved; additional ABI tags are rejected. Named local `__abi_tag` symbols
retain their individual section identity in the output symbol table.

All property records in each selected object are collected before merging.
Unsorted arrays and duplicates are accepted for installed producer compatibility;
duplicates within one object are ORed. Across selected objects, including empty
and propertyless objects and extracted archive members, `FEATURE_1_AND`
intersects (absence is zero), `ISA_1_NEEDED` unions, and `ISA_1_USED` and
`FEATURE_2_USED` union only when that type is present in every object. Present
zero USED values retain their presence; emitted FEATURE_2_USED includes basic
x86. Generated import code adds baseline ISA usage only to complete USED data.
DSO properties and unselected archive members do not participate.

Only FEATURE_1 bits 0–1, ISA_1 bits 0–3 and FEATURE_2_USED bits 0–11 are
accepted. Unknown IDs/bits and FEATURE_2_NEEDED are rejected before reduction.
Mandatory ISA requirements, including installed CRT `ISA_1_NEEDED=0xf`, are
retained. Surviving IBT is explicitly rejected until ENDBR64 generated stubs
are implemented; SHSTK is admitted with the existing balanced call/tail-jump
contract. Import stub bytes and sizes are unchanged by this stage.

Nonempty output properties become one sorted, unique, zero-padded read-only
`.note.gnu.property`, exposed by `PT_GNU_PROPERTY` and a separate eight-byte
aligned `PT_NOTE`. An ABI tag gets its own four-byte aligned `PT_NOTE`. Empty
property results create neither property section nor property segments.
References into consumed property input sections are rejected because their
offsets cannot be preserved through reduction. This includes undefined strong
and weak references and dynamic relocations against default-visible preemptible
DSO definitions, using the resolved visible executable provider. Defined-symbol
lookups do not become undefined strong archive demand. DSO local, hidden,
internal, protected and symbolically bound definitions retain local binding;
an unused export or a symbol-free relative relocation is not a lookup.
Same-named locals and overridden weak definitions do not supply that provider.
All REL/RELA table associations must use the selected object symbol table and
an ordinary nonzero target section before any note is consumed; malformed or
unsupported associations are errors, including links the object reader skips.
DSO lookup accounting inspects runtime REL/RELA and PLT descriptors, requiring
matching allocated relocation sections, dynamic symbol/string associations and
LOAD bytes. Packed symbol-bearing relocation formats are rejected. RELR is
symbol-free. This bounded interface accounting does not certify arbitrary
runtime-loader, symbol-versioning or preemption ABI behavior; existing
same-SONAME interface selection still retains the first equivalent interface.

This is metadata support, not standard CRT compatibility or compiler-driver
integration. Full installed CRT linking still requires combined real-file replay
with the integrated SFrame support. Exact retained note
fixtures and GNU oracle provenance are in
`crates/keel-link/tests/fixtures/gnu-notes/README.md`.

## Executable initialization and termination

The [C frontend lifecycle subset](c-lifecycle.md) emits default-priority typed
init/fini arrays. It does not supply CRT startup or broaden the dispatch scope
below; other C attribute priorities remain explicitly diagnosed.

Dynamically linked output advertises executable `.preinit_array`, `.init_array`
and `.fini_array` contributions using their address/size dynamic tags. Array
contributions are placed contiguously, with `.init_array.N` and `.fini_array.N`
ordered by numeric priority (0–65535), stable input order for ties, and ordinary
unsuffixed contributions last. Arrays require their matching ELF section type,
WRITE/ALLOC flags, pointer-sized entries, and eight-byte alignment when nonempty.
Input entry size zero or eight is accepted; output entry size is eight.

Split `.init` and `.fini` code contributions remain in input order, with NOP
padding for alignment. `DT_INIT` and `DT_FINI` reference defined `_init` and
`_fini` symbols, including hidden definitions, rather than assuming the section
start is an entry point. Empty arrays and unmarked code do not create dispatch
tags. Original contribution identities are retained for symbols and relocations.

Dispatch is performed by the dynamic loader/libc startup and shutdown protocol;
a syscall-only `_start` does not initialize libc or arrange all lifecycle calls.
For static output the sections are retained, but startup remains responsible for
invoking them. Legacy `.ctors`/`.dtors` conversion and linker-defined array
boundary symbols are not implemented. Installed CRT objects still require
additional metadata support; this stage does not establish standard compiler
driver/CRT compatibility. The lifecycle validator uses an explicit
`__libc_start_main` adapter to isolate these dispatch semantics:

```sh
python3 suite/validate_elf_lifecycle.py --keel target/debug/keel --out target/elf-lifecycle
```

It compares Clang O0/O2 inputs against GNU ld, checks argument passing and the
order of preinit, legacy init, prioritized constructors, main, reverse-order
destructors and legacy fini, and disables individual dispatch tags as negative
controls. Run on x86-64 Linux under the caller's workload lock.

## Runtime unwind scope

Unwind records use DWARF32 lengths, CIE versions 1/3/4 (64-bit addresses,
no segments), empty or `z` augmentations with `P`, `L`, `R`, `S`, and fixed-width
absolute/PC-relative encoded pointers. Indirect personality/LSDA pointers are
retained; FDE code pointers must be direct. Nonempty frame sections require
alignment of at least four bytes, `SHF_ALLOC` alone, and zero entry size.
Record bounds, backward CIE references, augmentation sizes, pointer encodings,
relocation fields and final FDE code ranges are checked. The existing RELA
implementation applies absolute 32/32S/64 and PC32/PC64 pointer relocations;
relocations into CFI instructions or structural fields are explicitly unsupported.
FDE ranges must lie within executable input sections, with distinct start
addresses. Input `.eh_frame_hdr`, DWARF64, variable-width encoded pointers,
and text/data/function-relative or aligned pointer applications are unsupported.
CFI instructions are retained verbatim, not interpreted or exhaustively validated.
When merging contributions, the preceding record length absorbs zero terminators
and alignment gaps as `DW_CFA_nop` instructions. A leading empty contribution
uses a synthetic CIE to absorb its zeros. CIE references remain within their
original contribution; symbol values and relocation sites gain its merged offset.

The Linux validator below tests `_Unwind_Backtrace` discovery through three
independent assembly functions without frame pointers, with Keel/Clang C
callers, and compares GNU ld on the same objects. This does not establish
arbitrary C++ exception, standard CRT or static-libgcc support; the
unsupported features above remain explicit errors. Startup/runtime setup
is still the caller's responsibility.

### Bounded SFrame v3 metadata

The linker also retains one selected `.sframe` input with `SHT_GNU_SFRAME`
(`0x6ffffff4`), `SHF_ALLOC` alone, zero entry size/link/info, and power-of-two
alignment from eight bytes through 1 MiB. It publishes the same loaded bytes
through one read-only `PT_GNU_SFRAME` (`0x6474e554`, alignment eight), pointing
at the SFrame header. Archive selection occurs before producer counting;
unused members do not count. A second selected section is rejected, including
an empty header. A complete 28-byte header with zero functions, rows and FRE
length is retained; a zero-byte section is invalid.

The accepted contract follows the retained binutils-2_47 `sframe.h` and
`libsframe/sframe.c`, matching the installed CRT metadata examined on
2026-10-20: little-endian v3, AMD64 arch 3, fixed FP=0, RA=-8, no auxiliary
header, PCREL flag 4 and optional SORTED flag 1. The packed 28-byte header is
followed immediately by 16-byte indices, then independent FRE blocks. Index
start fields at `0x1c + 16*i` are intentionally unaligned. Attributes occupy
five bytes inside the FRE length; row counts exclude attributes. Blocks must
partition the FRE bytes without sharing, gaps, overlaps or trailing data.
Only DEFAULT, incrementing-PC, nonsignal frames are supported. Row PC widths
and signed data-word widths are one, two or four bytes; DEFAULT AMD64 rows
have N=1 or N=2 offsets, or N=0 for RA undefined (including crt1's PC4 row).
FLEX, PCMASK, PAUTH, signal frames, other flags/architectures/versions and
auxiliary conventions are explicit errors.

Every index start field requires exactly one RELA `R_X86_64_PC64`; no other
real relocation into the section is accepted. `R_NONE` remains a no-op.
Targets must resolve by original symbol identity into retained native executable
input code, with a positive complete function range inside that target section.
Undefined/imported, absolute, common, omitted and noncode targets are rejected.
After static relocation, each signed field-relative PC is decoded independently
and checked against that target's final location. Resolved function PCs must
increase without overlaps; input SORTED is never trusted, and unordered indices
are rejected until reordering is implemented. SORTED is set only after this
proof. Raw signed displacement order is insufficient because every field has
a different anchor. All admitted row bytes and index identities are preserved.

The implementation separates validated decoding, reconstruction, code binding,
and final address validation. `Table` owns a validated header and function
records in original index order. Each function owns its exact five-byte
attributes plus row/data bytes, decoded row PCs and byte spans, original start
field bytes, original index field offset, and original FRE offset. FRE ownership
is established in physical block order independently of index order; no shared
block is admitted. Header counts and packed offsets are checked against the
input length before records or rows are allocated.

A checked reconstruction plan names original function identities separately in
index order and FRE order. Both lists must describe the same set without
duplicates. Encoding recomputes counts, lengths and packed offsets, checks u32
format bounds and the linker's 512 MiB section limit before output allocation,
and returns original/output relocation-field mappings. It copies admitted
attributes and rows exactly. A changed index sequence clears SORTED; final
address validation alone certifies resolved order. Non-GC uses the identity
plan and checks that relocation fields do not move, preserving its output
bytes and its rejection of unordered input. GC selects complete records,
translates PC64 fields and code-section identities through compaction/merge,
and sorts indices using the same permission, lifecycle and registration-group
contribution ordering as final layout, followed by the section-relative code
PC. Synthetic contributions are appended later and do not change that relative
order. Rebuilt sizes and relocation maps exist before common/GOT/dynamic/note
allocation; final relocation validation checks actual resolved PCs, complete
ranges and nonoverlap again. No layout/reconstruction fixed point is needed.

Binding retains each relocation's source unit/symbol and addend, selected
global winner (or original local unit/symbol), executable output-section
identity and section-relative code start. It runs before undefined symbols can
become imports. Final fixups independently decode the relocated fields and
validate the bound ranges and order before setting SORTED.

GC preserves immutable original code identities as well as remapped selected
code identities. Available original ranges and all selected ranges validate
before pruning. COMDAT-grouped SFrame and references to code already discarded
by COMDAT selection retain their earlier diagnostics; ordinary SFrame can own
selected COMDAT code without rooting it. Common, undefined/import and non-code
targets still diagnose. Multiple producers, new architectures, versions,
FLEX/PCMASK/PAUTH/signal forms, shared FREs and partial-function filtering remain
outside this contract. Native ELF TLS remains unsupported.

Independent review on 2026-10-22 compared `7f03c9e8`/`dacf4bb0` with their
pre-refactor parent `323285f2`, keeping the independently reviewed ELF and
Mach-O GC changes in both builds. No production correctness defect was found.
The differential corpus replaced each of the 102 metadata bytes and 72 RELA
bytes in the byte-built CRT-shaped fixture with every u8 value, with GC both
disabled and enabled. Controls included the unchanged real CRT with minimal
startup stubs, unresolved/noncode targets and unordered/overlapping ranges.
Across 89,099 calls, all 7,232 admitted complete ELF images were byte-identical;
both builds rejected the other 81,867 calls. The only diagnostic differences
were 956 oversized malformed inputs: the old 512 MiB error became the explicit
reconstruction format/linker-limit error. Other rejection text and precedence
matched, including malformed metadata before the GC rejection. This is a
bounded differential corpus, not exhaustive malformed-input verification.

The unchanged real-CRT/stub output image SHA256 in that comparison was
`b03c891b36d693646b6e04b6b4c45970f1c71fcc834c273b8bb89424a407e242`.
The whole retained CRT and its `.sframe`/`.rela.sframe` hashes were independently
checked against the fixture README and the original retained evidence copy.
The lasting real-CRT test now compares every output metadata byte against the
input, allowing only the three independently computed PC64 values and SORTED.
The review-stage CLI test checked destination preservation for row parsing,
duplicate relocations, code-range binding, final overlapping ranges, and the
then-deliberate GC rejection of nonempty metadata and empty headers. Review
validation passed the full
`keel-link` suite, seven ELF CLI tests and `cargo check --workspace`.

GC implementation validation (2026-10-22) adds all 288 combinations of three
record liveness masks, index permutations and independent FRE permutations.
Tests check exact row blocks, counts, packed offsets, PC64 addresses and exact
program-header sizes. Additional cases cover weak global winners and original
losing ranges, local/section-symbol addends in the retained real CRT, executable
registration grouping and start/stop roots, pointer relocations, ordinary
metadata owning COMDAT code, simultaneous DWARF compaction, property/ABI notes,
malformed dead ranges/rows, common/undefined/non-code targets, ignored `R_NONE`,
and CLI atomic failures plus successful nonempty/empty GC output.

A fresh non-GC CLI comparison against `b25bc616` used the retained real CRT,
GNU-assembled startup stubs and four byte substitutions (0, 1, 127, 255) at each
of its 102 metadata and 72 RELA bytes. All 697 cases agreed on acceptance;
all 233 admitted complete images were byte-identical. The full linker suite,
seven ELF CLI tests, workspace checking and the existing cross-host GNU GC
validator also passed. The expanded focused run passed 24 unit tests and 26
SFrame integration tests. This bounded differential does not cover every byte
value or allocation near the admission cap.

The complete workspace test run also passed with
`KEEL_GNU_AS=x86_64-elf-as cargo test --workspace --no-fail-fast` (using the
installed cross-as absolute path). The initial run without that setting hit
unrelated assembler-oracle failures because macOS `/usr/bin/as` invokes Clang
and rejects GNU `--64`; the configured rerun completed successfully. No Mach-O
source changes were made.

SFrame admission now enforces the checked estimate
`1024 * functions + 32 * rows + 4 * serialized_bytes <= 64 MiB`, in addition to
format/image limits. This practical cap accounts conservatively for owned row
spans, blocks, planning maps and simultaneous metadata copies; it is not a
whole-process RSS guarantee. Large record/row/block/output vectors use fallible
reservation. Planning trees and bounded auxiliary vectors still use ordinary
allocation. A bounded test roundtrips 4,096 records, reconstructs 2,048 in reverse
index order, and rejects a roughly 2.5 MiB/110,000-record wire table before owned
record allocation. Arithmetic boundaries are tested without allocating near the
cap. Subsequent [LinuxHost admission-domain memory testing](../suite/evidence/sframe-memory-20261022/README.md)
exercised nearest-admitted and first-rejected function-, row-, mixed- and
payload-heavy shapes with GC off/on. All 174 checks passed; the largest observed
linker peak was 66,144 KiB (64.59 MiB). This does not establish a global maximum
for the linker or a hard RSS ceiling for every admitted encoding.
The cap intentionally rejects some formerly admissible very large non-GC tables.

Cross-host GNU structural comparison is available without running x86 images:

```sh
python3 suite/validate_elf_sframe.py --keel target/debug/keel --structural-gc \
  --as x86_64-elf-as --ld x86_64-elf-ld --readelf x86_64-elf-readelf \
  --out /tmp/elf-sframe-gc
```

On macOS arm64, GNU binutils `2.47.20260726` agreed on retained records, resolved
symbol identities, sizes and exact FRE bytes across all eight liveness masks,
with deliberately reordered indices and accompanying DWARF. GNU emitted a
zero-byte `.sframe` for the all-dead case; Keel emitted the specified 28-byte
empty table. The validator retains input objects, both images, header/SFrame
dumps, command logs and hashes. Installed libsframe runtime lookup and intact
CRT runtime replay support `--gc-sections` in their Linux validators, but these
new GC runtime modes were **not executed** on this arm64 host. They subsequently
passed on LinuxHost x86-64 Linux; see the [Linux runtime validation report](validation/linux-sframe-gc/report.md).

Independent GC review (2026-10-22, baseline `2a5d86d9`) found and fixed two
gaps. DWARF CIE personality/FDE LSDA pointers into `.sframe` could bypass the
rewritten-section reference check because their metadata contribution was not
yet marked live. A surviving section anchor could silently retain an addend
past the rebuilt empty table; ordinary global anchors instead failed later
with an invalid-symbol diagnostic. The check now also examines live owners'
already canonicalized edges before any rewriting. A regression covers both
anchor kinds, personality/LSDA fields, shared CIEs and all four two-owner
liveness masks; discarded pointers remain harmless and non-GC still accepts
these inputs.

The parser also trusted the header row total for budget admission but checked
the sum of block row counts only after allocating rows. An understated header
could therefore exceed the admitted working-set estimate before rejection.
A linear attribute preflight now proves that block declarations do not exceed
the admitted row total before reserving any owned rows. The regression proves
early rejection even with a malformed first PC and checks an understated-count
budget counterexample arithmetically, without near-cap allocations. Valid
tables and non-GC output bytes use the same reconstruction as before; only
malformed-input diagnostic precedence can change.

Review validation passed all 231 `keel-link` tests (including 27 SFrame tests
and the 288 permutation cases), all seven ELF CLI tests, `cargo check
--workspace`, all eight GNU SFrame structural comparisons and the existing GNU
ELF GC structural validator. GNU cross tools report `2.47.20260726`; execution
was on macOS arm64. The structural SFrame oracle compares resolved symbol
identities, sizes, exact FRE bytes and counts; program-header spans/empty-table
geometry are additionally asserted by the Rust tests. Its DWARF input is a
coexistence case, not a runtime unwind oracle. Linux libsframe lookup and CRT
runtime replay were not executed in that review. Subsequent
[LinuxHost runtime validation](validation/linux-sframe-gc/report.md) passed
libsframe lookup, actual dead-record filtering/reordering, installed CRT replay
and unwind checks with GC enabled and disabled. Later
[bounded memory and profiling runs](../suite/evidence/elf-link-profile-20261022/README.md)
measured near-admission-limit inputs and oversized rejection; arbitrary
maximum-sized ELF images and whole-process worst-case memory remain untested.
The added SFrame work consists of linear scans and sorting/tree lookups.
Existing GC fixed-point traversal and DWARF per-FDE field/relocation scans
retain their pre-existing worst-case quadratic behavior; the SFrame admission
budget does not bound those whole-linker costs. Reviewed ELF/Mach-O GC and TLS
contracts remain unchanged.

Pinned primary definitions: [binutils-2_47 `sframe.h`](https://github.com/RTEMS/sourceware-mirror-binutils-gdb/blob/binutils-2_47/include/sframe.h)
and [the matching decoder/encoder](https://github.com/RTEMS/sourceware-mirror-binutils-gdb/blob/binutils-2_47/libsframe/sframe.c).
The retained installed header and CRT bytes remain the ABI check; draft
alignment descriptions do not replace the packed 28/16-byte geometry.

The custom-startup Linux validator uses installed libsframe lookup and
`dl_iterate_phdr`, comparing identical inputs with GNU ld and checking function
and row boundaries, RA-undefined rows, LOAD backing, header sizes, discovery
and sorted-lookup negative controls, second-producer rejection and unused
archive selection:

```sh
python3 suite/validate_elf_sframe.py --keel target/debug/keel --out target/elf-sframe
```

Run it under the caller's workload lock. It uses concrete installed libc and
libsframe DSOs, without CRT startup. This proves bounded metadata lookup, not
arbitrary CFI unwinding, full executable coverage, PIE or standard CRT
compatibility. The observed reference package is binutils **2.47-4**, while
its assembler/linker/readelf banners report **2.46**; the installed SFrame
header matches the pinned **2.47** header. Keep these separate facts when
reporting reference results. The current manual's draft alignment prose and
older intermediate v3 layouts do not override those installed bytes.

## Static archives and shared libraries

Regular GNU/BSD `ar` archives are extracted lazily, left to right. Each archive
is rescanned for newly introduced dependencies until no more members are
needed. Strong unresolved references and the entry symbol pull members;
undefined weak references do not. An archive's strong definition can replace an
earlier common symbol. Unused members are not linked, so unused duplicate
symbols do not conflict. Archive symbol tables and long member names are parsed
by the object reader. Thin archives and archive groups are unsupported; repeat an
archive after its users when dependencies cross archive boundaries. Archive
members must be parseable objects; full target/relocation validation occurs only
for selected members.

```sh
keel link --target x86_64-unknown-linux-gnu start.o libhelpers.a -o program
keel link --target x86_64-unknown-linux-gnu start.o -L target-libs -lhelpers -o program
```

Supplying ELF shared libraries selects a dynamically linked `ET_EXEC` image.
The linker reads their dynamic symbol tables, default symbol versions and
SONAMEs; it does not embed their code. The Linux runtime loader maps the shared
libraries and performs eager `GLOB_DAT` and writable `R_X86_64_64` relocations.
PLT32 calls use short indirect stubs through imported GOT slots; GOT-based
function addresses resolve to the same function address seen inside the DSO.
Imported data needs GOT addressing (compile foreign code with `-fPIC`). Copy
relocations, imported direct PC32 addresses, and dynamic relocations into
read-only input sections are diagnosed. Static references to executable-defined
symbols retain executable precedence.

Visible executable definitions are exported in `.dynsym` so a library can call
back into the executable or reference its data. Strong undefined references in a
shared library can pull definitions from subsequent archives. All explicitly
supplied shared libraries are retained as `DT_NEEDED` in input order. Repeated
SONAMEs coalesce; conflicting export interfaces for the same SONAME are errors.
Only explicitly supplied libraries satisfy direct executable imports at link
time; transitive dependencies are loaded by the runtime loader using the DSOs'
own dependency/search metadata. An absent weak reference stays a weak dynamic
lookup when shared libraries are present.

The default interpreter is `/lib64/ld-linux-x86-64.so.2`.
`--dynamic-linker /target/path` overrides it; that target path is recorded without
opening it on the build host. `--rpath path` (or `-rpath path`) emits `DT_RUNPATH`;
repeat it to join paths with colons. SONAMEs become dependency names; a library
without SONAME uses its input basename. Use `$ORIGIN` to find adjacent libraries:

```sh
# These commands work on the Mac with Linux objects/libraries copied locally.
keel link --target x86_64-unknown-linux-gnu start.o program.o target-libs/libhelper.so.1 \
  --rpath '$ORIGIN' -o program
scp program target-libs/libhelper.so.1 linuxhost:/target/directory/
```

`-L dir` adds an explicit search directory; `-lfoo` searches `libfoo.so` then
`libfoo.a` within each directory. `-l:filename` selects an exact filename.
`-Bstatic`/`-Bdynamic` switch subsequent `-l` searches. Native Linux links also
search conventional x86-64 Linux library directories. `--sysroot dir` supplies
those target-relative directories instead. Cross links on macOS consult explicit
paths and the supplied sysroot, never host Darwin library directories. No SDK or
library is implicitly downloaded. GNU linker scripts such as glibc's `libc.so`
are diagnosed; select the concrete ELF with `-l:libc.so.6` or its path.

Default imported GNU symbol versions are emitted as `.gnu.version` and
`.gnu.version_r` requirements. Non-default/hidden versions are excluded from
unversioned lookup. Explicit `name@VERSION` input aliases are unsupported. libc
calls are usable with custom startup; `puts`, `memcpy` (including glibc IFUNC)
and `exit` are tested. This does not establish support for arbitrary libc APIs or
standard CRT objects. Dynamic output uses separate R/RX/RW mappings and writable
import slots; lazy binding and a GNU_RELRO segment are not implemented.

Keel's C frontend now emits ordinary weak definitions and undefined references;
see [C weak symbols](c-weak-symbols.md) for the scope and Clang/native object
validation. The ELF linker consumes symbol binding from objects.

## Validation

```sh
cargo test -p keel-link -p keel
cargo clippy -p keel-link -p keel --all-targets --no-deps -- -D warnings
cargo fmt --all -- --check
# On x86-64 Linux, with clang, GNU ld, readelf and Python 3:
python3 suite/validate_elf_linker.py --keel target/debug/keel --out target/elf-validation
python3 suite/validate_elf_libraries.py --keel target/debug/keel --out target/elf-libraries
python3 suite/validate_elf_unwind.py --keel target/debug/keel --out target/elf-unwind
```

Validated on 2026-10-16 on LinuxHost (CachyOS, Clang 23.1.1, GNU ld 2.47)
and rm-nuc-01 (Ubuntu 24.04, Clang 18.1.3, GNU ld 2.42). All nine runtime
comparisons passed on each host. The full linker/driver Rust tests passed on
macOS arm64 and LinuxHost; the eight focused ELF/CLI tests also passed on the NUC.
The machine-readable report
records host/tool versions, outcomes and executable sizes.

The portable Rust tests cover deterministic images, local/global/common/weak
resolution, absolute pointers, GOT entries, BSS alignment, alternate entry,
relocation overflow, truncated inputs, malformed relocation records/fields,
and diagnostics. A CLI integration test links with an empty PATH on macOS and
Linux, checks end-of-options handling, and verifies that failed linking preserves
an existing destination. On Linux it also executes the result.

The runtime validator links identical objects with Keel and GNU ld and compares
exit status and stdout/stderr. Cases include Clang O0/O2 with and without PIC,
plain/optimized Keel C, common-size merging and strong-over-weak resolution in
both object orders, same-named locals, absent weak references, argc/argv,
initialized globals, pointers, BSS and syscall code from Keel's assembler.
It inspects program/section/symbol headers with `readelf` and retains its JSON
report and program-header dumps under `--out`. No link-speed claim is made.

### Library and cross-compilation validation — 2026-10-16

The library report
records nine additional differential cases on each Linux host: direct/archive
search, O0/O2 shared imports, shared-library search, mixed static/dynamic search,
a shared library pulling an executable definition from an archive, choosing a
default symbol version over its legacy version, and versioned libc/IFUNC calls.
Shared probes check library constructors, imported mutable data, executable
exports and function-pointer identity. The 18 Linux comparisons pass, as do
the nine original static probes on each host. Full linker/driver Rust regression
suites pass on macOS, LinuxHost and the NUC; strict local Clippy and formatting pass.

The Mac cross-compilation report
records three executables assembled/compiled/linked on MacHost macOS arm64 and
copied unchanged to both Linux hosts. Static-archive, shared-library and dynamic
libc programs all exit 42; the libc program also prints `hello libc`. Matching
binary hashes prove the same Mac-linked outputs ran on both hosts. Existing
Linux library artifacts were copied to the Mac as link inputs; Keel does not
produce shared libraries yet. The libc interface used for cross-linking is
Ubuntu's glibc 2.39, with each target's own runtime libc used at execution.

Format references: [System V ELF ABI](https://gabi.xinuos.com/elf/)
and [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI).

### Explicit installed CRT validation — 2026-10-20

On LinuxHost, ten cases using unchanged installed `crt1.o`, `crti.o`, `crtbegin.o`,
`crtend.o`, `crtn.o`, `libc.so.6` and (where needed) `libgcc.a` pass with Keel
and GNU ld. Each executable ran three times with matching stdout/stderr and
exit status. The five sources, each built with ordinary Clang `-O0` and `-O2`,
exercise argc/argv and stdio, a signed 128-bit division runtime helper, the
`_DYNAMIC` anchor, out-of-line C++ construction/destruction, and empty clone
bounds with a shared library supplying the optional ITM callbacks. Inputs
were compiled on Linux, linked by Keel on macOS, and executed unchanged on
Linux. No custom startup adapter or input-section stripping was used.

The outputs contain GNU property and ABI notes, SFrame, native unwind data
and real CRT lifecycle code together. Input and output hashes are retained.
At the time of that CRT validation, intact COMDAT templates, surviving IBT
and second-SFrame-producer inputs rejected while preserving the destination.
COMDAT selection is now supported as described below; the current validator
includes the template at O0/O2 among its 12 cases. A subsequent
[LinuxHost Linux run](validation/linux-sframe-gc/report.md) passed all twelve
cases with and without GC, with three executions per tool per case. Arbitrary C++ exceptions,
PIE, GNU linker scripts and drop-in Clang-driver compatibility remain outside
that original validation. Concrete ELF DSO paths avoid installed GNU linker scripts.

On x86-64 Linux, reproduce with installed Clang, GNU ld/as (including SFrame
v3 output), readelf and Python:

```sh
python3 suite/validate_elf_crt.py --keel target/debug/keel --out target/elf-crt
```

For cross-linking, `--prepare-only` retains objects, concrete libraries and
`cases.json`; cross-link those inputs with Keel, place each `<name>.keel`
beside its `<name>.gnu`, then run `--verify-only` on Linux. That mode runs the
executables and inspects metadata; rejection tests must be run on the linking
host separately. The retained LinuxHost tool qualification is package
`binutils 2.47-4`, while `ld`/`as`/`readelf` banners report `2.46`.

### Registration-table boundaries

Admitted allocated PROGBITS/NOBITS sections with ASCII C identifier names
(for example `registry`, but not `.registry`) are placed contiguously by name,
in input order, at the first contribution's position within their permission
class. Same-name contributions must agree on type and WRITE/ALLOC/EXEC flags;
otherwise linking diagnoses an incompatible registration section. Their group
start observes the maximum input alignment. Each contribution keeps its own
section index, symbol offsets, relocations and alignment. Inter-contribution
alignment padding belongs to the span; padding before the next group does not.
This deliberately does not merge ELF section headers or change the existing
special-section, TLS, group or link-order contracts.

Undefined `__start_registry` and `__stop_registry` references resolve to the
first byte and the byte after the last contribution, after layout. These are
hidden global NOTYPE symbols and never dynamic imports/exports. An existing
selected strong, weak or common definition wins independently for each name.
An admitted empty section has equal bounds. An absent section supplies neither
synthetic symbol; its names follow ordinary undefined-symbol resolution,
including imports from selected shared libraries. Without a selected provider,
strong references fail and weak references resolve to zero. For a present
section, synthesized hidden bounds take precedence over shared-library exports.
Bounds do not select archive members merely because they contain the named
section; ordinary archive selection by actual defined symbol names still
applies.
With `--gc-sections`, live synthesized boundary references root matching
identifier-named contributions under the GC policy above.

The naming, grouping and input-definition precedence follow the
[GNU ld section rules](https://sourceware.org/binutils/docs/ld/Orphan-Sections.html)
and [LLD's optional start/stop definitions](https://github.com/llvm/llvm-project/blob/main/lld/ELF/Writer.cpp).
Hidden visibility is an explicit choice matching GNU ld's recommended
[`-z start-stop-visibility=hidden`](https://sourceware.org/binutils/docs/ld/Options.html),
rather than its historical protected default. `section_bounds` integration tests
compare actual Clang ELF objects and images with LLD (or GNU ld via
`KEEL_TEST_ELF_LD`), including empty, absent, weak and overridden bounds.

## C custom sections and retention

The native linker accepts supported allocated custom code/data/readonly/NOBITS
sections, including `SHF_GNU_RETAIN`, and preserves the flag and GNU OSABI in
its output. With `--gc-sections`, retained input contributions are roots; with
the option disabled, selected supported sections survive. Retention does not
cause archive extraction. Native and system-linker validation is recorded in
[C sections and retain](c-sections-retain.md).
