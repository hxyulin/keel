# C language extensions

This page states the GNU and Clang extensions the C frontend implements and
their limits. [C frontend](README.md) covers the overall language support.

<a id="c-library-memory-builtins"></a>
## Standard-library builtins

Hosted native C compilation recognizes direct calls to canonical external
`memcpy`, `memmove` and `memset` declarations as memory intrinsics, and calls to
`sqrt`, `sqrtf` and `sqrtl` as the IR `fsqrt` operation. The library
API's `compile` and `compile_with_binding` entry points use this hosted policy.
`compile_with_policies` accepts `LibraryBuiltins` for explicit control. The bounded
Wasm profile remains freestanding and does not recognize ordinary library calls.

Recognition uses declaration identity and types after parsing the complete
translation unit. All declarations must retain the canonical native LP64
prototype, including pointer qualifiers and `unsigned long` size. `sqrtl` is
recognized only on Darwin, where `long double` is `double`. Typedef aliases
and top-level parameter `restrict` do not prevent recognition. Definitions,
static or weak bindings, renamed linker symbols, multiple C names for one linker
symbol, incompatible declarations and missing prototypes retain ordinary call
behavior. A later definition or incompatible declaration also prevents earlier
calls from acquiring builtin semantics.

Parentheses and taking a known function designator's address do not hide its
identity: `(memcpy)(...)` and a direct `(&memcpy)(...)` can be recognized. Identity-preserving casts may also be recognized. Calls through pointer objects
or parameters, and casts that change the function prototype or obscure the
designator, retain their ordinary call form.
Taking a function's address still produces the library function's address.

`-fno-builtin` disables ordinary-call recognition. The bounded per-function forms
are `-fno-builtin-memcpy`, `-fno-builtin-memmove`, `-fno-builtin-memset`,
`-fno-builtin-sqrt`, `-fno-builtin-sqrtf` and `-fno-builtin-sqrtl`.
`-ffreestanding` sets `__STDC_HOSTED__` to zero and disables ordinary recognition;
`-fhosted` restores hosted preprocessing and global recognition. `-fbuiltin`
reenables global recognition without changing `__STDC_HOSTED__`. Later global
flags win; per-function exclusions remain active across global toggles.
Explicit `__builtin_memcpy`, `__builtin_memmove` and `__builtin_memset` retain
their existing semantics regardless of these ordinary-call policies.

Arguments are evaluated once even for zero-length operations. Small fixed-size
intrinsics may become direct loads/stores; larger or dynamic operations can call
the linked system libc. Keel does not supply a libc implementation. Programs that
require interposition or custom ordinary memory-function behavior must disable
recognition. Optimizer passes rely on frontend intrinsic provenance rather than
matching arbitrary linker names.

Darwin's libm never sets `errno`, so a recognized square root there is only
`fsqrt`. Linux libm sets `errno` to `EDOM` for negative inputs, so on Linux a
recognized square root computes `fsqrt` for `x >= 0` and calls the library for
negative and NaN inputs. This matches Clang's default `-fmath-errno` code. Keel
has no `-fno-math-errno`.

### Explicit memory builtins

`__builtin_memcpy`, `__builtin_memmove` and `__builtin_memset` lower to the IR
`memcpy`, `memmove` and `memset` intrinsics without a library declaration, and
`__has_builtin` reports them. With an ordinary library declaration in scope,
the builtin name also works as a function designator (address taking,
`(__builtin_memcpy)(d,s,n)`), which becomes an ordinary call.

Lengths convert to `size_t` and the `memset` fill to `int`, truncated to the
IR's `i8`. All three arguments are evaluated once, left to right, even at zero
length, and the expression returns the destination. `memcpy` requires disjoint
regions; `memmove` allows overlap. A zero length adds no runtime pointer check.
Non-pointer arguments other than null constants, function pointers, const
destinations and volatile pointees are diagnosed; an explicit `(void *)` cast
may drop qualifiers. Small constant lengths may become loads and stores in the
[optimizer](../../compiler/optimizer.md#small-memory); other operations call
libc.

`__builtin___memcpy_chk`, `__builtin___memmove_chk` and `__builtin___memset_chk`
emit `__memcpy_chk`, `__memmove_chk` or `__memset_chk` calls with all four
arguments, even when a constant bound is smaller than the length. Their
prototypes are known without a declaration, conflicting declarations and
`returns_twice` are diagnosed, and `__has_builtin` does not report them.
Linking needs a runtime that provides the checked symbols. The macOS SDK's
`secure/_string.h` gates its fortify macros on `__has_builtin`, so those names
stay ordinary library calls under Keel's preprocessor. Semantics follow
[C11 N1570](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
sections 7.24.2 and 7.24.6.1 and GCC's
[library builtins](https://gcc.gnu.org/onlinedocs/gcc/Library-Builtins.html)
and [object size checking](https://gcc.gnu.org/onlinedocs/gcc/Object-Size-Checking.html).

### Other builtins

Supported builtins include `offsetof`, `types_compatible_p`, `choose_expr`,
`constant_p`, variadic access, `bswap16/32/64`, `clz/ctz/popcount` and their
`l`/`ll` forms, floating-point classification, `fabs`, `alloca`,
`frame_address(0)`, `debugtrap` and the overflow builtins below. A
`__builtin_` name with a declared library equivalent can also resolve to that
function; this is not blanket support for every builtin. Some builtins have
deliberately limited behavior:

- `__builtin_expect` and `__builtin_expect_with_probability` return their
  first operand converted to `long` and evaluate each runtime operand once.
  The probability must be a finite constant in [0, 1]. No branch-likelihood
  information is kept.
- `__builtin_assume_aligned` returns its pointer as `void *` without changing
  or checking it. The alignment must be a nonzero power-of-two constant, and
  the optional offset is evaluated once. Later loads and stores through that
  SSA pointer carry the alignment; a constant offset reduces it and an unknown
  offset removes it, as in
  [GCC's documentation](https://gcc.gnu.org/onlinedocs/gcc/Other-Builtins.html).
- `__builtin_constant_p` recognizes integer expressions the frontend can
  evaluate; it does not ask the optimizer.
- `__builtin_object_size` and `__builtin_dynamic_object_size` return the
  unknown-size value. Fortified builtins other than the three memory functions
  above become plain library calls and lose the bounds check.
- `__builtin_unreachable` lowers to a trap, like `__builtin_trap`, and adds no
  optimizer assumption.

GNU `packed` and `aligned` control aggregate and member layout. `aligned` and
C11 `_Alignas` also set local, static, global and thread-local object
alignment, with power-of-two requests up to 4096. `_Alignas(0)` has no effect,
and nonzero `_Alignas` cannot weaken natural alignment. Explicit member
alignment overrides packing, and `#pragma pack` caps member alignment.
Alignment-changing typedef attributes, alignment attributes on functions,
parameters and type names, and by-value aggregates aligned above 16 bytes are
diagnosed. On x86-64, by-value aggregates with unaligned fields are also
diagnosed. Native packed integer bit-fields access only the bytes that contain
the field, keep neighboring bits and keep volatile flags on each byte access.
Zero-width fields raise record alignment on AArch64 Linux, following its ABI.

<a id="c-cleanup-jumps-cleanup-aware-jumps"></a>
## Cleanup-aware jumps

A cleanup local has a declaration-lifetime region. Its region starts after its
initializer (or its declaration without an initializer) and ends at the end of
its block. Regions nest in declaration order, including declarations in the same
block. A label records the regions containing that source position.

A direct jump may retain a prefix of its source regions. It calls callbacks for
the remaining suffix in reverse declaration order before branching. A backward
jump across a declaration therefore cleans up the old object before executing
its declaration again. A jump forward over a cleanup declaration is rejected,
as is entry from a sibling block into a cleanup lifetime. This follows Clang's
checked entry rules. Entry into uninitialized cleanup objects is outside this
contract. No runtime initialized flags suppress callbacks.

In functions containing cleanup locals, statement expressions form additional
entry barriers. Jumps may leave them or stay within them, but cannot enter them from outside. Their internal labels and
cleanup declarations participate in the same analysis as ordinary statements.

Literal `goto *&&label` follows the direct-jump rules. Dynamic computed gotos
use every address-taken label as a possible destination. All such targets must
have exactly the source cleanup and statement-expression regions. Clang rejects
dynamic indirect edges that bypass initialization or teardown. The existing
shared computed-goto dispatch remains valid under this restriction.

Asm goto writes its outputs before dispatch. Its possible targets must have
exactly the source cleanup and statement-expression regions, following Clang's
checked asm-goto rules. Fallthrough and taken successors retain those lifetimes
until their ordinary scope exits. The existing backend contract needs no change.

All goto forms in functions with variably modified types remain rejected.
Stack-save tokens require a separate dominance and lifetime contract. Nonlocal
exits and returns-twice calls also retain their existing rejection.

The parser checks all cleanup jump edges, including in unused inline bodies.

GCC 14.4 skips the inner cleanup for a literal `goto *&&label` that leaves a
cleanup lifetime; Clang 21.1.8 and Keel run it.

The [GCC cleanup attribute contract](https://gcc.gnu.org/onlinedocs/gcc-14.4.0/gcc/Common-Variable-Attributes.html) specifies callbacks on scope exit. [Clang cleanup lowering](https://clang.llvm.org/docs/LLVMExceptionHandlingCodeGen.html) describes cleanup-aware direct edges.

## Empty aggregates

On x86-64 SysV, a complete, fieldless, alignment-one struct or union has size
zero ([GCC documentation](https://gcc.gnu.org/onlinedocs/gcc/Empty-Structures.html))
and contributes no argument or return payload. It uses no register or stack
argument slot, and an empty return has no hidden result pointer, so
`f(long, struct Empty, long)` passes its two integers in RDI and RSI, as GCC
and Clang do. Other targets and other empty shapes are diagnosed.

Argument expressions and calls producing empty values still execute, and a
returned empty expression is evaluated before exit actions. An omitted
parameter gets an ordinary local object, and an omitted result gets a local
object address; each has at least one backing byte that is never an ABI
payload. Unrelated empty objects need not have distinct addresses. IR `ByVal`
and `Sret` layouts stay nonempty.

Empty values may appear among the fixed parameters of a variadic function.
Aggregates in the variadic tail, aggregate `va_arg`, unprototyped aggregate
arguments, Microsoft ABI calls and records with nonzero size but no payload
are rejected.

<a id="c-lifecycle-c-executable-lifecycle-and-compiler-emission"></a>
## C executable lifecycle and compiler emission

Keel supports GNU `constructor`, `destructor` and `used` (including GNU
underscore spellings) on the two native C targets. Lifecycle callbacks must be
defined in the translation unit, have a `void(void)` prototype, and return
normally. Parameters, variadics, missing definitions, incompatible return-type
redeclarations, `noreturn` and `returns_twice` are diagnosed. Binding is merged
over the whole translation unit, including attributes after a definition.
Lifecycle annotations on data, typedefs, members, parameters and automatic locals diagnose.
A function may be both a constructor and a destructor.

Only the default priority is supported: no argument or an integer constant
expression equal to 65535. Every other priority is diagnosed on both targets,
even in unused declarations. Default-priority constructors execute in link input
order; within a translation unit, registration follows definition order, even
when forward declarations appear in a different order. Destructors execute in
reverse registration/array order. Programs must not assume a portable relative
order among equal-priority callbacks, especially when mixing producers. This is not general GNU priority support.

Lifecycle definitions are compiler emission roots, including otherwise uncalled
static functions. `used` similarly forces function/static-storage object emission
and preserves a used function's C calling convention. It does **not** request
linker retention, extract an archive member, or promise `no_dead_strip` or
`SHF_GNU_RETAIN`; see [custom sections and `retain`](#c-custom-sections-and-linker-retention). Lifecycle
array references keep their callbacks reachable in included objects; system
Darwin linking is tested with `-dead_strip`. An archive still needs ordinary
symbol demand (or an explicit linker whole-archive option) to include its member.
Registration relocations and Darwin callback addresses reference the bound
symbol, preserving ordinary weak override/interposition behavior.

<a id="c-lifecycle-darwin-arm64"></a>
### Darwin arm64

Constructors use pointer-aligned `__DATA,__mod_init_func` with
`S_MOD_INIT_FUNC_POINTERS`; dyld runs them before `main`. The native linker uses
`LC_MAIN`, so ordinary return from main goes through libSystem termination.

Clang lowers a static destructor to a `__mod_init_func` thunk that calls
`__cxa_atexit` with `__dso_handle`. The native linker does not synthesize
`___dso_handle`, so mixed input containing such a thunk is rejected as an
undefined symbol; system linking accepts it. Keel instead uses libSystem
`atexit` for an **executable-only** subset: a private constructor thunk
registers each destructor's bound address. It requires linking
libSystem and successful libc registration; registration exhaustion is not
handled specially. A compatible ordinary external `atexit` declaration/call is
allowed, including harmless function attributes such as `noinline`. The runtime
interface must have the prototype `int atexit(void (*)(void))`. A conflicting or
weak interface or a definition of that runtime symbol in the destructor's
translation unit is diagnosed before pruning, including unused declarations and
asm aliases naming `_atexit`. C-level
callback pointer types are checked before IR erases them to `ptr`. Return from
main and `exit` run destructors. `_Exit`, `_exit`, abort, signals, nonlocal
unwinding, DSO output/unload and freestanding/static startup
are outside the promise.

<a id="c-lifecycle-elf-x86-64"></a>
### ELF x86-64

Constructors and destructors use pointer-aligned `.init_array`/`.fini_array`,
`SHT_INIT_ARRAY`/`SHT_FINI_ARRAY` and `SHF_ALLOC|SHF_WRITE`. The native linker
retains them and emits dynamic array tags for dynamic output. Dispatch requires
the loader/libc startup and shutdown protocol described in [ELF linker](../../linking/elf.md#executable-initialization-and-termination).
A syscall-only `_start` does not arrange libc lifecycle dispatch; static startup
must explicitly invoke the arrays. Keel supplies no CRT objects, libc driver
startup, DSO output or legacy `.ctors`/`.dtors` conversion.

<a id="c-packed-enums-packed-enum-layout"></a>
## Packed enum layout

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

Enum type identity is not separate from its integer representation. Ordinary
small enums use `int`. Explicit enum alignment, fixed-type forward declarations
and enum representations wider than 64 bits reject. Existing ordinary
incomplete-enum extensions use `int`; a later definition that would change
that representation rejects rather than changing previously emitted types.

`packed_bitfields/packed_enums.rs` checks width boundaries and signedness on native targets and
the private Wasm32 layout.

## restrict

The frontend retains `restrict`, `__restrict`, and `__restrict__` in type
sugar, including pointer typedefs. A function definition emits
`restrict_params(0, 1)` for its own top-level restricted pointer-to-object
parameter objects. Indices include any ABI hidden result parameter. Prototype
qualifiers do not supply facts to a later unqualified definition, declarations,
call signatures, or indirect calls. Local/block restrict, inner pointer layers,
function pointers and array-bracket restrict supply no optimization facts.
Not every invalid C qualifier is diagnosed.

`Function::restrict_params` is a definition-only access contract for the whole
invocation. If memory accessed based on a listed parameter is modified during
that invocation, accesses to that memory must be based on that parameter. It
does not assert distinct pointer values: two parameters may read the same
unmodified object. It supplies no nonnull, notrap, alignment, immutable-memory,
or type-based alias fact. The semantics follow the modified-object access rule
in [C11 N1570 section 6.7.3.1](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
and the corresponding parameter access distinction in
[LLVM's noalias documentation](https://llvm.org/docs/LangRef.html#noalias).
C restrict also constrains restricted-pointer assignments; the initial consumer
uses no such assignment to infer aliasing facts.

The verifier checks unique in-range normal-purpose pointer indices; existing
entry/signature verification checks matching entry parameters. A C frontend must establish C pointee eligibility because opaque IR pointers
cannot express it. An LLVM producer may assert the same modified-object access
rule directly from a definition's normal-pointer `noalias` parameter, without
recovering C types. This mapping does not transport declaration, call-site or
return-value noalias, and it adds no pointer inequality, nonnull or readability
promise. Text preserves contract order deterministically. Whole-function
clones retain the contract. Inlining copies instructions and blocks, not the
callee's definition contracts, so callee entry remapping does not give caller
arguments new facts. Existing caller contracts retain their entry indices.
Transformations that change entry arguments must remap or drop these contracts.

[`memory-forward`](../../compiler/optimizer.md#memory-forward) and
`load-reuse` use these contracts to keep a load across a store when both
addresses are distinct eligible restricted entry parameters.

## Advisory prefetch

`__builtin_prefetch(address[, rw[, locality]])` accepts a pointer address,
an optional integer constant `rw` in 0..2, and an optional integer constant
`locality` in 0..3. It returns `void`. Keel evaluates address side effects
and volatile reads once, but emits no prefetch instruction and does not
dereference the resulting address. The optional controls are validated, then
discarded. This is an advisory hint, not a memory access or synchronization
operation. See [GCC's builtin contract](https://gcc.gnu.org/onlinedocs/gcc/Other-Builtins.html#index-__builtin_prefetch).

<a id="c-runtime-offsetof-runtime-offset-designators"></a>
## Runtime offset designators

`__builtin_offsetof` accepts integer expressions in array member subscripts.
This includes nested arrays, anonymous members, union members and flexible
array members:

```c
struct Entry { long key; long value; };
struct Table { char tag; struct Entry entries[8]; };

unsigned long value_offset(int index) {
    return __builtin_offsetof(struct Table, entries[index].value);
}
```

The result uses the target's `size_t` representation: unsigned long is 64 bits
on native targets and 32 bits in the bounded Wasm model. Index arithmetic wraps
in that unsigned domain, including negative and large indices. Each evaluated
index expression runs once. Zero-size elements still evaluate their indices.
Index expressions retain their ordinary reads and side effects; no object of
the designated type is created or dereferenced.

Designators must name actual members and array elements. Pointer-member
traversal, bitfields, missing members and noninteger indices are rejected.
Constant designators remain integer constants for enums, static initializers
and static assertions. A runtime designator does not become an ICE merely
because its numerical result can be predicted while discarding side effects.

`__builtin_constant_p` stays an unevaluated, conservative query and does not
evaluate a side-effecting offset index, unlike Clang.

`cargo test -p keel-c --test runtime_offsetof` checks native layouts, private
Wasm arithmetic and execution, static rejection, index side effects and IR
round trips.

<a id="c-sections-retain-c-custom-sections-and-linker-retention"></a>
## C custom sections and linker retention

Keel supports GNU `section("name")` and no-argument `retain`, including
`__section__` and `__retain__`, on function definitions and
static-storage object definitions (including `_Thread_local` / `__thread`) on both native targets. File-scope objects,
tentative definitions, and static locals are supported. Attributes merge over
the entire translation unit, including declarations following definitions.
Conflicting section redeclarations diagnose. `retain` requires a definition
in the same translation unit. `section` on an external declaration allocates no
storage and does not affect imported call contracts; a later definition
inherits it and gets the ordinary storage checks. Automatic locals, parameters,
members, typedefs, type positions and COMDAT/`weak_odr` combinations diagnose.
Ordinary `weak` definitions retain their binding and symbol identity.

`section` specifies placement and does not itself force compiler emission.
`used` remains a compiler emission root only, on both targets. `retain` is both
a compiler emission root and a request to prevent system-linker garbage
collection of that definition. It preserves a function's ordinary C calling
convention. Optimization, IR text roundtrips and both native object paths
preserve placement and retention. The compiler does not infer an archive
extraction request from either attribute: a member still needs ordinary symbol
demand or a linker whole-archive option. Weak definitions can still be
superseded by strong definitions. Retention does not strengthen binding.

<a id="c-sections-retain-elf-x86-64"></a>
### ELF x86-64

An explicit name contains 1–128 ASCII letters/digits or `._-$`. Flags, section
types, group arguments, injected assembly and segment syntax are not accepted
in the C string. Safe arbitrary code, writable initialized data, readonly data,
and writable all-zero sections are supported. Code uses `SHF_ALLOC|SHF_EXECINSTR`;
writable data uses `SHF_ALLOC|SHF_WRITE`; readonly bytes use `SHF_ALLOC`.
All-zero writable data uses `SHT_NOBITS`, except conventional `.data`, `.data1`,
`.init.data`, `.exit.data`, `.meminit.data`, `.ref.data` and their dotted suffixes,
which use `SHT_PROGBITS`. Other storage also uses `SHT_PROGBITS`.

Unplaced readonly data containing addresses keeps writable relocatable-constant
storage in `.data.rel.ro`. Explicit `.rodata`, `.rodata1`, `.init.rodata`,
`.exit.rodata`, `.meminit.rodata`, `.ref.rodata` and dotted family suffixes use
`SHF_ALLOC` without `SHF_WRITE`, including pointer relocations. This is the static
readonly contract qualified against GCC/Clang with `-fno-pic -fno-pie` for the
kernel corpus. PIC-mode differences and dynamic imports into readonly sections
are not covered. The same explicit-storage rules apply to AArch64 ELF. An arbitrary custom name does not arrange a system linker's RELRO
segment policy.

Section names with standard placement implications (`.text`, `.data`, `.rodata`,
`.bss` and their dotted suffixes) must match the storage class. Runtime arrays,
notes, debug/unwind metadata, dynamic-loader sections, groups and GNU
special sections (including `.gnu.*` and `.gnu_*`) are reserved and rejected. C cannot request merge/string,
COMDAT, link-order, compressed or other special section forms. The
`.keel.retain.` namespace is reserved for compiler-generated names.

Retained input sections add `SHF_GNU_RETAIN` (`0x200000`), spelled `R` in assembly.
The object writer marks GNU OSABI, which GNU ld requires to interpret the
OS-specific flag. Unplaced retained definitions receive separate
`.text/.data/.bss/.rodata/.data.rel.ro.keel.retain.<symbol>` sections so retention
does not accidentally root unrelated definitions in a default input section.
ELF assembly distinguishes retained and ordinary input contributions with the
same name, matching GNU as. Explicitly sharing a retained section retains
that whole retained input section, not
individual bytes or symbols. A compiler-only `used` definition can therefore
survive GC when it shares an input section with a live root; the attribute
itself still makes no linker-retention request.

All definitions explicitly sharing a named section in one translation unit
must have the same effective storage class after
explicit-name normalization, and the same retention request on ELF. Thus plain
constants and relocated constants can share a conventional readonly section,
and zero/nonzero objects can share conventional initialized data sections.
Other zero-fill/initialized conflicts remain diagnosed.
Conflicting requests diagnose before pruning, including unused definitions.
This avoids silently broadening retention or changing section type.
Data alignment must be a power of two up to 4096 bytes; sections preserve the
maximum alignment requested by their members. Function entries keep the
backend's normal alignment.

The native ELF linker accepts and preserves `SHF_GNU_RETAIN` and GNU OSABI in
its executable section headers. Native `--gc-sections` collects whole input
sections and roots retained contributions; without that option, selected supported
sections survive. See [native GC policy](../../linking/elf.md#opt-in-section-garbage-collection)
for roots, unwind handling and limits. ELF COMDAT selection is
a separate earlier step: supported assembler-authored groups coalesce before
layout, while C attributes and IR `weak_odr` plus section/retain combinations
continue to diagnose. See [native COMDAT scope](../../linking/elf.md#comdat-selection-before-native-garbage-collection).

Ordinary `.init.*` and `.fini.*` names are not special ELF sections. Known
kernel text, data and rodata lifetime sections check their storage classes.
Zero-initialized `.init.data`, `.exit.data` and `.meminit.data` objects occupy
PROGBITS bytes and can share a section with initialized data. Special ELF
names such as `.init_array`, TLS and COMDAT keep their own checks.

<a id="c-sections-retain-darwin-arm64"></a>
### Darwin arm64

The spelling is exactly `"segment,section"`, with each component containing
1–16 ASCII letters/digits/underscores. Only `__TEXT` and `__DATA` are supported.
Functions require `__TEXT` and emit regular sections with instruction attributes.
Mutable objects and readonly objects containing relocations require `__DATA`;
readonly objects without relocations may use `__TEXT` or `__DATA`. Conventional
`__text`, `__data` and `__const` names must match their storage class. Special
runtime, Objective-C, unwind, literal, stub/pointer and ordinary zero-fill section
forms are rejected. Custom all-zero objects use regular data with actual zero
bytes, rather than claiming arbitrary Darwin zerofill support. The same-class
sharing and data alignment restrictions above also apply.

`retain` emits symbol-level `.no_dead_strip`, represented by `N_NO_DEAD_STRIP`
(`0x20`) in the MH_OBJECT nlist descriptor, preserving `N_WEAK_DEF` (`0x80`)
independently. It does not set `S_ATTR_NO_DEAD_STRIP` on a shared default section.
The object-only descriptor is consumed at link time and is not copied into an
executable nlist. Native `--dead-strip` / `-dead_strip` roots the containing
input section; symbol atomization remains deferred, so unrelated definitions
sharing that section survive together. The option is off by default. See
[native dead stripping](../../linking/macho.md) for root and unwind policies.

### Registration tables and cross-TU sharing

`crates/keel-c/tests/sections.rs` links registration tables built from three
translation units in interleaved order. Darwin boundary names
(`section$start$...`, `section$end$...`) are raw assembler labels without a C
underscore, so C cannot declare them directly. Section bounds include padding
between inputs, so a table is a dense C array only when its record size matches
the strongest input alignment. Storage-class sharing rules apply within a
translation unit. Native ELF linking also rejects identifier-named sections
whose type, flags or retention differ across translation units; Mach-O regular
sections cannot carry all of those distinctions. The ELF validator reserves
`.gnu.*` and `.gnu_*` names because `.gnu_debuglink` and related sections have
special debug consumers.

### Thread-local storage

`section` and `retain` apply to TLS definitions. Every declaration of an object
must agree on TLS storage. Block-scope TLS needs `static` or `extern`, and
functions cannot be TLS. Attributed TLS imports without a definition in the
translation unit, weak TLS and TLS COMDAT are diagnosed; ordinary TLS imports
work.

`section` places the template storage, never the Darwin TLV descriptor. The
initializer selects initialized or zero-fill placement, including an explicit
all-zero initializer. Named TLS sections cannot share with ordinary storage or
mix initialized and zero-fill templates in a translation unit. `.tdata` and
`.tdata.*` need nonzero initialized TLS, `.tbss` and `.tbss.*` need all-zero
TLS, and `.data`, `.bss`, `.text` and `.rodata` reject TLS.

- ELF emits `SHF_ALLOC | SHF_WRITE | SHF_TLS` (`0x403`) with `SHT_PROGBITS`
  for initialized templates and `SHT_NOBITS` for zero-fill, plus
  `SHF_GNU_RETAIN` when retained. Unplaced retained TLS uses
  `.tdata.keel.retain.<symbol>` or `.tbss.keel.retain.<symbol>`. Defined
  accesses use local-exec `R_X86_64_TPOFF32` and imports use initial-exec
  `R_X86_64_GOTTPOFF`. The native ELF linker rejects TLS, including TLS in a
  losing COMDAT group, and asks for an external ELF linker.
- Darwin templates require `__DATA` and get `S_THREAD_LOCAL_REGULAR` (`0x11`)
  or `S_THREAD_LOCAL_ZEROFILL` (`0x12`). `__thread_data` and `__thread_bss`
  need the matching class, and other `__thread*` names are reserved. The public
  symbol stays in `__DATA,__thread_vars`, pointing at a `<symbol>$tlv$init`
  template whose name gets extra `$` characters if it would collide with a
  declared symbol. `retain` marks both descriptor and template
  `N_NO_DEAD_STRIP`. The native linker rejects cross-TU TLVP references to
  ordinary definitions.

Template alignment is kept in objects and linked layout, but dyld allocates
per-thread copies with malloc and cannot honor alignment above its ordinary
guarantee; see [`tools/tls-alignment`](../../../tools/tls-alignment/README.md).
Clang needs explicit `thread_local_regular` or `thread_local_zerofill` in its
section spelling; Keel derives the TLS section types itself. DWARF uses the
descriptor address with `DW_OP_form_tls_address` on Darwin and
`R_X86_64_DTPOFF64` with `DW_OP_GNU_push_tls_address` on ELF. The section
types and dyld behavior follow the
[ELF TLS ABI](https://www.akkadia.org/drepper/tls.pdf), Apple's
[`loader.h`](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/loader.h)
and [dyld TLV implementation](https://github.com/apple-oss-distributions/dyld/blob/main/libdyld/ThreadLocalVariables.cpp).

```sh
cargo test -p keel-c --test sections
cargo test -p keel-ir --test sections -p keel-toolchain --test foundations \
  -p keel-llvm --test export
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked \
  --target-dir target/keel-cranelift
```

<a id="c-stack-observers-c-frame-and-stack-observers"></a>
## C frame and stack observers

Keel supports bounded GNU frame queries and read-only x86 stack-pointer
bindings through IR frame and stack observers.

<a id="c-stack-observers-frame-addresses"></a>
### Frame addresses

`__builtin_frame_address(depth)` accepts checked integer constant expressions
from 0 through 64 on native LP64 targets. Depth zero returns the current frame
record, preserving the existing behavior. Each additional level loads the
saved caller frame pointer from the preceding record.

Nonzero depths require readable caller records using the target's saved-frame
ABI. Omitted frames, tail calls and invalid chains can make a query unsafe;
Keel does not manufacture records or replace invalid memory accesses with null.
The limit bounds emitted loads, not the number of records known to exist.
Wasm and runtime, negative or overflowing depth expressions are diagnosed.
The [GCC contract](https://gcc.gnu.org/onlinedocs/gcc/Return-Address.html)
also makes nonzero-depth queries dependent on the actual calling environment.

<a id="c-stack-observers-return-addresses"></a>
### Return addresses

On x86-64 Linux, `__builtin_return_address(depth)` accepts constant depths
0 through 64. It follows that many saved frame links and reads the selected
continuation slot. Nonzero depths require readable caller frame records.
Runtime, negative and overflowing depth controls reject.
`__builtin_extract_return_addr` and `__builtin_frob_return_addr` are pointer
identities on this target and evaluate their arguments once. Other targets
remain rejected because their stored return-address representations may differ.
The corresponding builtin capability queries are enabled only on x86-64 Linux.

<a id="c-stack-observers-read-only-stack-pointer-bindings"></a>
### Read-only stack-pointer bindings

The x86-64 Linux frontend accepts this file-scope declaration:

```c
register unsigned long current_stack_pointer asm("rsp");
```

Every reference samples the current stack pointer. It creates no global data
object or symbol named `rsp`. Reads remain separate observations across dynamic
stack changes. Ordinary writes, increments and address-taking are rejected.
The exact `"+r"` asm operand has the separate balanced-call contract below.
Other global registers, types, qualifiers, initializers and declaration
attributes remain unsupported. Matching declarations may repeat; conflicting
ordinary declarations are rejected. Ordinary local and parameter `register`
storage keeps its existing behavior.

Linux's `ASM_CALL_CONSTRAINT` uses a read/write `"+r"` operand naming this
binding; see the [balanced asm-call contract](../../targets/x86_64/c-inline-asm.md#balanced-stack-calls).

<a id="c-stack-observers-inlining"></a>
### Inlining

The C frontend's mandatory inline phase rebinds frame and stack observations to
the emitted caller. The ordinary IR inliner refuses frame and stack observers,
and stack allocation and restoration stay outside mandatory inlining.

<a id="c-storage-metadata-c-storage-and-function-metadata"></a>
## C storage and function metadata

Array elements must be complete object types when their array is declared; a
record completed later cannot change an already-admitted array size. Scalar
extern declarations and pointers to incomplete records remain supported.
Native array and record sizes use checked byte arithmetic before enclosing types
or initializer offsets consume them. Declaration-only types and `sizeof` queries
may exceed the IR storage limit when their byte size remains representable.
Aggregate ABI layouts separately check their u32 element counts. Local and
temporary stack slots check their u32 byte capacity, including padding reserved
for over-aligned locals. Uses exceeding one of these limits return a diagnostic;
a representable type does not authorize a truncated ABI layout or allocation.
Wasm32 retains its stricter object-size bounds.

Bounded array initializers derive element positions as items are parsed. Empty
and sparse initializers do not allocate a child table for the declared bound,
including compound literals used only in unevaluated layout queries.

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
not support ELF note placement, and the LLVM and Cranelift exporters reject
custom sections. The native ELF linker rejects allocated producer notes such as
`.note.Linux`; use a system ELF linker for those objects.

Packed relocations are representable in ELF and Mach-O relocatable objects.
Darwin's default chained dyld fixups cannot encode an unaligned pointer location.
Executable runtime tests use `-Wl,-no_fixup_chains` to select classic fixups.
This is a linker format constraint and applies to Clang's equivalent object too.

`crates/keel-c/tests/sections/storage_metadata.rs` compares relocations, note sections
and function entry alignment with Clang.

## Extended floating types

Recent glibc headers declare GNU extended floating types even in programs that
never use them. On x86-64 Linux, `mode(TC)` on complex float, double or long
double declares complex binary128, with 32-byte size and 16-byte alignment, as
in GCC and Clang. Layout queries work, but evaluated values and any ABI use are
rejected before object emission, including binary128 members of by-value
aggregates. An unbound `_Float32` in a declaration reports
`unsupported extended floating type '_Float32'`; the Clang header path supplies
`typedef float _Float32`, which Keel accepts as `float`. GCC's builtin
`_Float32`, `_Float64`, `_Float32x`, `_Float64x` and `_Float128` identities,
binary128 arithmetic and its ABI are not supported, so GCC-preprocessed glibc
input fails at their declarations. Native GCC gives these types:

| Type | Size / align | Notes |
| --- | --- | --- |
| `_Float32` | 4 / 4 | distinct from `float` |
| `_Float64`, `_Float32x` | 8 / 8 | distinct from each other |
| `_Float64x` | 16 / 16 | x87 encoding, distinct from `long double` |
| `_Float128` | 16 / 16 | binary128, compatible with `__float128` |
| `TC` | 32 / 16 | compatible with `_Complex _Float128` |

`crates/keel-c/tests/fixtures/extended-float-layout-oracle.c` checks this
layout with native GCC and Clang. See
[GCC's floating types](https://gcc.gnu.org/onlinedocs/gcc/Floating-Types.html)
and the [AMD64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex).

<a id="c-weak-symbols-ordinary-c-weak-symbols"></a>
## Ordinary C weak symbols

Keel supports `__attribute__((weak))` (and `__weak__`) on external function
and object definitions, tentative object definitions, and undefined references.
The supported native targets are AArch64 Mach-O and x86-64 ELF. The attribute
must have no arguments. Static symbols, automatic variables, members, parameters,
typedefs and type names cannot carry it. Weak TLS is rejected. Hidden/internal
weak **definitions** work within a linkage unit; hidden/internal weak undefined
references are rejected. `weak_import`, `weakref`, aliases and ifuncs remain explicit errors.
Unknown `weak*` attribute forms are diagnosed rather than producing strong
symbols.

Binding is merged over the entire translation unit, including declarations
after references, initializers and definitions. Every emitted reference sees the
final binding. This deliberately supports late weakening where Clang warns and
ignores an attribute following a definition; Clang oracle tests put the same
attribute on an equivalent early declaration. Unused undefined declarations
continue to emit nothing. Externally visible weak definitions are emitted even
without references. Definition bodies remain interposable: no inlining, private
calling convention conversion, or memory-preservation proof may use them,
even when IR also supplies `noninterposable` or `alwaysinline`.

Ordinary `weak` is distinct from `weak_odr`: there is no ODR equivalence,
COMDAT promise, alias or resolver. Functions store ordinary weak binding in
`FunctionAttrs::weak`, shared by definitions, declarations and callee metadata;
objects use `DataObject::weak`. Text examples:

```text
decl @optional() -> i32 weak
decl data weak @optional_data
data @value = weak align 4 { i32 7 }
func @hook() -> i32 weak {
block0:
    v0 = iconst.i32 7
    return v0
}
```

Function `weak` is printed with other function attributes before `local`,
`weak_odr` and `hidden`. Object `weak` follows `local` and precedes `weak_odr`.
Verification and native codegen reject local/weak, weak/weak_odr and weak TLS
combinations. LLVM export and Cranelift reject ordinary weak modules, including
weak undefined function declarations, rather than silently strengthening them.
There is no whole-program IR linker or LTO weak resolution.

Both native object paths preserve binding in code and initializer relocations.
ELF uses `STB_WEAK`/`.weak`; Mach-O uses `N_WEAK_DEF`/`.weak_definition` and
undefined `N_WEAK_REF`/`.weak_reference`. Zero-initialized Mach-O weak objects
use section storage rather than common/zerofill declarations. Weak definition
addresses use the GOT, and x86 direct calls use the PLT relocation path, so
references to discarded weak bodies/objects bind to the selected symbol.
Strong definitions win in either input order; duplicate weak definitions
coalesce. Function/data pointer initializers and dynamically obtained addresses
agree. Missing weak function and data addresses are zero; guarded calls and
loads retain their runtime guards, including Boolean conversion.

Parser-time constant evaluation cannot assume a global address is nonzero
because a later declaration may weaken it. Static initializers are evaluated
after declaration merging and can use final strong binding to fold address
truth tests, including Boolean casts and conditional/logical expressions.
Weak-address truth tests in static initializers remain conservatively rejected.
Address constants remain available for pointer initializers. Runtime tests are
unaffected.

Archive behavior follows the existing native linker contracts: ELF undefined
weak references do not extract a member; an independent strong demand can
extract the member and satisfy them. Mach-O weak undefined references do extract
a matching archive member, matching Apple ld. An already satisfied weak
definition does not itself create fresh archive demand. No archive member is
loaded merely to search for a stronger replacement.

On Darwin, system linking an absent weak symbol requires an explicit allowance,
e.g. `clang objects.o -Wl,-U,_optional,-U,_optional_data`. This is also required
for the equivalent Clang-produced object with the installed Apple linker.
Keel's native Mach-O linker supports absent weak flat lookup directly; dyld
supplies zero when no process symbol exists. The runtime therefore still uses
libSystem/dyld and native code signing. An unguarded call/load through a missing
weak symbol has the ordinary invalid-null-address behavior.

<a id="generic-overflow-builtins-generic-integer-overflow-builtins"></a>
## Generic integer overflow builtins

Keel supports `__builtin_add_overflow`, `__builtin_sub_overflow`, and
`__builtin_mul_overflow` for its integer types through signed and unsigned
`__int128`, including mixed operand widths, mixed signedness, and destinations
narrower or wider than either operand. `__has_builtin` advertises these names.
The return type is `_Bool`.

The contract follows [GCC's generic overflow specification](https://gcc.gnu.org/onlinedocs/gcc/Integer-Overflow-Builtins.html):
compute the exact mathematical operation, convert and store the result in the
pointed-to integer type, and return whether that mathematical value is outside
that type's range. In particular, operands are not first converted to the
result type. For example, adding signed `-1` to unsigned `1` stores zero without
overflow, even when the destination is unsigned. Adding unsigned `UINT_MAX` to
signed `1` with an `int` destination stores the low bits and reports overflow.

Integer operands include boolean and enum values. The destination must point
to a non-const integer other than boolean or enum; typedefs and qualifiers are
resolved when checking this restriction. Volatile integer destinations retain
the volatile store. Atomic destinations, float and pointer operands, vectors,
and `_overflow_p` are outside this interface. The fixed-type `sadd`, `uadd`,
`ssub`, `usub`, `smul`, and `umul` families retain their existing conversions
and lowering.

The operands and the destination expression are evaluated once each, in
frontend order, before the store, and the store happens even when the result
is unused. A destination that aliases an operand sees the operand values
captured before the store. The lowering computes sign and unsigned magnitude
in an i64 or i128 work type, splits i128 multiplication into 64-bit limbs, and
compares the magnitude with the destination's range; it needs no wider IR type
or runtime helper.

<a id="gnu-vector-mode-gnu-c-integer-modes-and-vectors"></a>
## GNU C integer modes and vectors

Keel C implements a bounded GNU type-attribute subset on
`aarch64-apple-darwin`, `aarch64-unknown-linux-gnu` and
`x86_64-unknown-linux-gnu`. These attributes change
C type identity before layout, initialization, and IR generation. Unsupported
sizes and element types produce diagnostics; they never fall back to a scalar.
Double-underscore attribute and mode-name spellings are accepted.

<a id="gnu-vector-mode-integer-modes"></a>
### Integer modes

`__attribute__((mode(DI)))` on an integer variable, parameter, member, typedef,
or scalar type name selects a real integer type. Signedness comes from the
original integer type. The supported names and canonical signed types are:

| Mode | Bytes | Signed type on the supported LP64 targets |
| --- | --- | --- |
| QI, byte | 1 | signed char |
| HI | 2 | short |
| SI | 4 | int |
| DI, word, pointer | 8 | long |
| TI | 16 | __int128 |

Unsigned bases select the corresponding unsigned type. Alignment, conversion
rank, promotions, arithmetic and ABI behavior follow the selected type.
For example, `typedef unsigned int U __attribute__((mode(DI)));` is compatible
with `unsigned long`, occupies eight bytes, and performs unsigned 64-bit
arithmetic. Numeric conflicts in object, typedef and function redeclarations
are diagnosed. This does not replace the frontend's existing policy for all
unrelated C redeclaration categories.

Declaration-specifier attributes change the base before composing declarators;
final named-declarator attributes change their scalar subject. Attributes on
pointers/functions, `_Bool`, floating types, enum tags, and unsupported modes
(including vector machine modes and ordinary floating machine modes) are diagnosed.
The separate x86-64 Linux [TC subset](#extended-floating-types)
accepts `mode(TC)` on complex float/double/long double, canonicalizing to complex binary128
(32-byte size, 16-byte alignment) while rejecting evaluated values and ABI use.
Use `vector_size`, rather than a vector `mode`, for vectors.

<a id="gnu-vector-mode-six-lane-shapes-at-16-or-32-bytes"></a>
### Six lane shapes at 16 or 32 bytes

Supported elements are signed/plain/unsigned char, signed/unsigned short, int,
long and long long, and float/double. `_Bool`, `__int128`, `_Float16`, opaque
floating types, structs, pointers, and vector elements are unsupported.
`vector_size(16)` is supported on all targets. Bounded `vector_size(32)` is
supported only on x86-64, using the existing AVX ABI and backend feature gates.
Eight-byte vectors remain unsupported. The 16-byte shapes are `i8x16`, `i16x8`,
`i32x4`, `i64x2`, `f32x4`, `f64x2`; the 32-byte shapes are `i8x32`, `i16x16`,
`i32x8`, `i64x4`, `f32x8`, `f64x4`. C retains element signedness and nominal
integer identity. Size and natural alignment equal the selected byte width.
Lane zero is at the lowest address. Struct fields, arrays and pointer arithmetic use the complete size.

```c
typedef int V __attribute__((vector_size(16)));
typedef int __attribute__((vector_size(16))) *VP;
V add(V a, V b) { return a + b; }
int lane(V a, int i) { V b = {1, 2}; b[i] = a[i]; return b[i]; }
```

Attributes before the base, among declaration specifiers, and after a scalar
name are supported. For pointers, arrays and function returns put the attribute
in the declaration specifiers or use a vector typedef. A final attribute on a
pointer, array or function declarator is rejected, matching Clang. GNU type
attributes on tags or array-parameter qualifier positions are diagnosed.

Supported expressions:

- Braced lane initialization of globals and locals, zero-filled omitted lanes,
  compound literals, whole-vector initialization/assignment, conditional
  expressions with a scalar condition, pointers and memory loads/stores.
- Constant or dynamic lane reads, lane assignments and compound assignments.
  Constant out-of-range subscripts are diagnosed. Dynamic subscripts must be
  within the vector, as in C; no runtime bounds check is inserted. A lane is
  not addressable. Volatile lane access is diagnosed; whole-vector volatile
  loads and stores retain the existing memory flags.
- Matching-vector `+`, `-`, `*`; integer `&`, `|`, `^`, `~`, `<<`, `>>`;
  floating `/`; unary `+` and `-`; and matching-vector comparisons.
  Integer vectors keep their lane width without scalar integer promotions.
  Comparison results have signed integer lanes of the operand width, with
  all bits set for true and zero for false. Right shift follows element
  signedness. Shift counts must be valid C lane shift counts. The separate
  [bounded SSE2 wrappers](../../targets/x86_64/intrinsics.md#c-x86-intrinsics-bounded-sse2-packed-shifts) use
  architectural zero/sign-fill for oversized counts; they do not extend these
  ordinary C operators or add scalar broadcast shifts.
- Explicit equal-width casts between supported vectors, and between a 16-byte
  vector and signed or unsigned `__int128`, preserving all bytes. These are bit casts,
  not numeric lane conversions. The separate
  [SSE2 conversion wrappers](../../targets/x86_64/intrinsics.md#numeric-conversion-expansion)
  preserve architectural rounding/invalid/status behavior through dedicated IR.
- Bounded `__builtin_shufflevector` permutations with matching input/result
  shapes and constant in-range indices; see the
  [C x86 intrinsic subset](../../targets/x86_64/intrinsics.md) for the builtin and header contract.

Two vector operands must have the same C vector type; implicit conversions
between distinct vectors are diagnosed. The bounded scalar broadcasts below
also preserve that vector type. Integer vector division/remainder, floating bitwise operations, vector logical
operators/conditions, vector `!`, and reading vectors with `va_arg` are
unsupported. Initializer designators into vector lanes are unsupported.

The backend may expand an operation into scalar **lanes**, preserving the full
vector type, layout and ABI; that is distinct from ignoring the type attribute.
Debug information presents a vector as an array of its elements.

The [bounded SSE2 saturation wrappers](../../targets/x86_64/intrinsics.md#c-x86-intrinsics-bounded-sse2-saturation)
provide explicit signed/unsigned byte/word clamping on x86-64 using existing
typed architectural operations. Ordinary vector `+`/`-` retain their current
semantics; the wrapper subset adds no numeric conversions or new vector widths.

<a id="gnu-vector-mode-scalar-broadcasts"></a>
### Scalar broadcasts

For `+`, `-`, `*`, integer `&`, `|`, `^`, floating `/`, and the six comparisons,
one operand may be a scalar. Either operand order is supported, including
noncommutative operations. The scalar is converted to the vector element type
and replicated into every lane. Integer lanes do not undergo promotions;
comparison masks retain the existing signed all-one/zero lane type.
`+=`, `-=`, `*=`, integer `&=`, `|=`, `^=`, and floating `/=` accept a scalar
right operand under the same conversion checks. Each operand and each
compound-assignment lvalue is evaluated once. Vector widths, memory accesses,
volatile whole-vector flags, ABI and target feature requirements are unchanged.
This applies to 16-byte vectors on all supported targets and x86 32-byte vectors.

The accepted conversions are deliberately bounded and lossless:

- Integer scalars through 64 bits: the complete source type range must fit the
  lane type, or a supported constant expression must have a value within the
  lane's signed/unsigned range. Widening with unchanged signedness, conversion
  from unsigned to a wider signed type, and `_Bool` to integer are accepted. Runtime signed-to-unsigned conversion is
  diagnosed, even at equal widths; unsigned constants are interpreted as
  unsigned values, including `ULONG_MAX`.
- Floating scalars: `float` to `float`/`double`, and `double` to `double`.
  A finite constant `double` also converts to `float` when exactly representable,
  including negative zero. Runtime `double` to `float` is diagnosed.
- Cross-domain integer/floating broadcasts, 128-bit integers, extended FP,
  scalar shifts, and implicit scalar-to-vector assignment/initialization remain
  diagnosed. Use an explicit **scalar** cast to the element type to request a
  conversion before broadcasting, for example `v + (float)n` or `v + (int)n`.

Constant proofs cover numeric literals, scalar casts, unary/binary numeric
expressions, logical expressions and conditionals with supported numeric
operands. Expressions with i128, extended FP or pointer intermediates, comma
operators, calls or side effects cannot prove a narrowing conversion. This
avoids using the frontend's 64-bit constant evaluator to prove an unsafe
wide-to-narrow conversion. Runtime operands with an already safe type can
still contain calls or side effects.

This follows the safe-conversion rule in
[GCC's vector extension documentation](https://gcc.gnu.org/onlinedocs/gcc/Vector-Extensions.html)
for the accepted subset, without claiming all GNU scalar conversion cases.
Clang accepts some out-of-range integer constants with a warning; Keel
diagnoses them. The FP contract does not promise signaling-NaN payloads,
exception flags or traps.

```sh
cargo test -p keel-c --test gnu_vector_broadcast --test gnu_vector_abi \
  --test gnu_types --test x86_intrinsics
```
