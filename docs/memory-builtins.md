# Explicit C memory builtins

`__builtin_memcpy`, `__builtin_memmove`, and `__builtin_memset` now lower to
Keel's existing `memcpy`, `memmove`, and `memset` intrinsics. They need no
library declaration and are advertised by `__has_builtin`. Ordinary `memcpy`,
`memmove`, and `memset` identifiers keep ordinary function call semantics.
This does not add a freestanding runtime or a new driver option.

The frontend converts lengths to `size_t` (unsigned long on both supported
targets), and memset's fill to `int`, then truncates the fill to the IR's
`i8` byte. All three arguments are evaluated once, left to right (an order
C permits), including at zero length. The expression returns the saved
`void *` destination; the IR memory intrinsic itself returns no value.
Constant lengths remain visible constants, including zero, small and large
sizes. There is no new size-specific expansion: both existing backends
currently emit libc calls for these intrinsics. This is an IR visibility and
correctness change, without a performance claim.

Memcpy requires disjoint regions; memmove preserves overlap support. Zero
length does not add a runtime pointer check or make otherwise invalid C
pointers valid. Argument typing accepts null pointer constants, independently
of whether execution would satisfy the C library's pointer preconditions.
Non-pointer arguments (except null constants), function pointers, const
destinations and retained volatile pointee qualification are diagnosed.
Keel conservatively retains pointer qualifier sugar through explicit casts;
these builtins therefore also reject casts which still retain volatile or
const qualification. Volatile loads used to compute ordinary pointer values
or lengths remain explicit volatile loads.

Fortified `__builtin___memcpy_chk`, `__builtin___memmove_chk`, and
`__builtin___memset_chk` no longer discard their fourth argument. A correctly
prototyped declaration of `__memcpy_chk`, `__memmove_chk`, or `__memset_chk`
lets the frontend emit a checked external call with all four converted
arguments, retaining bound evaluation and checking. Without that declaration,
or with an incompatible prototype, the call is diagnosed. Even a constant
bound smaller than the length stays on the checked call; Keel does not
silently turn it into an unchecked intrinsic. Checked variants are not
unconditionally advertised by `__has_builtin`.

This narrows compatibility with externally preprocessed fortified headers
that emit checked builtins without declaring their checked library symbols.
They now receive a diagnostic. Source inspection of the installed macOS SDK
`secure/_string.h` shows that its memory fortify macros test `__has_builtin`;
Keel's native preprocessor consequently leaves those memory names as ordinary
library calls. This is not a claim of complete SDK or glibc fortify support.
The legacy fallback for unrelated fortified builtins remains unchanged and
can still discard bounds; object-size queries still report unknown sizes.

The audit found existing lowering for `bswap16/32/64`, `clz`, `ctz`, and
`popcount` (including `l` and `ll`), typed and generic overflow, alignment
hints, atomic operations, floating classification, varargs, stack allocation,
frame address, and traps. This change does not duplicate them.

`tests/memory_builtins.rs` covers intrinsic shape, constant lengths,
destination identity, argument diagnostics, ordinary calls, volatile argument
loads, checked bounds and feature queries for both target backends. Its native
differential fixture compares Keel with Clang at O0 and O2 using explicit
builtins under `-fno-builtin`, side-effecting arguments, lengths 0/1/7/32/257/4096,
both overlap directions, destination return values and fill conversions.

Semantics references: [C11 draft N1570](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf),
sections 6.5.2.2, 7.24.1, 7.24.2.1, 7.24.2.2 and 7.24.6.1;
[GCC library builtins](https://gcc.gnu.org/onlinedocs/gcc/Library-Builtins.html);
[GCC object size checking](https://gcc.gnu.org/onlinedocs/gcc/Object-Size-Checking.html).
