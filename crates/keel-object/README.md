# keel-object

Semantic AArch64 Darwin link units shared by keel-asm and keel-link. Numeric
atoms carry section-relative identity, boundaries and dead-strip roots. Only
linkage symbols carry names. Fixups carry explicit addends, combine differences
and adjacent page-address pairs, and reference atoms or linkage symbols.
Compact-unwind records name their owning atom directly; DWARF fallback bytes
remain in sections.

`Object::encode` writes a versioned flat little-endian cache. Zerocopy records
use fixed-width integers and alignment one. `Cache::parse` borrows tables,
linkage names and payloads; `Object::decode` materializes an owned unit.
`keel_link::link_caches` borrows payloads and materializes mutable metadata.
The caller supplies cache keys and compiler-version invalidation.

See [the Mach-O guide](../../docs/linking/macho.md#owned-link-inputs-and-compilation-caches)
for the handoff, validation and debug-map limitation.
