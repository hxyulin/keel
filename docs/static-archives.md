# Deterministic static archives

`keel archive` packages relocatable object files into a regular indexed `.a`
archive entirely in process. It does not invoke `ar` or `ranlib`, resolve
relocations, or link members together.

```sh
keel archive --target x86_64-unknown-linux-gnu -o libexample.a first.o second.o
keel archive --target aarch64-apple-darwin -o libexample.a first.o second.o
keel link --target x86_64-unknown-linux-gnu start.o libexample.a -o program
```

`--target` defaults to the native target on x86-64 Linux or arm64 macOS;
other hosts must supply it. `-o` is required, as is at least one member.
Members retain command-line order even when options appear between them.
`--` ends option parsing, allowing member paths beginning with `-`.
Only the two targets above are supported. There is no independent format flag:
the target selects GNU archives for ELF64 x86-64 objects and Darwin/BSD
archives for little-endian Mach-O64 arm64 objects. Executables, shared libraries,
archives as members, foreign-target objects and malformed object tables are
rejected before any output is installed.

## Names, bytes and indexes

The archive stores each input's basename, including duplicate basenames, in
input order. It does not deduplicate members or symbols. Names must be valid
UTF-8 without NUL, newline or carriage return. Names beginning with
`__.SYMDEF` are reserved for archive metadata. Object bytes are copied exactly,
including debug data and trailing bytes; reproducibility therefore also depends
on reproducible input objects.

Each member has timestamp, uid and gid zero and mode `0644`. Metadata never
comes from the input file's filesystem attributes. Index entries retain member
and symbol-table order and include externally visible strong, weak, common and
absolute definitions. Undefined symbols and local/file/section symbols do not
provide index entries. Multiple definitions remain separate index entries;
conflict resolution belongs to the consuming linker.

- GNU output begins with a `/` symbol table: big-endian 32-bit count and
  member-header offsets followed by NUL-terminated symbol names. A `//` name
  table stores long, whitespace-containing, non-ASCII or special member names
  as `name/\n`; member headers reference byte offsets in that table.
- Darwin output begins with an unsorted `__.SYMDEF` index. Its little-endian
  32-bit ranlib byte count precedes `(string offset, member-header offset)`
  pairs, then the string-table byte count and NUL-terminated symbol names.
  All names use BSD `#1/length` extended names. NUL padding in extended names
  aligns object payload starts to eight bytes without modifying their payloads.
- Archive headers are 60 bytes. Member sizes include extended names but exclude
  the newline used to pad odd-sized members to an even boundary. Index offsets
  address member headers, not object payloads. Outputs requiring more than
  32-bit offsets are rejected; 64-bit index dialects are outside this scope.

The library API is `keel_link::archive::build(&[Input], archive::Target)`,
returning archive bytes without filesystem access, or
`archive::write(&[Input], archive::Target, &Path)`. The library also permits an
empty indexed archive. `write` validates/builds first, writes a create-new
sibling temporary file, closes it, then renames it over the destination.
Read, validation, write and installation errors preserve existing outputs;
a temporary file is removed after failed installation. Durability across power
loss is not promised, matching the linker's rename-based installation model.

Object validation covers target/kind, load-command and section/table bounds,
entry sizes, symbol names and symbol section references. Packaging does not
check instruction encodings, interpret every relocation type, or require the
object to fall within Keel's executable linker's supported relocation subset.

## Validation and limits

`crates/keel-link/tests/archive.rs` checks both dialects independently of the
host: exact payloads/order, repeated bytes, duplicate basenames and symbols,
long/UTF-8/special names, odd payloads, index offsets, empty indexes,
weak/common/absolute/undefined/local symbols, malformed tables, wrong targets,
atomic replacement and transitive Keel extraction with unused conflicts.

`crates/keel/tests/archive.rs` runs native CLI interoperability: `ar t`, `nm`,
Keel linking, system Clang linking, executable exit status 42, compiler-produced
weak/common definitions and preservation of existing archives on errors.
These tests exercise system tools only as references/consumers.

The native linker implementations retain their existing extraction semantics;
the writer does not change resolution policy. In particular, the Darwin Keel
linker's extraction predicate currently selects section definitions rather than
common-only providers. Darwin system-linker common/weak consumption and archive
index contents are tested; common-only extraction by Keel needs separate linker
work if required.

Thin archives, universal/fat archives, append/delete/update operations, partial
linking and shared-library creation are deferred. No performance claim is made.

## Implementation validation, 2026-10-16

The implementation commit is `03a3a3b949e40d704ce1d143e389fb13a2a367ca`,
based on `9bf415d5fdc57b89b7ebd2113297600a6149e868`. Validation used this
isolated worktree on MacHost for focused correctness builds and the exclusively
allocated rm-nuc-01 at `/tmp/keel-static-archives-8635.ZFeift` for Linux.
The remote was inspected for load and existing compiler jobs before use;
all Cargo commands used one build job. Source SHA256 values match across hosts.

```sh
CARGO_BUILD_JOBS=1 cargo test -p keel-link
CARGO_BUILD_JOBS=1 cargo test -p keel --bin keel --test archive --test elf_link
CARGO_BUILD_JOBS=1 cargo clippy -p keel-link -p keel --no-deps --all-targets -- -D warnings
rustfmt --edition 2024 --check crates/keel-link/src/archive.rs crates/keel-link/src/lib.rs crates/keel-link/tests/archive.rs crates/keel/src/archive.rs crates/keel/src/main.rs crates/keel/tests/archive.rs
```

Darwin passed 34 linker tests and 22 CLI tests. Linux passed 32 linker tests
(the Darwin-native test module is disabled there) and 22 CLI tests. Formatting
and scoped all-target Clippy passed locally. Linux Clippy could not run because
that toolchain lacks `cargo-clippy`; no host components were installed.
Both hosts used rustc 1.99.0 (`b940084d7`, LLVM 23.1.1); system Clang versions
were Apple 21.0.0 and Ubuntu 18.1.3 respectively.

The [validation record](static-archives-validation-20261016.json) contains exact
commands/results, host/kernel details, source hashes, resolved tool hashes,
test/driver binary hashes, and archive/executable hashes. Separate manual
archive fixtures exercised repeated bytes, reverse dependency order, duplicate
basenames and an unused conflicting entry member; both Keel and system-linker
executables exited 42 on each native host. No benchmarks were run.

This is implementation-worker evidence for this revision. Independent review
and coordinator integration remain pending; historical validation of other
packages is not evidence for this change. Suggested umbrella follow-up: link
this document from `docs/design.md` and `suite/README.md` when the coordinator
integrates the feature. Neither umbrella file was edited here.

## Corrective implementation after review round 1, 2026-10-16

Correction `1cad30cdfcdd6fbf21ecd71372094a7564afb2b6` is based on
`2749c923819542fc794405256ce22ec38d144d9b`. Raw validation now rejects the
reviewer's missing ELF extended symbol index, Mach-O `N_SECT` zero ordinal,
and out-of-range ELF relocation symbol reference before output installation.
The original reviewer evidence in `/tmp/keel-review-e1c03471` is preserved;
its three original object files now fail with destination sentinels unchanged.

ELF definitions distinguish undefined, absolute, common and AMD64 large-common
(`0xff02`) special indices from ordinary section ordinals. Other reserved
direct indices are rejected. `SHN_XINDEX` requires a uniquely linked, correctly
sized `SHT_SYMTAB_SHNDX`, with zero unused entries and real extended section
ordinals in range. A positive fixture uses section `0xff00` and extended
`e_shnum`. Mach-O non-STABS `N_SECT` symbols require a nonzero ordinal within
the actual section count; common, absolute and indirect symbols retain their
format-specific meanings. A blanket rejection of generic `Unknown` sections
would incorrectly reject valid large-common and indirect providers.

ELF REL/RELA tables validate entry sizes, symbol-table links, target sections
and raw symbol ordinals. Mach-O relocation tables validate bounds and external
symbol/local section references. `ARM64_RELOC_ADDEND` contains a signed payload,
not a symbol ordinal; local `R_ABS` zero remains valid. Packaging does not apply
Keel's executable-linker relocation whitelist. Positive TLSGD and paired ADDEND
fixtures protect that boundary.

Sequential single-job focused tests passed on MacHost and isolated rm-nuc-01:
eight library tests and two CLI tests per host. Shared fixtures exercise 21
malformed cases through library build/write and CLI, preserving existing output
on every failure. Both formats' metadata checks run on each host. Native Keel
and system-linker probes exit 42 on each host, and compiler common/weak system
consumption passes. Local scoped all-target Clippy and formatting passed.

The [corrective validation record](static-archives-correction-validation-20261016.json)
contains exact commands/results, source/toolchain/tool/executable hashes,
original evidence hashes and reproducible native probe commands. Linux source
hashes match local source hashes. The remote directory is
`/tmp/keel-archive-correction-0b941bed.BGvZS2`; prior author artifacts and LinuxHost
were not used. No benchmarks, full regression, 4GiB/allocation, disk-full or
temporary-collision probes were run. Linux Clippy was not rerun. The documented
Darwin Keel common-only extraction boundary remains unchanged and was not
re-probed directly in this correction. Independent corrective review remains
pending.

## Corrective implementation after review round 2, 2026-10-16

This correction starts at `44120c5761fe5f59536c05afa857f69ce47d3aec`.
The raw-table boundary now additionally includes:

- Mach-O non-STABS `N_INDR` values must fit a string-table offset and identify
  a NUL-terminated string entirely inside the declared raw string table.
  Alias providers remain indexed, including aliases to undefined names. Empty
  target strings are structurally accepted; alias resolution belongs to consumers.
- ARM64 ADDEND records must have external=false, PC-relative=false and length=2,
  and immediately precede a BRANCH26, PAGE21 or PAGEOFF12 relocation at the same
  address. The pair must be external, length=2, and PC-relative for BRANCH26 and
  PAGE21, absolute for PAGEOFF12. Signed 24-bit ADDEND payloads remain exempt
  from symbol-ordinal checks. The following relocation still receives ordinary
  reference and site checks. This follows the pair kinds and flags described by
  [LLVM's ARM64 relocation attributes](https://github.com/llvm/llvm-project/blob/main/lld/MachO/Arch/ARM64.cpp).
- ELF REL and RELA sites use checked `offset + field width <= target size` for
  known fixed-width x86-64 fields (1, 2, 4, 8 or 16 bytes), independently of Keel
  executable-linker support. NONE has no site check. TLSDESC_CALL is an instruction
  marker and requires an in-section starting byte. Unknown and variable-width
  kinds, including COPY, also require only an in-section starting byte; their
  patch width is deliberately not inferred. Mach-O sites use the width encoded
  by `r_length`; ADDEND's own record has no independent storage field.
- ELF symbol-table `sh_info` must be at most the symbol count. All entries before
  that boundary must be local and all remaining entries nonlocal.

These are bounded raw-table, index, pair and site checks. They do not validate
instruction encodings, resolved values, alias graphs, every relocation pairing
rule, every symbol invariant, or arbitrary link-time correctness. The existing
Darwin common-only Keel extraction limitation remains unchanged.

The shared library/CLI fixtures add each new reviewer failure, unterminated
alias targets, ADDEND pair flag/type/address/reference errors, section-end and
arithmetic-overflow sites, and symbol partition errors. Positive cases retain
aliases, signed ADDEND extremes, all three pair types, TLSGD, unknown relocation
kinds, REL/RELA, and exact field-end boundaries. Historical special-symbol and
extended-index tests remain in place. Exact retained reviewer objects are checked
separately without changing the original evidence directories.

See the [round 2 validation record](static-archives-round2-validation-20261016.json)
for exact source revision, commands, results, hashes and remaining coverage gaps.
Independent corrective review and integration follow separately.
