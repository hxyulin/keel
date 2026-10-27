# Linux note fixtures

`aggregate.note.Linux.bin` is the 48-byte `.note.Linux` section extracted from
`/tmp/keel-linux64-keel-aggregate.o` on 2026-10-28. Its section header has
SHT_NOTE, SHF_ALLOC, alignment 4, and zero entry size, link and info. Its records
have owner `Linux\0`, types 0x100 and 0x101, and descriptor sizes 1 and 4.
`aggregate.s` reproduces these bytes literally.

`gnu-ld.note.Linux.bin` is the section dumped from GNU ld 2.46's executable
output for `aggregate.s`, assembled with GNU as 2.46. GNU readelf reports a
48-byte, read-only PT_NOTE with alignment 4. Regenerate with:

```sh
x86_64-elf-as --64 -o /tmp/linux-notes.o aggregate.s
x86_64-elf-ld -o /tmp/linux-notes /tmp/linux-notes.o
x86_64-elf-objcopy --dump-section .note.Linux=gnu-ld.note.Linux.bin /tmp/linux-notes
```

Linux's [ELF note macros](https://github.com/torvalds/linux/blob/v6.12/include/linux/elfnote.h)
use 4-byte alignment for names and descriptors. They impose no constraints on
descriptor contents. The validator therefore checks record bounds and the
Linux owner, preserves all types and descriptor bytes, and requires complete
padding without interpreting its bytes. Empty sections and empty descriptors
are valid. It allocates no memory based on untrusted record lengths.

## Caller integration required

The implementation in `notes.rs` validates Linux section metadata and returns
`None` so these sections retain ordinary section and symbol identity.
`Plan::prepare` validates retained payloads and records a PT_NOTE for each
nonempty Linux section, including when GNU properties are absent.

The current `elf.rs` caller still rejects `SectionKind::Note` on the ordinary
section path. Within that caller, `.note.Linux` needs payload validation via
`notes::linux(data)` before GC and an exception in the allocated-section kind
check. Relocation handling must reject writes into these validated payloads
or validate the final bytes again. No caller change is included because this
work is scoped to `notes.rs` and focused note tests and fixtures.

GC currently roots `.note.ABI-tag` but not `.note.Linux`. A script can retain
Linux notes with KEEP and assign its output note section to explicit PT_NOTE.
Automatic Linux-note retention under GC needs a caller or GC change if that
behavior is required. A script that merges or renames notes must derive its
PT_NOTE from the final output section, rather than reusing these input indices.
