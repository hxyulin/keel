OUTPUT_FORMAT(elf64-x86-64)
ENTRY(entry32)
PHDRS { image PT_LOAD FLAGS(7); }
SECTIONS {
  . = 0x100000;
  image_start = .;
  .multiboot : { KEEP(*(.multiboot)) } :image
  .text : { *(.text.boot) *(.text .text.*) } :image
  .rodata : { *(.rodata .rodata.*) } :image
  .data : { *(.data .data.*) } :image
  load_end = .;
  .bss (NOLOAD) : { bss_start = .; *(.bss .bss.*) *(COMMON) . = ALIGN(16); bss_end = .; } :image
  /DISCARD/ : { *(.eh_frame*) *(.comment) *(.note*) }
  ASSERT(bss_end <= 0x2000000, "guest exceeds 32 MiB RAM")
}
