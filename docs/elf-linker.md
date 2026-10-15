# Static x86-64 ELF linker

`keel link` has a native, host-independent static ELF64 executable path for
x86-64 Linux. It consumes relocatable objects from Keel or Clang, resolves
symbols and relocations, and writes an `ET_EXEC` image without invoking a
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
`--static`/`-static` are accepted to make the output mode explicit. `--` ends
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
  interpreter or dynamic relocation processing. This output is not PIE.
- ELF section headers and a global symbol table for `readelf`/`nm`. Input debug
  sections are omitted with warnings. Ordinary `.eh_frame` bytes and their
  relocations are retained, but there is no generated unwind search index or
  tested exception/runtime-unwind contract.

The library API is `keel_link::elf::link(&[Input], elf::Options)`, returning
image bytes and warnings without filesystem access. The CLI reads all inputs,
links in memory, writes a temporary file beside the destination, sets executable
permissions on Unix, and renames it into place. Failed linking preserves an
existing output. Allocated image size is bounded to 512 MiB.

Archives, shared libraries, libc startup, PIE, shared-object output, TLS,
COMDAT groups, IFUNC/GNU-unique symbols, REL relocations, allocated notes,
constructor/destructor arrays, linker scripts, dead stripping, identical-code
folding, relaxation and source debugging are outside this milestone.
Unsupported allocated sections, flags, relocations and native options are
errors. Nonallocated metadata is omitted. This is not a general GNU ld
replacement or a static-libc linker.

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
```

Validated on 2026-10-16 on LinuxHost (CachyOS, Clang 23.1.1, GNU ld 2.47)
and rm-nuc-01 (Ubuntu 24.04, Clang 18.1.3, GNU ld 2.42). All nine runtime
comparisons passed on each host. The full linker/driver Rust tests passed on
macOS arm64 and LinuxHost; the eight focused ELF/CLI tests also passed on the NUC.
The [machine-readable report](../suite/reports/elf-linker-x86_64-20261016.json)
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

Format references: [System V ELF ABI](https://gabi.xinuos.com/elf/)
and [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI).
