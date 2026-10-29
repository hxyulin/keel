# C frontend

Keel compiles C11 with selected GNU extensions. Native targets are AArch64
Darwin, AArch64 Linux and x86-64 Linux; WebAssembly has a narrower frontend
and backend subset. This is not full GCC or Clang language compatibility.

`keel-c` preprocesses C source and compiles one translation unit to one module:
`keel cc in.c -o out.s`. `keel cc -E` prints the preprocessed text. `.i` inputs
and the library's `compile` API skip preprocessing; the `preprocess` API
returns text that `compile` accepts. Line markers (`# 12 "file.c"`) carry
source files and lines through to diagnostics and `-g`; output whitespace and
columns need not match Clang.

## Supported areas

- Native preprocessing, declarations, expressions, control flow and constant
  evaluation, including the GNU forms needed by the pinned kernel fixture.
- Structures, unions, packed records and enums, bit-fields, aggregate values,
  initializers and target-dependent layout.
- Native calls, varargs, aggregate ABI transport, TLS and executable lifecycle
  metadata, within the target's documented ABI subset.
- Bounded VLAs, cleanup attributes, GNU case ranges, union casts, statement
  expressions, type queries and overflow builtins.
- Scalar atomics and selected generic atomic builtins.
- GNU vector types, vector expressions and selected x86 intrinsic headers.
- Symbol visibility, weak definitions, checked function aliases, sections,
  retention and function optimization attributes.
- File-scope assembly and checked extended inline assembly, with a broader
  x86-64 constraint set than AArch64.

Beyond ISO C11 the frontend accepts `__attribute__((..))`, `#pragma pack`,
`__asm("_name")` symbol renames, C23 enums with a fixed type, zero-length
arrays and empty structs, block pointers (`^`, as plain pointers), `__int128`,
`typeof`, `__auto_type`, `__extension__`, statement expressions, `a ?: b`,
case ranges, range designators, computed `goto`, selected `__builtin_`
operations and `_Generic`. `setjmp` and friends are marked `returns_twice`.

## Preprocessing

Phase 2 line splicing precedes comment removal and preprocessing
tokenization. Macro expansion uses per-token hide sets, argument prescan,
stringification, placemarkers for empty pasted arguments, and rescanning with
the remaining input. C and GNU named variadics are accepted; GNU
`, ## __VA_ARGS__` distinguishes missing and explicitly empty arguments.
Conditional expressions use 64-bit signed and unsigned preprocessing integers
with short-circuit evaluation. `defined` produced by macro expansion is
accepted, as in Clang.

Quoted includes search the including file's directory first, then `-I`
directories, `-isystem` directories, embedded compiler headers and system
directories. Angle includes skip the first step. `include_next`,
`__has_include_next`, include guards and canonical-path `#pragma once` are
supported, as are `#line`, `#error`, pragma forwarding, `_Pragma`, `__FILE__`,
`__LINE__`, `__BASE_FILE__`, `__FILE_NAME__`, `__INCLUDE_LEVEL__` and
`__COUNTER__`. Includes and macro argument nesting are limited to 200 levels,
and each expansion has a token and work limit. `-D` and `-U` apply in
command-line order after the predefined macros.

The preprocessor runs no external commands and has no Clang fallback. Gaps
include trigraphs, universal character names in identifiers, C23
`__VA_OPT__` and newer directives, date and time builtins, dependency output,
complete framework lookup, Clang-specific pragma semantics and exact
diagnostics. `#warning` is accepted without output. The implemented semantic
pragma is `pack`. The CLI uses C11 mode.

## Target environment

`predefined_macros` generates integer types, ranges, literal suffixes, printf
formats, endian markers, atomic constants, floating-point limits and
data-layout macros for the LP64 targets from shared type descriptions. Darwin
`int64_t` is `long long` and Linux `int64_t` is `long`; `intmax_t` is `long`
on both. Darwin `wint_t` is signed and Linux `wint_t` is unsigned. Darwin
`long double` is `double`, and Linux has 80-bit `long double` layout macros.

`__KEEL__` and `__KEEL_VERSION__` identify Keel, alongside Clang 18.0.0 and
GCC 4.2.1 compatibility identities. These select source paths; they do not
promise compiler parity. CPU and vector feature macros are not copied from the
host. Standard capability macros report incomplete complex and VLA support and
no C threads implementation. `__has_builtin`, `__has_feature` and
`__has_attribute` use a conservative registry of what Keel implements; unknown
names return zero, ignored attributes are not reported, and `cleanup` reports
zero. Externally Clang-preprocessed files reflect Clang's answers instead.
`keel cc -E -dM` dumps the final macro set for either target.

## Sysroots and deployment

`--sysroot` (also `-isysroot`) selects the target SDK. Native lookup uses
`<root>/usr/include`, preceded by `<root>/usr/include/x86_64-linux-gnu` on
Linux, and Darwin frameworks use `<root>/System/Library/Frameworks`. An
explicit root adds no host include directories, and `-I`/`-isystem` paths are
not rebased. Native Linux uses `/` when no root is given and the target
matches the host. The macOS driver consults `SDKROOT`, then the SDK path
reported by `xcrun --sdk macosx --show-sdk-path`, which it reads directly for
the Command Line Tools. The library API leaves SDK selection to its caller.

The macOS deployment version defaults to 11.0. `-mmacosx-version-min` and
`MACOSX_DEPLOYMENT_TARGET` select the deployment macros; SDK selection does
not change them. `keel cc` does not link, so pass the same version to the
[native Mach-O linker](../../linking/macho.md) or the system linker.

## Compiler-owned headers

`include_str!` embeds ten C11 headers in the compiler, so installation needs no
source checkout or Clang resource directory. The same files serve every
target, with conditional ABI macros. `stddef.h` supplies target types,
`offsetof`, `max_align_t` and libc's `__need_*` inclusions. `stdarg.h` exposes
the frontend's `__builtin_va_list` and variadic builtins. `stdint.h` and
`limits.h` wrap sysroot headers with `include_next` in hosted environments and
stand alone otherwise. `float.h` uses the generated format limits;
`FLT_ROUNDS` describes the default rounding mode. `stdbool.h`, `stdalign.h` and
`stdnoreturn.h` provide the C11 spellings. `stdatomic.h` maps the C11 atomic
API to Keel's atomic builtins. `ptrcheck.h` supplies Darwin's pointer
annotations with bounds checking reported unavailable. The x86 vector headers
are described in [x86 intrinsics](../../targets/x86_64/intrinsics.md).

Libc headers, libraries, startup objects and runtime helpers come from the
target SDK. Keel does not ship a libc.

## Lowering

- **Parsing.** C cannot be parsed without knowing which names are types, so
  the parser keeps scopes, type-checks as it goes and builds a typed tree in
  which every implicit conversion is an explicit `Cast`. IR generation follows
  the tree.
- **Locals.** A scalar local whose address is never taken and that is not
  `volatile` becomes an SSA builder variable; everything else gets a stack
  slot. A declaration without an initializer starts a new value each time it
  is reached, so a local declared in a loop does not carry its value round the
  loop. Loops seal their header after the back edge, and labels are sealed at
  the end of the function. With `-g` every named local gets a stack slot.
- **Debug types.** Type checking compares `Ty`s, in which a typedef is the
  type it names, an enum is `int` and qualifiers are gone. A declaration also
  records a `Sugar` (`types.rs`) tree that adds typedef names, enums and
  qualifiers back for debug info. Only compound statements and `for` loops
  that declare variables become debug scopes.
- **Aggregates** are addresses: a struct-typed expression evaluates to a
  pointer to its bytes, assignment copies (loads and stores up to 64 bytes,
  `memmove` beyond, since the source may alias the destination), and calls
  pass them `byval` and return them `sret`. The frontend freezes by-value
  argument bytes when later arguments or callee evaluation may change memory.
- **Initializers** are flattened to (offset, value) items while parsing.
  Locals are zeroed and then stored to; static objects become bytes plus
  relocations.
- **Emission.** External definitions are always emitted; other functions are
  emitted only when something emitted refers to them. An inline-only
  definition becomes a local copy. A file-scope `extern` or non-inline
  declaration requests public emission unless the function is `static`. See
  [function attributes](../../compiler/function-attributes.md#c-declarations-and-contracts).
- **Qualifiers.** Expression qualifiers survive pointer arithmetic,
  dereferences, casts, decay, members, typedefs, `typeof`, `__auto_type`,
  conditional pointers and function returns. Volatile aggregate reads use
  explicit qualified accesses, and copies involving volatile storage never
  become `memcpy` or `memset` calls. Local objects too large for IR stack slots
  are diagnosed.
- **Alignment.** Access alignment comes from stack slots, global declarations,
  rounded local addresses, `__builtin_assume_aligned` and pointer offsets, and
  is written into `MemoryFlags`. Copy chunks get the alignment of their byte
  offset. Unknown pointers and unresolved block parameters have alignment one,
  and packed accesses are not strengthened. Alignment never implies `notrap`,
  immutability or alias freedom.

Target facts come from `Options::target`. All native targets are LP64 with
signed `char`. On Apple arm64, `long double` is `double` and `va_list` is a
`char *`. On x86-64 Linux, `va_list` is SysV's one-element `__va_list_tag`
array, and `long double`, `__float128` and `_Complex long double` are opaque
types: declarations parse, but values are errors. The C frontend does not
map `long double` to F80.

## Implementation notes

- **Computed goto.** `&&label` is the label's number in its function, starting
  at 1, and every `goto *p` jumps to one shared dispatch block that switches
  on that number; the switch becomes a jump table. When every computed goto in
  a function subscripts the same immutable, nonvolatile pointer table, the
  frontend switches on the subscript instead. The table must have a supported
  constant layout, at most 4096 entries and at most twice as many entries as
  address-taken labels; zero destinations trap. Other forms keep the numeric
  lowering for the whole function. Numbers work in static tables, and
  `&&a - &&b` offsets work.
- **Atomics.** `_Atomic(T)` is a separate type (`Ty::Atomic`) for native-width
  integers, pointers, float and double. Reads are atomic loads, and
  assignment, compound assignment and `++` are atomic stores or
  read-modify-writes, all seq_cst. GCC's `__atomic_*` and `__sync_*` and
  Clang's `__c11_atomic_*` builtins become IR atomic instructions;
  `compare_exchange` stores the old value back to `*expected` only on failure.
  Float and double operations use integer representations and CAS loops.
  Generic buffer operations support naturally aligned 1, 2, 4 and 8-byte
  objects. Atomic pointer `++` and `--` scale by the pointee size; pointer
  `+=` and `-=` are unsupported.
- **`__auto_type`** declares one variable with a plain identifier and an
  initializer, following
  [GCC's rules](https://gcc.gnu.org/onlinedocs/gcc/Typeof.html). The name
  enters scope after the initializer. Bit-fields and incomplete values cannot
  supply the type, and inference is not allowed in parameters, members,
  typedefs or type names.
- **`__builtin_debugtrap()`** emits the resumable `debugtrap` intrinsic
  (`brk #0xf000` on Apple ARM64, `int3` on x86-64). Code after it stays
  reachable; without a debugger it normally raises SIGTRAP.
- **Constants** are `i64`s, sign-extended to wider types, so an unsigned
  64-bit constant above `i64::MAX` converted to `__int128` is extended at run
  time rather than folded.

## Boundaries

Support for a syntax or attribute does not imply every GCC extension or every
placement is supported. Unknown attributes can still be accepted without
semantic effect; code must not depend on that behavior. `naked` and `interrupt`
metadata do not establish executable entry/return support.

Multidimensional automatic block VLAs, VM typedefs and pointers, dependent
parameter bounds and prototype-only `[*]` have a bounded implementation;
selected goto, switch and alloca combinations are diagnosed. Float and double
complex values support a bounded arithmetic and ABI subset; multiplication and
division call the target's compiler-runtime helpers, and extended complex
types and complex varargs are diagnosed. Old-style definitions support
explicitly typed integer, float, double and pointer parameters. Nested
functions, `_Atomic` structs, generic atomics outside the native size subset,
block literals and the Blocks runtime are unsupported.

Keel does not ship a complete system SDK, libc or compiler runtime. Vendor
intrinsic headers are subsets. Kernel acceptance uses pinned, preprocessed
64-bit inputs and an explicit feature policy; it is not a general Kbuild driver.

## References

- [Language extensions and layout](extensions.md), including [library builtin policy](extensions.md#c-library-memory-builtins).
- [Function attributes and optimization policies](../../compiler/function-attributes.md).
- [AArch64 inline assembly](../../targets/aarch64/c-inline-asm.md) and [x86 inline assembly](../../targets/x86_64/c-inline-asm.md).
- [x86 intrinsics](../../targets/x86_64/intrinsics.md) and [Wasm frontend](../../targets/wasm/c-frontend.md).
- [Linux integration](../../integrations/linux-kernel.md) and [application tests](../../../suite/README.md).

Tests in `crates/keel-c/tests/` define the accepted and diagnosed cases. Test
programs use both external Clang preprocessing and Keel's native
preprocessing, compile with and without optimization, and must behave like
Clang. The [suite](../../../suite/README.md) builds Lua, SQLite and QuickJS.
