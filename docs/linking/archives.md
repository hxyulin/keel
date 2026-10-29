# Deterministic static archives

`keel archive` packages relocatable object files into a regular indexed `.a`
archive entirely in process. It does not invoke `ar` or `ranlib`, resolve
relocations, or link members together. Output is byte-for-byte deterministic
for identical inputs.

## Usage

```sh
cargo build --release -p keel
target/release/keel archive -o libexample.a first.o second.o
target/release/keel link start.o libexample.a -o program
```

```text
keel archive [--target <triple>] -o <output.a> [--] <member.o>...
```

| Target | Archive format | Member objects |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | GNU | ELF64 x86-64 |
| `aarch64-unknown-linux-gnu` | GNU | ELF64 AArch64 |
| `aarch64-apple-darwin` | Darwin/BSD | Mach-O64 arm64 |

`--target` defaults to the native target on x86-64 or AArch64 Linux and arm64
macOS; other hosts must supply it. There is no separate format flag. `-o` and at
least one member are required. Members keep command-line order even when
options appear between them, and `--` ends option parsing so member paths may
begin with `-`. Executables, shared libraries, nested archives, objects for
another target and malformed object tables are rejected before any output is
written; an existing destination is left untouched on any error.

The resulting archives are consumed by the native [ELF](elf.md#static-archives-and-shared-libraries)
and [Mach-O](macho.md#archives) linkers, and by system `ar`, `nm` and
linkers.

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

## Library API

The library API is `keel_link::archive::build(&[Input], archive::Target)`,
returning archive bytes without filesystem access, or
`archive::write(&[Input], archive::Target, &Path)`. The library also permits an
empty indexed archive. `write` validates/builds first, writes a create-new
sibling temporary file, closes it, then renames it over the destination.
Read, validation, write and installation errors preserve existing outputs;
a temporary file is removed after failed installation. Durability across power
loss is not promised, matching the linker's rename-based installation model.

## Object validation

Validation covers target and object kind, load-command and section/table
bounds, entry sizes, symbol names and symbol section references, plus bounded
raw-table checks:

- ELF definitions distinguish undefined, absolute, common and AMD64
  large-common (`0xff02`) special indices from section ordinals; other reserved
  indices are rejected. `SHN_XINDEX` requires a uniquely linked, correctly sized
  `SHT_SYMTAB_SHNDX` with in-range extended ordinals. Symbol-table `sh_info`
  must partition local from nonlocal entries.
- ELF REL/RELA tables validate entry sizes, symbol-table links, target sections
  and symbol ordinals. Sites must satisfy `offset + width <= target size` for
  known fixed-width x86-64 fields (1, 2, 4, 8 or 16 bytes); compressed targets
  use the logical size from their compression header. Unknown or
  variable-width kinds (including COPY, and the TLSDESC_CALL marker) only need
  an in-section starting byte; NONE has no site check.
- Mach-O non-STABS `N_SECT` symbols need a nonzero ordinal within the section
  count. `N_INDR` aliases must name a NUL-terminated string inside the string
  table; aliases stay indexed and their resolution is left to consumers.
- Mach-O relocation tables validate bounds, external symbol and local section
  references, and sites using the `r_length` width. `ARM64_RELOC_ADDEND` records
  must be non-external, non-PC-relative, length 2, and immediately precede a
  BRANCH26, PAGE21 or PAGEOFF12 relocation at the same address with matching
  flags (see [LLD's ARM64 relocation attributes](https://github.com/llvm/llvm-project/blob/main/lld/MachO/Arch/ARM64.cpp));
  their signed 24-bit payload is not treated as a symbol ordinal.

Packaging does not check instruction encodings, resolved values, alias graphs
or every relocation pairing rule, and does not restrict members to the
relocation subset Keel's executable linkers support (for example, TLSGD
relocations are accepted).

## Tests and limits

```sh
cargo test -p keel-link --test archive
cargo test -p keel --test archive
```

`crates/keel-link/tests/archive.rs` checks both dialects independently of the
host, including the malformed-table cases above. `crates/keel/tests/archive.rs`
checks native CLI interoperability with `ar`, `nm`, Keel and system linkers.

The writer does not change linker resolution policy. In particular, the
native Darwin linker extracts members for section definitions but not for
common-only providers; system linkers do consume common/weak definitions from
these archives.

`keel archive` does not write thin or universal archives or 64-bit indexes,
and does not append, delete or update members.
