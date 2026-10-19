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
- ELF section headers and a global symbol table for `readelf`/`nm`. Input debug
  sections are omitted with warnings. Read-only allocated `.eh_frame` inputs
  accept both PROGBITS and x86-64 `SHT_X86_64_UNWIND`, preserving their types,
  alignment, CFI bytes and supported pointer relocations. A generated read-only
  `.eh_frame_hdr` and `PT_GNU_EH_FRAME` advertise a sorted FDE search index to
  the runtime unwinder, including frames spread across independent sections.

The library API is `keel_link::elf::link(&[Input], elf::Options)`, returning
image bytes and warnings without filesystem access. The CLI reads all inputs,
links in memory, writes a temporary file beside the destination, sets executable
permissions on Unix, and renames it into place. Failed linking preserves an
existing output. Allocated image size is bounded to 512 MiB.

Automatic libc/CRT startup, static libc, PIE, shared-object output, native TLS
relocations, COMDAT groups, native IFUNC/GNU-unique definitions, REL relocations,
allocated notes, executable constructor/destructor arrays, linker scripts, dead
stripping, identical-code folding, relaxation and source debugging remain outside
the supported scope. Runtime-loader IFUNC resolution and constructors inside
input shared libraries are supported.
Unsupported allocated sections, flags, relocations and native options are
errors. Nonallocated metadata is omitted. This is not a general GNU ld
replacement or a static-libc linker.

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

The Linux validator below tests `_Unwind_Backtrace` discovery through three
independent assembly functions without frame pointers, with Keel/Clang C
callers, and compares GNU ld on the same objects. This does not establish
arbitrary C++ exception, standard CRT or static-libgcc support; COMDAT and the
other unsupported features above remain explicit errors. Startup/runtime setup
is still the caller's responsibility.

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

Keel's C frontend currently emits an ordinary strong undefined symbol for the
weak C declaration used by the validator. Weak-symbol tests therefore use
Clang-produced objects; Keel-produced C inputs exercise ordinary references.
The ELF linker does not infer weak binding from source declarations.

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
