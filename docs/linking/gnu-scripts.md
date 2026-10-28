# GNU scripts for native ELF final links

`keel_link::elf::Options::script` accepts owned GNU linker script text. `None`
keeps the existing userspace layout. The script path supports static x86-64
ELF final links. It reads no files and invokes no external linker.

```rust,ignore
let options = keel_link::elf::Options {
    script: Some(expanded_script),
    ..Default::default()
};
let image = keel_link::elf::link(&inputs, options)?;
```

This is a bounded grammar for the expanded minimal x86-64 Linux v6.12 script,
not a replacement for all GNU ld scripts. Unsupported commands and expression
functions fail with a token position. Script text must be ASCII and at most
1 MiB. Expressions have at most 512 nodes, with nesting and evaluation-depth
limits.
Resolved aliases are cached between layout mutations. Self-reassignment uses
the previous value, which must be resolvable at that point.

Supported syntax includes:

- `OUTPUT_FORMAT("elf64-x86-64")`, `OUTPUT_ARCH(i386:x86-64)`, `ENTRY`,
  `PHDRS` with `PT_LOAD`/`PT_NOTE` and constant `FLAGS`, and `SECTIONS`.
- Symbol assignments, location-counter assignments and `+=`, forward aliases,
  lazy `PROVIDE`, and `ABSOLUTE`. Dot is captured at each assignment. Output
  section symbols retain the aggregated output section identity.
- Decimal/hex integers, parentheses, unary arithmetic, arithmetic/bitwise/
  comparison/logical operators, `ALIGN` with one or two arguments, `ADDR`,
  `LOADADDR`, `SIZEOF`, `MIN`, `MAX` and `ASSERT`.
- Output section addresses, `AT(expression)`, `(INFO)`, inherited and multiple
  `:phdr` memberships (one LOAD plus NOTE), `:NONE`, and four-byte big-endian
  output fill expressions. Fill patterns restart at each hole.
- All-file input selectors `*(...)`, `*` and `?` section wildcards, `KEEP`,
  `SORT`/`SORT_BY_NAME` with one pattern per selector, `COMMON`, and `/DISCARD/`.
  A list in one selector preserves input order; separate selectors order their
  contributions separately. First matching placement wins.
- `CONSTRUCTORS`, which GNU ld ignores for ELF. Constructor arrays still need
  ordinary input selectors.

GNU documents [PHDR inheritance](https://sourceware.org/binutils/docs/ld/PHDRS.html),
[load addresses](https://sourceware.org/binutils/docs/ld/Output-Section-LMA.html),
and [input-list ordering](https://sourceware.org/binutils/docs/ld/Input-Section-Basics.html).
Explicit PHDRS produce exactly the requested headers. A section inherits the
preceding PHDR selection. Without PHDRS, each nonempty allocated output gets a
LOAD. File offsets, virtual addresses and load addresses are separate values.
Relocation S/P use virtual addresses; `p_paddr` uses load addresses. The entry
may be a script-defined physical address inside an executable output's load
range. High virtual addresses do not consume file-size allowance. File-backed
outputs and reservations remain bounded by 512 MiB.

The two compiler-generated orphan classes in the pinned aggregate get bounded
placements: `.eh_frame` after `.rodata` (or `.text` when rodata is absent), and
`.data.rel.ro` after `.data`. Other nonempty orphans are errors. Input unwind
sections use the existing merger. Their CIEs are preserved without GNU's CIE
deduplication. Linux notes are validated before layout, reject relocations into
their payloads, and retain their bytes. Script NOTE headers use the final
output mappings. GNU properties are merged before script selection, so a script
can preserve or discard their canonical note.

Scripts currently reject GC, shared-library inputs, AArch64 output, SFrame,
GOT-dependent relocations, MEMORY/region layouts, file-specific selectors,
EXCLUDE_FILE, bracket/escaped wildcards, nested/mixed sort directives, byte
emission directives and PHDR FILEHDR/AT attributes. The existing input pipeline
continues to omit nonallocated debug/metadata sections. An explicit
`Options::entry_override` wins over ENTRY; otherwise ENTRY wins
over the fallback `Options::entry`. `Options::effective_entry` exposes the same
choice for archive selection. The CLI sets the override for `-e`/`--entry`.
Archive extraction is still performed before script symbol evaluation;
script-only symbol references do not independently pull archive members. The
pinned kernel uses whole-archive and explicit library groups.

# Validation

Run focused tests with GNU ld available on the host or through Nix:

```sh
KEEL_GNU_LD=/path/to/x86_64-unknown-linux-gnu-ld \
  cargo test -p keel-link --test elf_script
cargo test -p keel-link --lib elf::script
cargo test -p keel-link --test elf_script pinned_kernel_aggregate -- --ignored --nocapture
```

The ignored test reads `/tmp/keel-linux64-vmlinux.lds` and
`/tmp/keel-linux64-keel-aggregate.o` without modifying them. Set
`KEEL_KERNEL_AGGREGATE` to probe another input. Set `KEEL_KERNEL_OUTPUT` to save
an output; by default the image stays in memory.

On 2026-10-28 the unchanged pinned aggregate completed native final linking.
The output had RX, RW and RWX LOADs plus the requested NOTE, with physical entry
`0x1000000`. All 6,869 global symbol names matched GNU ld 2.46. Of these, 33
values between `.eh_frame` and the next large alignment differed because GNU
compacted CIEs. No reference symbol values were injected. Boot, Linux sorttable,
packaging and removal of the GNU partial-link stage were not tested here.

## CLI and kernel qualification

Use `-T path`, `-Tpath`, `--script path` or `--script=path`. Multiple script
files are concatenated in argument order within the 1 MiB total limit.
`-z max-page-size=0x200000` sets the 2 MiB LOAD alignment required by the pinned
Linux decompressor. The API equivalent is `Options::max_page_size`.

The initial aggregate-only result above has since been extended to native
archive selection, thin archives and ET_REL output. Both native link routes
pass boot and checked runtime workloads. See the
[native kernel qualification](../integrations/linux-kernel.md) for evidence,
commands and the external boot-stage boundary.
