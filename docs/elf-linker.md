# x86-64 ELF executable linker

`keel link` has a native, host-independent ELF64 executable path for x86-64
Linux. It consumes relocatable objects from Keel, Clang or GNU as, regular
archives and ELF shared-library interfaces, resolves symbols and relocations,
and writes an `ET_EXEC` image without invoking a system linker. It can
cross-link from macOS. (Freestanding AArch64 Linux linking is covered in
[ARM Linux](arm-linux.md).)

## Usage

Linux is the default link target on an x86-64 Linux host; select it explicitly
elsewhere with `--target x86_64-unknown-linux-gnu`:

```sh
cargo build --release -p keel
cat > start.s <<'ASM'
.text
.globl _start
_start:
mov eax, 60
mov edi, 42
syscall
ASM
target/release/keel asm --target x86_64-unknown-linux-gnu start.s -o start.o
target/release/keel link --target x86_64-unknown-linux-gnu start.o -o program
./program; echo $?   # 42, on x86-64 Linux
```

The entry symbol defaults to `_start`. The kernel's initial stack (argc, argv,
environment, auxiliary vectors) is passed through unchanged; calling `main`,
exiting and any runtime initialization are the startup code's job unless you
link the installed CRT objects explicitly (see
[Installed CRT objects](#installed-crt-objects)).

| Option | Meaning |
| --- | --- |
| `-o PATH` | Output executable (default `a.out`). |
| `-e SYM`, `--entry SYM` | Entry symbol; must be defined inside executable code. |
| `-L DIR`, `-LDIR` | Add a library search directory. |
| `-l NAME`, `-lNAME` | Search `libNAME.so`, then `libNAME.a`, in each directory. `-l:FILE` selects an exact filename. |
| `-Bstatic`, `-Bdynamic` | Switch how subsequent `-l` options search. |
| `--static`, `-static` | Static library search only; shared inputs are rejected. |
| `--sysroot DIR` | Search target-relative library directories under `DIR`. |
| `--dynamic-linker PATH` | Interpreter path (default `/lib64/ld-linux-x86-64.so.2`); not opened at link time. |
| `--rpath PATH`, `-rpath PATH` | Emit `DT_RUNPATH`; repeat to join paths with colons. |
| `--gc-sections`, `--no-gc-sections` | Opt-in section garbage collection (last option wins). |
| `--` | End of options. |
| `--target aarch64-apple-darwin` | Use the [Mach-O linker](macos-linker.md) instead. |
| `--system ...` | Forward to the Clang driver instead. |

Other options are rejected. Native Linux links also search the conventional
x86-64 Linux library directories. Cross links consult only explicit paths and
`--sysroot`, never host Darwin directories, and nothing is downloaded.

The library API is `keel_link::elf::link(&[Input], elf::Options)`, which returns
image bytes and warnings without filesystem access. The CLI maps or reads the
inputs, writes a temporary file beside the destination, sets executable
permissions on Unix and renames it into place; a failed link preserves an
existing output. Allocated image size is limited to 512 MiB.

For bounded GNU script final linking through the library API, see
[GNU linker scripts](gnu-linker-scripts.md).

## Supported scope

- Little-endian x86-64 ELF64 `ET_REL` objects with the System V/Linux OS ABI.
- Allocated PROGBITS code, read-only constants, writable data and NOBITS BSS.
  Input sections are kept independently, in input order within each
  permission/zero-fill class. Mergeable strings and constants are kept without
  deduplication. Alignment is preserved up to 1 MiB.
- Local, global, absolute, common and weak symbols. Strong definitions override
  commons and weak definitions; commons combine maximum size and alignment; the
  first weak definition wins. Undefined weak references resolve to zero. Strong
  duplicate and unresolved definitions are errors. Same-named locals in
  different objects keep distinct identities.
- RELA relocations: `R_X86_64_NONE`, `64`, `PC32`, `PLT32`, `GOTPCREL`, `32`,
  `32S`, `PC64`, `GOTPCRELX` and `REX_GOTPCRELX`. Static calls bind directly;
  GOT loads keep their original instructions and use resolved read-only GOT
  slots. Narrow relocations are overflow-checked; 64-bit absolute arithmetic
  wraps.
- Fixed base `0x400000`, 4 KiB segment alignment, separate R/RX/RW LOAD
  segments, zero-filled BSS and a non-executable `GNU_STACK` header. Output is
  not PIE. Static output has no interpreter or dynamic relocations.
- Section headers and local/global symbol tables for `readelf`, `nm` and
  debugger function bounds. Input debug sections are omitted with a warning;
  other nonallocated metadata is omitted silently.
- `.eh_frame` inputs (PROGBITS or `SHT_X86_64_UNWIND`) merge into one section,
  with a generated `.eh_frame_hdr` and `PT_GNU_EH_FRAME` search index. See
  [Runtime unwind scope](#runtime-unwind-scope).
- Dynamic output includes a writable `DT_DEBUG` slot for the loader and
  debugger.

Not supported: automatic libc/CRT startup, static libc, PIE, shared-object
output, native TLS relocations, native IFUNC/GNU-unique definitions, REL
relocations, other allocated notes, linker scripts, identical-code folding,
relaxation and source debugging. Runtime-loader IFUNC resolution and
constructors inside input shared libraries do work. Unsupported sections,
flags, relocations and options are errors. This is not a general GNU ld
replacement.

## Static archives and shared libraries

Regular GNU/BSD `ar` archives (for example from [`keel archive`](static-archives.md))
are extracted lazily, left to right, and each archive is rescanned for newly
introduced dependencies until no more members are needed. Strong unresolved
references and the entry symbol pull members; undefined weak references do not.
An archive's strong definition can replace an earlier common. Unused members
are not linked, so their duplicate symbols do not conflict, and only selected
members get full target/relocation validation. Thin archives and archive groups
are unsupported; repeat an archive after its users when dependencies cross
archives.

```sh
keel link --target x86_64-unknown-linux-gnu start.o libhelpers.a -o program
keel link --target x86_64-unknown-linux-gnu start.o -L target-libs -lhelpers -o program
```

Supplying ELF shared libraries produces a dynamically linked `ET_EXEC`. The
linker reads their dynamic symbol tables, default symbol versions and SONAMEs;
it does not embed their code. At run time the loader performs eager `GLOB_DAT`
and writable `R_X86_64_64` relocations. PLT32 calls use short indirect stubs
through imported GOT slots, and GOT-based function addresses equal the address
seen inside the DSO. Imported data needs GOT addressing (compile foreign code
with `-fPIC`); copy relocations, imported direct PC32 addresses and dynamic
relocations into read-only sections are diagnosed. Lazy binding and
`GNU_RELRO` are not implemented.

- Visible executable definitions are exported in `.dynsym`, so libraries can
  call back into the executable. Strong undefined references in a shared library
  can pull definitions from later archives.
- Every explicitly supplied shared library becomes `DT_NEEDED`, in input order.
  Repeated SONAMEs coalesce; conflicting interfaces for one SONAME are errors.
  A library without a SONAME is named by its input basename. Transitive
  dependencies are left to the runtime loader.
- An absent weak reference stays a weak dynamic lookup when shared libraries
  are present.
- Default imported GNU symbol versions become `.gnu.version` and
  `.gnu.version_r` requirements; non-default/hidden versions are excluded.
  Explicit `name@VERSION` aliases are unsupported.
- GNU linker scripts such as glibc's `libc.so` are diagnosed; use
  `-l:libc.so.6` or a concrete path.

Use `$ORIGIN` to locate adjacent libraries, for example when cross-linking on
macOS against Linux libraries copied locally:

```sh
keel link --target x86_64-unknown-linux-gnu start.o program.o \
  target-libs/libhelper.so.1 --rpath '$ORIGIN' -o program
```

libc calls (`puts`, `memcpy` including glibc IFUNC, `exit`) are tested with
custom startup; this does not establish support for arbitrary libc APIs. The C
frontend emits ordinary weak definitions and references; see
[C weak symbols](c-weak-symbols.md).

## Opt-in section garbage collection

`--gc-sections` (or `elf::Options::gc_sections = true`) enables collection. It
is **off by default**, and `--no-gc-sections` turns it off again.

Order of operations: archive extraction, COMDAT selection, symbol resolution,
GC, unwind filtering and merging, common/import/GOT/stub sizing, then layout
and relocation. Extraction sees undefined references even from sections GC
later discards. `SHF_GNU_RETAIN` never extracts an archive member. Duplicate
strong definitions and unsupported inputs are diagnosed before GC, and TLS is
rejected even in dead sections.

Granularity is the input section: a live function keeps unrelated functions in
the same section, and mergeable sections are not split. Compile with
`-ffunction-sections -fdata-sections` for finer collection. Selected COMDAT
groups are atomic liveness units. Commons are reached individually; only live
commons enter `.bss.common`.

Roots:

- The entry symbol (`_start` or `-e`/`--entry`).
- Sections with `SHF_GNU_RETAIN`. Compiler `used` alone only forces object
  emission and is **not** a linker root.
- Every preinit/init/fini array, `.init`, `.fini` and admitted empty
  `.tm_clone_table`, together with their relocation targets. The selected
  `.note.ABI-tag` stays.
- When any shared library is linked, every default/protected-visible executable
  definition (preserving the existing export-all policy). Hidden definitions
  are reached only through references.

A **live** undefined `__start_NAME`/`__stop_NAME` reference retains every
contribution named `NAME` (C-identifier names only, matching GNU ld's default;
there is no `-z start-stop-gc` mode). Dead references create no root and
allocate no imports, GOT entries, stubs or dynamic relocations. Live strong
undefined references still fail. Selected DSOs remain `DT_NEEDED`; GC is not
`--as-needed`.

Unwind data follows code: each FDE belongs to the code contribution addressed
by its initial-location relocation. Live code keeps its FDE, CIE and
personality/LSDA targets; dead FDEs and unused CIEs are removed before merging,
with CIE back-references and relocation offsets adjusted. Unwind metadata never
roots code. A shared LSDA section is indivisible. For safety the rewrite is
stricter than GNU ld: every FDE needs an executable owner and an
initial-location relocation within the owner's original section; a live FDE
whose PC symbol rebinds to another contribution, live references or dynamic
exports into rewritten `.eh_frame`, and unrelocated nonzero relative pointers
in rewritten records are diagnosed.

SFrame (single AMD64 v3 producer) is also collected: complete records survive
only with their code, FRE blocks are preserved exactly, and an all-dead table
becomes a valid 28-byte empty header with its `PT_GNU_SFRAME`. SFrame never
roots code. GNU property reduction still covers every selected object, even
objects whose code is collected.

```sh
keel link --target x86_64-unknown-linux-gnu --gc-sections start.o -o program
cargo test -p keel-link --test elf_gc --test elf_sframe
```

## COMDAT selection before native garbage collection

ELF `SHT_GROUP` with exactly `GRP_COMDAT` selects the first included group for
each signature name, regardless of the signature symbol's binding. Selection is
independent of symbol resolution: a strong symbol can override a weak
definition in a selected group without discarding the group's other members.
Distinct signatures do not make duplicate strong symbols legal. Unnamed
`STT_SECTION` signatures use the section name, as in GNU ld and LLD. Signature
symbols defined in the group (as GAS/Clang emit) are metadata, not definitions,
and create no archive demand.

Selection is atomic across code, data, BSS, lifecycle arrays and their RELA
sections. Group structure is validated, including in losing groups; unknown
flags, non-COMDAT groups, special metadata members and orphan membership are
errors. References from kept sections to discarded global/weak definitions use
normal resolution; references to discarded locals or section symbols are
errors. Unreferenced losing definitions do not become duplicates or imports.

Archive extraction uses the same plan: only extracted members reserve
signatures, losing definitions cannot satisfy demand, and relocations only in
losing groups cannot extract members.

In a shared `.eh_frame`, FDEs whose initial-location relocation targets
discarded code are removed with their relocations, along with CIEs only they
used; kept CFI bytes are unchanged. References into a rewritten contribution,
symbols spanning discarded records, and unrelocated relative pointers needing
rebase are errors. Grouped notes and grouped SFrame are unsupported.

Native IR `weak_odr` definitions produce real COMDAT groups with weak signature
symbols on x86-64 ELF. C `weak` remains ordinary weak binding; there is no GNU C
COMDAT attribute or C++ frontend. Mach-O has no ELF groups; its `weak_odr` uses
weak symbol atoms.

Without `--gc-sections`, selected sections stay even when a selected weak symbol
is overridden; with it, groups follow the GC rules above.

```sh
cargo test -p keel-link --test elf_comdat
cargo test -p keel-codegen --test weak_odr
```

Real compiler fixtures live in `crates/keel-link/tests/fixtures/comdat`.
References: [System V section groups](https://www.sco.com/developers/gabi/latest/ch4.sheader.html#section_groups),
[signature binding clarification](https://groups.google.com/g/generic-abi/c/2X6mR-s2zoc),
[LLD ELF input selection](https://github.com/llvm/llvm-project/blob/main/lld/ELF/InputFiles.cpp).

## Registration-table boundaries

Allocated PROGBITS/NOBITS sections with C-identifier names (for example
`registry`, but not `.registry`) are placed contiguously by name, in input
order, at the first contribution's position within their permission class.
Same-name contributions must agree on type and WRITE/ALLOC/EXEC flags. Each
contribution keeps its own section index, symbols, relocations and alignment;
inter-contribution padding belongs to the span.

Undefined `__start_registry`/`__stop_registry` resolve to the first byte and one
past the last contribution. They are hidden global NOTYPE symbols, never
dynamic imports or exports, matching GNU ld's
[`-z start-stop-visibility=hidden`](https://sourceware.org/binutils/docs/ld/Options.html).
An existing strong, weak or common definition wins for each name. An empty
section has equal bounds. An absent section supplies no symbols, so its names
resolve normally (including imports from shared libraries); without a provider,
strong references fail and weak references are zero. Bounds never select
archive members. With `--gc-sections`, live boundary references root matching
sections. Rules follow [GNU ld](https://sourceware.org/binutils/docs/ld/Orphan-Sections.html)
and [LLD](https://github.com/llvm/llvm-project/blob/main/lld/ELF/Writer.cpp);
`cargo test -p keel-link --test section_bounds` compares Clang objects linked
by Keel and LLD (or GNU ld via `KEEL_TEST_ELF_LD`).

## C custom sections and retention

Supported allocated custom code/data/read-only/NOBITS sections, including
`SHF_GNU_RETAIN`, are accepted; the flag and GNU OSABI are preserved in the
output. With `--gc-sections`, retained contributions are roots; otherwise all
selected supported sections survive. Retention does not extract archive
members. See [C sections and retain](c-sections-retain.md).

## Executable initialization and termination

The [C frontend lifecycle subset](c-lifecycle.md) emits default-priority typed
init/fini arrays.

Dynamically linked output advertises `.preinit_array`, `.init_array` and
`.fini_array` through their address/size dynamic tags. Arrays are contiguous,
with `.init_array.N`/`.fini_array.N` ordered by numeric priority (0–65535),
input order for ties, and unsuffixed contributions last. Arrays need the
matching ELF section type, WRITE/ALLOC flags, 8-byte entries and 8-byte
alignment when nonempty.

Split `.init` and `.fini` contributions stay in input order with NOP padding.
`DT_INIT`/`DT_FINI` reference the defined `_init`/`_fini` symbols (hidden is
fine) rather than assuming the section start is the entry. Empty arrays and
unmarked code create no tags.

The dynamic loader and libc startup perform the dispatch; a syscall-only
`_start` does not. Static output keeps the sections but startup must invoke
them. Legacy `.ctors`/`.dtors` conversion and linker-defined array boundary
symbols are not implemented.

`suite/validate_elf_lifecycle.py --keel <keel> --out <dir>` (x86-64 Linux) uses
an explicit `__libc_start_main` adapter to compare Clang O0/O2 inputs against
GNU ld, checking preinit, legacy init, prioritized constructors, main, reverse
destructors and legacy fini ordering, with individual tags disabled as
negative controls.

## Linker-owned anchors and CRT clone-table bounds

Undefined `_GLOBAL_OFFSET_TABLE_` resolves to Keel's `.got` anchor, and in
dynamic output `_DYNAMIC` resolves to `.dynamic`. These names never extract
archive members or become imports, appear as hidden `.symtab` symbols, and
cannot be defined by inputs. A strong `_DYNAMIC` reference requires dynamic
output; a weak one in static output is zero. Shared libraries requiring either
anchor from the executable are rejected. This does not enable `R_X86_64_GOTPC32`,
the reserved GOT-entry protocol, lazy PLT binding or general GOT ABI
compatibility.

Empty writable PROGBITS `.tm_clone_table` contributions with alignment eight are
grouped so CRT list/end bounds stay equal. Nonempty clone tables are errors.

## Bounded GNU note metadata

Objects may contain allocated `.note.gnu.property` and one Linux
`.note.ABI-tag` (`SHT_NOTE`, `SHF_ALLOC` only, zero entry size/link/info,
alignment eight and four respectively). Unknown allocated notes, malformed
bounds, note relocations and additional ABI tags are errors. ABI-tag bytes are
preserved.

Property records are merged across all selected objects (including extracted
archive members, but not DSOs): `FEATURE_1_AND` intersects (absence counts as
zero), `ISA_1_NEEDED` unions, and `ISA_1_USED`/`FEATURE_2_USED` union only when
present in every object. Only FEATURE_1 bits 0–1, ISA_1 bits 0–3 and
FEATURE_2_USED bits 0–11 are accepted; FEATURE_2_NEEDED and unknown bits are
rejected. A surviving IBT bit is rejected until generated stubs carry
`ENDBR64`; SHSTK is accepted. Mandatory ISA requirements (such as the installed
CRT's `ISA_1_NEEDED=0xf`) are kept.

Nonempty results become one sorted `.note.gnu.property` with `PT_GNU_PROPERTY`
and an 8-byte-aligned `PT_NOTE`; an ABI tag gets its own 4-byte-aligned
`PT_NOTE`. References into consumed property sections are rejected, since their
offsets cannot be preserved. Fixtures and GNU reference provenance are in
`crates/keel-link/tests/fixtures/gnu-notes/README.md`.

## Runtime unwind scope

Accepted unwind records use DWARF32 lengths, CIE versions 1/3/4 (64-bit
addresses, no segments), empty or `z` augmentations with `P`, `L`, `R`, `S`,
and fixed-width absolute/PC-relative pointer encodings. Indirect
personality/LSDA pointers are kept; FDE code pointers must be direct. Frame
sections need `SHF_ALLOC` only, zero entry size and alignment of at least four.
Record bounds, CIE references, augmentation sizes, encodings, relocation fields
and FDE code ranges (inside executable sections, distinct starts) are checked.
Absolute 32/32S/64 and PC32/PC64 pointer relocations are applied; relocations
into CFI instructions or structural fields are unsupported. Input
`.eh_frame_hdr`, DWARF64, variable-width encodings and text/data/function-relative
or aligned pointers are unsupported. CFI instructions are copied verbatim.

When merging, zero terminators and alignment gaps are absorbed into the
preceding record as `DW_CFA_nop` (a leading empty contribution gets a synthetic
CIE). `suite/validate_elf_unwind.py` tests `_Unwind_Backtrace` through
frame-pointer-less assembly functions with Keel/Clang C callers against GNU ld.
This does not establish arbitrary C++ exception or static-libgcc support.

### Bounded SFrame v3 metadata

The linker keeps one selected `.sframe` input (`SHT_GNU_SFRAME`, `SHF_ALLOC`
only, zero entry size/link/info, power-of-two alignment from 8 bytes to 1 MiB)
and publishes it through one read-only `PT_GNU_SFRAME`. Producers are counted
after archive selection; a second selected `.sframe`, even an empty one, is
rejected. A complete 28-byte header with zero functions is kept; a zero-byte
section is invalid.

The accepted format follows binutils 2.47
[`sframe.h`](https://github.com/RTEMS/sourceware-mirror-binutils-gdb/blob/binutils-2_47/include/sframe.h)
and [`libsframe/sframe.c`](https://github.com/RTEMS/sourceware-mirror-binutils-gdb/blob/binutils-2_47/libsframe/sframe.c),
matching installed CRT objects: little-endian v3, AMD64 (arch 3), fixed FP=0,
RA=-8, no auxiliary header, PCREL flag and optional SORTED flag. The packed
28-byte header is followed by 16-byte indices (start fields at `0x1c + 16*i`,
deliberately unaligned), then independent FRE blocks that must partition the FRE
bytes exactly. Only DEFAULT, incrementing-PC, non-signal frames are accepted;
row PC and data widths are 1, 2 or 4 bytes; rows have 1 or 2 offsets, or 0 for
RA-undefined (as in crt1). FLEX, PCMASK, PAUTH, signal frames and other
flags/architectures/versions are errors.

Every index start field needs exactly one `R_X86_64_PC64` relocation (other
than ignored `R_NONE`), targeting retained executable code with a complete
function range. After relocation each PC is decoded and checked against its
target; function ranges must be increasing and nonoverlapping. Input SORTED is
never trusted and is set only after that check. Without GC, output bytes are
identical to input except for the relocated PCs and SORTED, and unordered
indices are rejected. With GC, kept records are re-sorted by final layout order
and their offsets recomputed.

Admission is bounded by
`1024 * functions + 32 * rows + 4 * serialized_bytes <= 64 MiB` in addition to
format and image limits; this is a working-set estimate, not a process RSS
bound. Block row declarations are checked against the header total before any
rows are allocated.

```sh
cargo test -p keel-link --test elf_sframe
# Cross-host structural comparison with GNU binutils (no execution):
python3 suite/validate_elf_sframe.py --keel target/release/keel --structural-gc \
  --as x86_64-elf-as --ld x86_64-elf-ld --readelf x86_64-elf-readelf --out target/elf-sframe-gc
# On x86-64 Linux, with installed libc and libsframe (add --gc-sections for GC):
python3 suite/validate_elf_sframe.py --keel target/release/keel --out target/elf-sframe
```

The Linux validator uses installed libsframe and `dl_iterate_phdr` to check
function and row boundaries, RA-undefined rows, LOAD backing, header sizes,
negative discovery/sorted controls, second-producer rejection and unused
archive selection against GNU ld. It establishes bounded metadata lookup, not
arbitrary CFI unwinding, PIE or general CRT compatibility. Some distributions
ship a binutils 2.47 package whose tool banners report 2.46; the installed
SFrame header is what matters.

## Installed CRT objects

Unmodified installed `crt1.o`, `crti.o`, `crtbegin.o`, `crtend.o`, `crtn.o`,
`libc.so.6` and, where needed, `libgcc.a` can be linked explicitly. The tested
programs (Clang `-O0` and `-O2`) cover argc/argv and stdio, a signed 128-bit
division runtime helper, `_DYNAMIC`, out-of-line C++ construction/destruction,
empty clone-table bounds with optional ITM callbacks from a shared library, and
C++ template COMDAT selection. Outputs combine GNU property and ABI notes,
SFrame, unwind data and real CRT lifecycle code. Pass concrete DSO paths to
avoid installed GNU linker scripts. Arbitrary C++ exceptions, PIE, linker
scripts and drop-in Clang-driver compatibility are out of scope.

```sh
python3 suite/validate_elf_crt.py --keel target/release/keel --out target/elf-crt
```

To cross-link, run it with `--prepare-only` on Linux (keeps objects, libraries
and `cases.json`), link each case with Keel elsewhere, place each
`<name>.keel` beside its `<name>.gnu`, then run `--verify-only` on Linux.
Rejection tests must run on the linking host. Results on x86-64 Linux with GC
disabled and enabled are in the
[Linux SFrame, GC and CRT runtime validation](validation/linux-sframe-gc/report.md).

## Validation

```sh
cargo test -p keel-link -p keel
# On x86-64 Linux, with clang, GNU ld, readelf and Python 3:
python3 suite/validate_elf_linker.py --keel target/release/keel --out target/elf-validation
python3 suite/validate_elf_libraries.py --keel target/release/keel --out target/elf-libraries
python3 suite/validate_elf_unwind.py --keel target/release/keel --out target/elf-unwind
```

The Rust tests cover deterministic images, symbol resolution, absolute
pointers, GOT entries, BSS alignment, alternate entry points, relocation
overflow, malformed inputs, GC, COMDAT, notes, SFrame and diagnostics. A CLI
test links with an empty `PATH`, checks end-of-options handling and that a
failed link preserves the destination, and on Linux executes the result.

The Linux validators link identical objects with Keel and GNU ld and compare
exit status and output: Clang O0/O2 with and without PIC, plain and optimized
Keel C, commons, strong-over-weak resolution in both orders, same-named locals,
absent weak references, argc/argv, globals, BSS and syscall code; plus archive
and shared-library search, mixed static/dynamic search, shared libraries pulling
executable definitions from archives, default-over-legacy symbol versions, and
versioned libc/IFUNC calls. These pass on two x86-64 Linux distributions
(Ubuntu 24.04 with Clang 18 and GNU ld 2.42; CachyOS with Clang 23 and GNU ld
2.47). Static-archive, shared-library and dynamic libc programs cross-linked on
macOS arm64 (against Ubuntu's glibc 2.39 interfaces) run unchanged on both. No
link-speed claim is made.

Format references: [System V ELF ABI](https://gabi.xinuos.com/elf/) and
[x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI).
