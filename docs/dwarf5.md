# DWARF 5 line tables in the AArch64 Mach-O assembler

## Scope

An assembler and a compiler split DWARF work differently:

- **Line-table generation (the assembler's job).** From `.file` and `.loc`,
  `keel asm` builds the line-number program in `__DWARF,__debug_line`. In
  DWARF 5 it also builds the `__DWARF,__debug_line_str` strings that program
  references.
- **Everything else (the compiler's job).** Clang emits `__debug_info`,
  `__debug_abbrev`, `__debug_str`, `__debug_str_offs`, `__debug_addr`,
  `__debug_rnglists`, `__debug_loclists` and `__debug_names` as ordinary
  data. The assembler only accepts these sections and copies their bytes,
  relocations and assignments through.

This document covers the assembler side for Apple arm64 Mach-O. It does not
claim full DWARF 5 support in Keel:

- Keel's own C/IR code generator still emits a DWARF 4 compile unit and
  DWARF 4 `.file`/`.loc` directives, and that path is unchanged.
- There is no DWARF 5 split-DWARF (`.dwo`), type-unit or DWARF64 support.
- The line program for DWARF 5 is the same program used for DWARF 4.

## Selecting version 5

Like LLVM's `AsmParser::parseDirectiveFile`, `.file 0` upgrades the line table
to version 5. `clang -gdwarf-5 -S` always emits `.file 0` before any other
`.file` or `.loc`.

Without `.file 0`, the DWARF 4 output is byte-for-byte unchanged. The
`dwarf4_tables_are_unchanged_without_file_zero` test pins HEAD a24ae66's
bytes. In this mode, `md5` and `source` operands are errors rather than
being silently ignored.

There is no `--dwarf-version` flag. Without `.file 0`, LLVM's version 5
table names the assembler's working directory as directory entry 0 and
duplicates file 1 as the root. That depends on the environment, and Clang's
DWARF 5 output never needs it.

## Grammar

```
.file 0 "compilation-dir" "name" [md5 <integer>] [source "text"]
.file N ["dir"] "name"           [md5 <integer>] [source "text"]
.loc  F [line [column]] [basic_block] [prologue_end] [epilogue_begin]
        [is_stmt 0|1] [isa N] [discriminator N]
```

- `md5` takes an integer of at most 128 bits (`0x…`, decimal, `0…` octal or
  `0b…`). It is stored big-endian in `DW_FORM_data16`.
- `source` takes a quoted string with the usual escapes.
- In version 5, `.loc 0` refers to the root file.

## Semantics (following LLVM `MCDwarf.cpp`)

**Root file (`.file 0`, LLVM `setRootFile`)**

- The directory becomes include-directory entry 0.
- The name is kept unsplit, with directory index 0.

**Numbered files (`tryGetFile`)**

- A directory equal to the compilation directory is dropped.
- An empty name becomes `<stdin>`.
- A name without a directory is split at its last `/`, using POSIX
  `parent_path`/`filename` rules.
- Directories are deduplicated in first-use order, starting at index 1.

**Folding into the root.** A numbered entry whose name and checksum equal
the root's is folded into file 0, and its number is not allocated. The
directory is not compared. A later `.loc N` to that number is an error, as
it is in Clang; Keel's message names file 0.

**Header layout**

- The header contains: version 5, address size 8, segment selector size 0,
  `header_length`, and LLVM's standard parameters (`line_base` -5,
  `line_range` 14, opcode base 13, standard opcode lengths).
- Directory entry format: `DW_LNCT_path`/`DW_FORM_line_strp`.
- File entry format:
  - always `DW_LNCT_path`/`line_strp` and `DW_LNCT_directory_index`/`udata`;
  - `DW_LNCT_MD5`/`data16` when every file has a checksum;
  - `DW_LNCT_LLVM_source` (0x2001)/`line_strp` when any file has embedded
    source.
- The file count is the number of `.file N` entries plus the root.

**`__debug_line_str`**

- Strings are deduplicated in first-use order: compilation directory,
  directories, then each file's name and source.
- Mach-O needs no relocations for `line_strp` offsets.
- Any explicit `__debug_line_str` contents come first, and generated
  offsets account for them. LLVM would emit offsets from 0 in that case.

**Embedded source**

- An absent source is an empty string.
- An empty source is `"\n"`, following upstream LLVM and DWARF issue
  180201.1. The installed Apple clang-2100 still writes `""`, which makes an
  empty source indistinguishable from an absent one; this is the one
  intended difference from that Clang.

**When the table is emitted.** A version 5 table is emitted whenever
`.file 0` exists, even with no rows. This matches LLVM and keeps
`DW_AT_stmt_list` valid for data-only units. The DWARF 4 path keeps its
rows-only behavior.

**Line program**

- It is the DWARF 4 program: the initial file register is 1 and `.loc 0`
  emits `DW_LNS_set_file 0`.
- Keel uses standard opcodes rather than LLVM's special opcodes. Decoded
  rows are identical, but `__debug_line` is about 1.5x Clang's size on the
  measured corpus.

Mixed MD5 use is accepted, including a root `.file 0` without a checksum
followed by numbered files with checksums. Like LLVM, Keel omits the MD5
column for the whole line table when any entry lacks a checksum. Supplied
checksums still participate in file identity and root-file folding.

## Where Keel is stricter than LLVM

Each of these is an explicit error where LLVM would warn, guess or silently
drop data:

- `.file 0` without a nonempty directory. LLVM substitutes the working
  directory.
- A missing root name, a duplicate `.file 0`, or `.file 0` after a numbered
  `.file` or `.loc`.
- A folded entry whose `source` differs from the root's. LLVM silently
  keeps the root's source.
- Duplicate `md5`/`source` operands, malformed checksums, NUL in paths or
  source.
- Paths ending in `/` or starting with `//`, where LLVM's components are
  surprising.
- Gaps in file numbers.

The `malformed_dwarf5_directives_are_diagnosed` test covers all of these.

## Interaction with other assembler parts

The atom check, Mach-O writer and CLI input decoding are outside the debug
module but needed for unchanged `-O2` output:

- **Cross-atom LEB128 differences in debug sections.**
  - Clang's v5 `__debug_rnglists` and `__debug_loclists` use
    `DW_RLE_offset_pair`/`DW_LLE_offset_pair` with
    `.uleb128 Ltmp-Lfunc_begin0` across functions.
  - LLVM resolves LEB128 differences by layout in every section. dsymutil
    interprets debug addresses in object space.
  - Keel limits its subsections-via-symbols atom check to non-debug sections,
    allowing these object-space differences while retaining the restriction
    for runtime data.
- **Mach-O segment vmsize.**
  - The writer aligns VM addresses and file offsets independently, so file
    padding can exceed address padding. Tools then reject the object
    because segment filesize > vmsize.
  - This also affected unchanged `-gdwarf-4` Lua `lauxlib.c` output.
  - The writer now uses `max(vmsize, filesize)` for the segment virtual size.
    Objects whose file size already fit within their virtual size are unchanged.
- **Raw bytes in comments.** Clang's verbose assembly prints DWARF data bytes
  (for example `DW_OP_implicit_value` operands) as raw characters in `;`
  comments, which need not be UTF-8. Only after strict decoding fails,
  `keel asm` (AArch64) accepts input whose invalid bytes all lie in `;` or
  `//` line comments outside quotes. Invalid bytes anywhere else still fail.

## x86-64 ELF

Not supported.

- The x86 assembler rejects `.file 0`, two-string `.file`, and `md5`/`source`
  operands explicitly. Previously it folded extra operands into a garbled
  file name.
- Its DWARF 4 line table is unchanged.
- It does not accept Clang's x86 output at all: for example the AT&T suffix
  mnemonic `testl` in `-O1` code.
- A DWARF 5 ELF table would also need `.debug_line_str` section
  relocations (`R_X86_64_32`), which the x86 metadata path does not emit.

## Evidence

- `crates/keel-asm/tests/dwarf5.rs` compares Keel against Clang's integrated
  assembler on directory, checksum, embedded-source, header-only, folding
  and pass-through cases. It also covers an unchanged `clang -gdwarf-5 -S`
  program that is linked, run and `dsymutil`-verified, plus negative cases
  and x86 rejection.
- `suite/dwarf5_differential.py` covers Lua (`-O0`/`-O2`), SQLite and
  QuickJS from unchanged Clang `-gdwarf-5` output, with a Lua link and dSYM
  comparison.
- Results are in `target/dwarf5-support-20261019/REPORT.txt`.
