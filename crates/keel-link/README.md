# keel-link

Native linkers that take input bytes and return an executable image: arm64
Mach-O through `link` in `lib.rs`, and x86-64 and AArch64 Linux ELF through
`elf::link`. `link_objects` also consumes owned AArch64 Darwin linker/cache units from
keel-asm, moving their payloads without Mach-O encoding or parsing.
Neither reads the filesystem; the driver reads inputs and installs
the output. The contracts are in the [Mach-O](../../docs/linking/macho.md),
[ELF](../../docs/linking/elf.md), [GNU script](../../docs/linking/gnu-scripts.md)
and [archive](../../docs/linking/archives.md) references.

## ELF phases

`elf::link_prepared` runs these phases in order, each a named function:

1. Parse the input units and resolve global and common symbols (`input.rs`,
   `groups.rs` for COMDAT groups).
2. Collect live sections and merge metadata such as notes and SFrame inputs
   (`gc.rs`, `notes.rs`, `sframe.rs`).
3. Allocate commons, compute section boundary symbols (`bounds.rs`) and plan
   imports from shared libraries.
4. Fix the symbol selection (`selected.rs`) and plan the GOT and, for dynamic
   output, the dynamic tables (`dynamic.rs`, `lifecycle.rs`).
5. Prepare `.eh_frame_hdr` (`unwind.rs`) and the note sections, then lay out
   sections and segments.
6. Write the image, apply relocations (`reloc.rs`, `aarch64.rs`), and finish
   SFrame, dynamic, unwind and section tables.

GNU scripts take the `script.rs` and `script/layout.rs` path, and partial
links (`-r`) take `relocatable.rs`.

## Module map

| Module | Owns |
| --- | --- |
| `lib.rs` | Mach-O linker: `link`, `link_signed`, symbol resolution and layout |
| `read.rs` | Mach-O object/archive reading and semantic unit adaptation |
| `extract.rs` | Fixed-point Mach-O archive member extraction |
| `gc.rs` | Mach-O dead-section collection |
| `parallel.rs` | Ordered scoped workers for independent Mach-O work |
| `tbd.rs` | Bounded TAPI v4 dylib interfaces |
| `unwind.rs` | `__unwind_info` from compact unwind entries |
| `write.rs` | Mach-O image writing and relocation |
| `sign.rs` | Ad-hoc code signatures |
| `archive.rs` | Deterministic static archives for both formats |
| `elf.rs`, `elf/` | ELF linker, with the phases above |
| `elf_selection.rs` | Ordered archive selection for ELF |
