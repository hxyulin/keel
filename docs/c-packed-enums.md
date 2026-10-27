# Packed enum layout

The C frontend honors GNU `packed` on enum definitions, before the tag or
after the closing brace. It selects the smallest integer representation that
holds every enumerator: unsigned for an entirely nonnegative range, signed
when any value is negative. Storage widths are 1, 2, 4 or 8 bytes. This follows
the [GNU enum packing rule](https://gcc.gnu.org/onlinedocs/gcc-14.1.0/gcc/Common-Type-Attributes.html).

Tag references and typedefs retain the selected integer type. Existing scalar
layout and lowering then handle arrays, record members, bitfields, initializers,
loads, stores, integer promotions, function arguments and returns. Packing the
enum does not pack a containing record. Its natural alignment equals that of
the selected integer type.

Enumerators represent mathematical values from signed 64-bit minimum through
unsigned 64-bit maximum. Implicit increments are checked before conversion to
stored bits. A range containing negative values and values above signed 64-bit
maximum rejects. Enumerators representable as `int` retain type `int`; wider
constants retain the selected underlying integer type after the definition.
C23 fixed underlying types retain their declared representation even with
`packed`, and reject out-of-range values.

This increment does not implement enum type identity independently of its
integer representation. Ordinary small enums retain the frontend's existing
`int` representation. Explicit enum alignment, fixed-type forward declarations
and enum representations wider than 64 bits reject. Existing ordinary
incomplete-enum extensions use `int`; a later definition that would change
that representation rejects rather than changing previously emitted types.

`packed_enums.rs` checks width boundaries and signedness on native targets and
the private Wasm32 layout. It checks IR roundtrips, native backend acceptance,
and text/typed/chunked object parity across 24 x86 selection, allocator, pool
and optimization variants. Its C harness checks narrow scalar arguments and
returns, record passing, signed loads, integer promotions, volatile stores,
initializers and adjacent field canaries against independently compiled code.
