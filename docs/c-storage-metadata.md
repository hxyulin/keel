# C storage and function metadata

`DataItem::Ptr` occupies the target pointer width at the current byte cursor.
It adds no padding and needs no storage alignment beyond one byte. `DataInit.align`
controls the containing object, not each relocation. Native assembly emits an
integer relocation at that cursor. LLVM uses packed aggregate fields, Cranelift
uses byte offsets, and wasm32 writes four little-endian fixup bytes. Pointer
loads still carry their own access alignment. Apple RTTI name tagging retains
its existing eight-byte storage and alignment requirements.

C static initializer relocation ranges must fit the object and must not overlap.
Address-valued bitfields remain unsupported, so relocation writes cannot replace
neighboring bitfields. Addends remain unsigned 32-bit byte offsets. Negative
addends and addends above `u32::MAX` remain unsupported.

C GNU `aligned(N)` on a function becomes `FunctionAttrs.align = Some(N)`.
IR spells it `align N`. `N` is a power of two from 1 through 65536.
Redeclarations merge by maximum. The attribute specifies minimum entry alignment;
native backends also retain their usual minimum, 16 bytes on x86-64 and four on
AArch64. It applies to the out-of-line definition, independently of weak binding,
section placement, and inline policy. LLVM and Cranelift exports preserve it.
Wasm rejects it because wasm functions have no native entry address alignment.
C `_Alignas` on a function remains invalid.

ELF `.note` and `.note.*` placement selects `Placement::Note` for constant,
non-TLS objects without relocations. Notes require object alignment of at least
four bytes. The typed assembly directive carries note kind and retention,
and both native ELF writers emit `SHT_NOTE`, `SHF_ALLOC`, and optional
`SHF_GNU_RETAIN`. Zero payloads occupy file bytes. The producer supplies complete
note records, including header lengths and name/descriptor padding; the compiler
does not synthesize or reinterpret their contents. Note objects cannot share a
section with code, writable data, TLS, relocations, or incompatible retention.
The stack marker `.note.GNU-stack` and processor property `.note.gnu.property`
remain reserved. Other section restrictions remain in force. Darwin and wasm do
not support ELF note placement. Diagnostic exporters continue to reject custom
sections explicitly. The bounded Keel executable linker also explicitly rejects
allocated producer notes such as `.note.Linux`; use a system ELF linker for
those objects. Its existing GNU ABI/property note handling is unchanged.

Packed relocations are representable in ELF and Mach-O relocatable objects.
Darwin's default chained dyld fixups cannot encode an unaligned pointer location.
Executable runtime tests use `-Wl,-no_fixup_chains` to select classic fixups.
This is a linker format constraint and applies to Clang's equivalent object too.

`crates/keel-c/tests/storage_metadata.rs` compares Clang and Keel relocation
bytes, offsets, symbols, addends, note section kind, flags and alignment, and
function entry alignment. It checks optimized and unoptimized IR, IR roundtrips,
and text, typed and chunked object parity. Native runtime and wasm runtime tests
check unaligned fixups and neighboring bytes.
