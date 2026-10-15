# Native toolchain integration review — 2026-10-15

The x86-64 assembler and macOS linker sessions were reviewed together against
main `5581272`. Their independent histories were replayed onto a linear
integration branch so main could be updated with a fast-forward.

| Session commit | Replayed commit | Change |
| --- | --- | --- |
| `7399cf7` | `ca56126` | Native x86-64 ELF assembler and Linux object emission |
| `2580f24` | `ce60b34` | Native macOS arm64 Mach-O executable linker |

The original feature branches remain intact.

## Review fixes

Commit `cb3bd8d` fixes executable x86 alignment padding to use NOPs by default,
while preserving explicit zero fill and data-section zero fill. It also fixes
absolute-symbol operands: already-defined Intel RIP displacements fold as
constants, forward absolute references retain their relocation semantics, and
GOT references retain linker relocations. Differential tests compare bytes and
normalized relocations with Clang and GNU as, and a Linux executable regression
checks both padding and absolute addressing.

The macOS application validator now copies runtime fixtures into its output
folder, because Lua and QuickJS tests write temporary files. Prepared suite
inputs remain read only.

## Scope

The x86 assembler covers current Keel output with integer instructions, scalar
SSE, atomics, ELF relocations, DWARF line data, and CFI. It uses Intel syntax and
does not provide general AVX, x87, or AT&T compatibility. A subsequent x86 layout
follow-up adds safe same-section JMP/Jcc relaxation and omits unneeded temporary
ELF symbols; see the assembler scope document for its
binding and layout rules. The code generator still formats and reparses its
x86 instruction stream internally before native encoding.

The macOS linker is an explicit `keel link` command for arm64 Mach-O executable
output. It supports the tested archives, dyld imports/fixups, initializers,
compact unwind and exception data. It does not replace all Apple linker
features; unsupported forms produce diagnostics. The compiler's default link
path is unchanged, and `keel link --system` selects the system linker.

See [assembler scope](native-x86-assembler.md) and
[linker scope](macos-linker.md) for supported syntax, formats and exclusions.

## Integration validation

All checks below passed against the combined implementation and review fixes.

- macOS M5 Max: `cargo test --workspace --all-targets` (430 Rust tests),
  followed by assembler/driver/linker tests after the review fixes (231 tests)
  and the final x86 differential test rerun (9 tests).
- macOS applications: Lua (33 objects), QuickJS (9), and SQLite (2), compiled
  through native object emission in baseline and optimized modes. Native and
  Apple linking of the same objects both passed their runtime suites and
  signature verification.
- macOS linker fixtures: pointers/initializers, archives, relocation addends,
  backtraces, C++ exceptions, and compact unwind. Repeated launches and the
  ASLR probe passed.
- Linux NUC: workspace tests excluding architecture-specific assembler suites;
  assembler unit, structured, x86, and backend tests; and the complete compiler
  runtime matrix with `KEEL_TEST_ASSEMBLER=native` all passed. Modern AArch64
  differential tests ran on macOS rather than against the NUC's older Clang.
- Linux applications: Lua, QuickJS, and SQLite passed baseline, optimized,
  debug, and optimized selection modes (176 native objects across 12 builds).
  SQLite output matched a Clang build in each mode.
- Linux metadata: GNU readelf inspection, GDB source-line/locals/backtrace,
  and libgcc unwinding through a native Keel frame passed.
- Formatting, diff checks, and strict Clippy for the driver/linker passed.
  Unqualified strict assembler Clippy still reports 19 existing AArch64 parser
  warnings in files unchanged by this integration. The assembler check passed
  with only those existing lint categories allowed (`needless_return`,
  `collapsible_if`, `needless_borrow`, `items_after_test_module`).

The final test portability follow-up (`b9675b3`) accounts for GNU as rejecting
Clang's absolute-GOT syntax and GNU ld's relaxation restriction on that form;
execution testing explicitly retains GOT loads. Absolute PC32 and PLT32
relocations are normalized as equivalent for reference comparisons.
