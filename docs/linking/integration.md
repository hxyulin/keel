# Native toolchain integration

Keel's native x86-64 ELF assembler and native macOS arm64 Mach-O linker were
integrated together. This note summarizes their combined scope and how the
integration was validated. See [assembler scope](../targets/x86_64/assembler.md) and
[linker scope](macho.md) for supported syntax, formats and exclusions.

## Scope

The x86 assembler covers current Keel output with integer instructions, scalar
SSE, atomics, ELF relocations, DWARF line data, and CFI. It uses Intel syntax and
does not provide general AVX, x87, or AT&T compatibility. It performs safe
same-section JMP/Jcc relaxation and omits unneeded temporary ELF symbols. The
compiler hands instructions and ELF metadata to the assembler as typed items
rather than round-tripping through text; standalone Intel syntax input remains
available.

Executable x86 alignment padding uses NOPs by default, while explicit zero fill
and data-section zero fill are preserved. For absolute-symbol operands,
already-defined Intel RIP displacements fold as constants, forward absolute
references retain their relocation semantics, and GOT references retain linker
relocations.

The macOS linker is an explicit `keel link` command for arm64 Mach-O executable
output. It supports the tested archives, dyld imports/fixups, initializers,
compact unwind and exception data. It does not replace all Apple linker
features; unsupported forms produce diagnostics. The compiler's default link
path is unchanged, and `keel link --system` selects the system linker.

## Validation

- Workspace: `cargo test --workspace --all-targets` on macOS arm64, plus the
  assembler, driver and linker suites. Differential tests compare assembler
  bytes and normalized relocations with Clang and GNU as; a Linux executable
  regression checks padding and absolute addressing.
- macOS applications: Lua, QuickJS and SQLite compiled through native object
  emission in baseline and optimized modes. Native and Apple linking of the same
  objects both pass their runtime suites and signature verification. The
  application validator copies runtime fixtures into its output folder because
  Lua and QuickJS tests write temporary files; prepared inputs stay read-only.
- macOS linker fixtures: pointers/initializers, archives, relocation addends,
  backtraces, C++ exceptions, compact unwind, repeated launches and an ASLR
  probe.
- x86-64 Linux: workspace tests, assembler unit/structured/x86/backend tests,
  and the compiler runtime matrix with `KEEL_TEST_ASSEMBLER=native`. Lua,
  QuickJS and SQLite pass in baseline, optimized, debug and optimized-selection
  modes; SQLite output matches a Clang build in each mode.
- Linux metadata: GNU readelf inspection, GDB source lines/locals/backtraces,
  and libgcc unwinding through a native Keel frame.

GNU as rejects Clang's absolute-GOT syntax, and GNU ld restricts relaxation of
that form, so execution tests explicitly retain GOT loads. Absolute PC32 and
PLT32 relocations are normalized as equivalent for reference comparisons.
