# C language extensions

This reference describes current implementation contracts and limits. Use
[testing](../../development/testing.md) for validation commands; per-run results belong outside Git.

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

The parser validates all cleanup jump edges, including unused inline bodies.
Lowering uses the same destination-region table. Existing return, break,
continue, switch and ordinary block-exit lowering retains its reverse-order
exit-action stack. A jump emits actions without consuming that compile-time
stack, so sibling paths and later labels retain their own actions.

<a id="c-cleanup-jumps-compiler-witnesses"></a>
### Compiler witnesses

Apple Clang 21.0.0 rejects forward entry across a cleanup declaration and entry
into a GNU statement expression. A native witness with a backward jump across a
cleanup declaration records one callback per iteration. A literal computed jump
out of an inner cleanup block runs the callback before reaching the outer label. A
dynamic computed jump and asm goto across the same boundary are rejected.
The regression tests retain these cases as executable witnesses.

GCC 14.4 differs for literal `goto *&&label` leaving a cleanup lifetime: the
unmodified fixture returns 10 at both `-O0` and `-O2`, because the inner cleanup
is skipped. Clang 21.1.8 and Keel run that cleanup and pass the fixture.
Keel deliberately follows the checked Clang contract for this form. The audit
records GCC's different result separately from passing reference executions.

The [GCC cleanup attribute contract](https://gcc.gnu.org/onlinedocs/gcc-14.4.0/gcc/Common-Variable-Attributes.html) specifies callbacks on scope exit. [Clang cleanup lowering](https://clang.llvm.org/docs/LLVMExceptionHandlingCodeGen.html) describes cleanup-aware direct edges.

<a id="c-empty-aggregate-abi-empty-aggregate-abi-design"></a>
## Empty aggregate ABI design

Status: implemented for complete, fieldless, alignment-one structs and unions on
x86-64 SysV. Other empty shapes remain diagnosed.

The first target is `x86_64-unknown-linux-gnu` under native LP64. The motivating
Linux input is `mm/vma.i`, which passes `struct vm_userfaultfd_ctx {}` by value.
Other targets retain their diagnostic until independently qualified.

<a id="c-empty-aggregate-abi-contract"></a>
### Contract

GNU C permits an empty structure with size zero ([GCC documentation](https://gcc.gnu.org/onlinedocs/gcc/Empty-Structures.html)).
Its C type and object remain distinct from `void`. For the initial x86-64 ABI
contract, a complete zero-size aggregate contributes no argument or return
payload. It consumes neither a register nor a stack argument slot, and an empty
return introduces no hidden result pointer.

The prior checkpoint's GCC 14.4.0 and Clang 21.1.8 witnesses both pass the two
integers in `f(long, struct Empty, long)` in RDI and RSI. Both preserve an
effectful call producing the empty argument. Those witnesses establish the
simple case, not every aggregate shape, alignment or variadic case below.

C argument expressions still execute. An empty-valued call still executes even
when its result is unused. Returning an empty expression evaluates that
expression before running exit actions and emitting an IR return with no values.
Omission must not impose a new C argument evaluation order.

IR `ByVal` and `Sret` layouts remain nonempty. No IR syntax, verifier exception,
calling convention or machine encoder change is proposed.

<a id="c-empty-aggregate-abi-one-frontend-mapping"></a>
### One frontend mapping

Add a private C ABI mapping beside the current signature construction. It owns
the IR signature, one mapping entry per source parameter, and a result mapping.
Use enums to distinguish omitted parameters from parameters with an IR index,
and void results from omitted aggregate, direct and indirect results.

The mapping removes three sources of duplicated arithmetic:

- Hidden result parameters shifting ordinary parameter indices.
- Omitted source parameters shifting later IR parameter indices.
- Distinguishing fixed source arguments from variadic arguments.

Construct it through the same checked classifier for declarations, definitions,
direct calls and indirect calls. Existing scalar and nonempty aggregate
classification stays unchanged. Keep source function types intact; two different
C types do not become compatible merely because their IR signatures match.

An omission candidate must be a complete aggregate of known size zero, not an
incomplete type, VLA, arbitrary zero-length expression or unsupported value type.
Run existing unsupported-value checks before omission. Establish the admitted
shapes with compiler witnesses: empty structs/unions, nested empty members and
zero-length-array members. Explicit alignment and flexible-array-only records
need separate witnesses; reject unqualified shapes rather than infer an ABI
from size alone. Complex values continue through existing nonempty handling.

<a id="c-empty-aggregate-abi-lowering-changes"></a>
### Lowering changes

| Consumer | Required behavior |
| --- | --- |
| Function declarations and imports | Use the mapped IR signature, including cached direct imports. |
| Definition entry | Bind ordinary parameters by mapped index. Allocate an ordinary local object for an omitted parameter without reading an incoming value. |
| `restrict` metadata | Record the mapped IR parameter index, including any hidden result parameter. |
| Debug variables | Keep source parameter identity and local storage; omission does not remove the source variable. Current debug metadata uses a parameter flag, not an ABI index. |
| Call arguments | Evaluate every source expression once. Append only mapped payloads. Preserve existing aggregate snapshot barriers around later effects. |
| Variadic boundary checks | Use the source argument index against the source fixed-parameter count, never the number of emitted IR values. |
| Call result | For an omitted aggregate result, return a local object address to expression lowering without passing it to the callee. |
| Explicit return | Evaluate the expression and exit actions, then emit an empty IR return for an omitted result. |
| Implicit return | Keep the existing no-payload aggregate return behavior. |
| Unreachable result | Preserve aggregate address representation without inventing a scalar ABI result. |

The frontend already represents aggregate expression values as addresses and
reserves at least one backing byte for zero-size automatic objects. Reuse that
storage policy for parameter objects and call-result temporaries. Their C size
remains zero. The backing byte must not become an ABI payload or a required
load/store. Do not promise unique addresses between unrelated empty objects.

Zero-byte copies transfer no bytes, but computing their source and destination
must retain effects. Empty expressions must remain usable in assignments,
conditionals, comma expressions and forwarding returns.

<a id="c-empty-aggregate-abi-boundaries"></a>
### Boundaries

Support prototyped direct and indirect calls, definitions and declarations in
the first increment. Retain the existing rejection of aggregate arguments
without a prototype. Old-style definitions must either receive the same mapping
with qualified tests or diagnose explicitly; do not silently use the old zip.

Support omitted empty values among the fixed parameters of a variadic function.
Qualify that `va_start` locates later scalar varargs correctly. Continue rejecting
aggregate values in the variadic tail, including empty values, and explicitly
reject aggregate `va_arg` until separately designed. A fixed empty argument is
not permission to accept empty values in every varargs position.

Microsoft ABI calls remain unsupported. Non-x86 targets and nonnative data
layouts retain their existing boundary. Records with nonzero size but no
ordinary payload, such as padding-only records, are unsupported.

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
`SHF_GNU_RETAIN`. [Custom sections and distinct `retain`](extensions.md)
are now implemented for a bounded non-TLS definition subset. Lifecycle
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

The installed Clang oracle (`clang -S` on a static C destructor, macOS SDK 27)
emits a `__mod_init_func` thunk calling `__cxa_atexit`, with `__dso_handle`, rather
than emitting `__mod_term_func`. The current native executable linker does not
synthesize `___dso_handle`: mixed input containing a Clang destructor thunk is
explicitly rejected as an undefined symbol, although system linking supports
that mixed form. Keel uses the existing libSystem `atexit`
interface for its **executable-only** subset: a private constructor thunk registers
each destructor's bound address. This avoids introducing DSO handle synthesis or
claiming the obsolete module-termination section ABI. It requires linking
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
are outside the promise. No external compiler/linker subprocess is used by the
production lowering or native linker.

<a id="c-lifecycle-elf-x86-64"></a>
### ELF x86-64

Constructors and destructors use pointer-aligned `.init_array`/`.fini_array`,
`SHT_INIT_ARRAY`/`SHT_FINI_ARRAY` and `SHF_ALLOC|SHF_WRITE`. The native linker
retains them and emits dynamic array tags for dynamic output. Dispatch requires
the loader/libc startup and shutdown protocol described in [ELF linker](../../linking/elf.md#executable-initialization-and-termination).
A syscall-only `_start` does not arrange libc lifecycle dispatch; static startup
must explicitly invoke the arrays. This change does not supply CRT objects,
standard libc driver startup, DSO output or legacy `.ctors`/`.dtors` conversion.
Actual ELF objects and native linked static images are inspected on the arm64
host; these x86-64 images are not executed there.

<a id="c-local-register-variables-local-register-variables"></a>
## Local register variables

On x86-64 Linux, an automatic declaration such as
`register unsigned long value asm("rdx")` binds a direct extended-asm operand
to that hardware register. The variable otherwise uses ordinary C value and
storage lowering. The declaration does not reserve a register or sample its
contents outside an asm operand. This implements the supported
[GNU local register use](https://gcc.gnu.org/onlinedocs/gcc-14.1.0/gcc/Local-Register-Variables.html).

Bindings accept the existing GPR names and aliases, with an optional `%` prefix.
Aliases identify the canonical register; the C type and template modifiers
select the access width. RSP, RBP and high-byte aliases reject. The backend can
also reject a register needed as the active frame base. Types are integers up
to 64 bits or pointers. Floating, vector and aggregate bindings are outside this
increment. Global register declarations retain their separate read-only RSP
contract.

Direct input, output and read/write operands retain their binding through
`r`, `q`, supported register/immediate or register/memory choices and `g`.
A fixed-letter constraint must name the same register. Numeric and named
matching inputs propagate their binding to a generic register output, or check
that an existing output binding agrees. Matching GPR operands retain their own input and result widths. Early-clobber markers remain intact.

Taking a bound local's address rejects, including explicit memory asm operands.
Pure immediate constraints and incompatible register classes reject. A cast,
arithmetic expression or other expression that is not the local itself does
not carry the binding. The compiler continues to check clobber conflicts,
register overlap, frame reservations and existing stack-call preconditions.

The frontend stores the binding on the local and emits existing fixed-register
IR constraints at the asm statement. Existing operand staging, allocation,
restoration and output stores handle the transfer. No IR schema, backend
operand class, ISA table or public intrinsic ID changes are needed.

`x86_local_register.rs` checks inputs, outputs and read/write operands for all
14 nonreserved GPRs. It checks early-clobbers, named/numeric ties, integer aliases,
pointers, shadowing, volatile accesses and a kernel-shaped RAX/RDX call with an
explicit stack dependency. Hardcoded physical-register reads and writes in the
fixture make incorrect operand placement observable. IR roundtrips and
text/typed/chunked objects agree across 24 selector, allocator, pool and
optimization variants.

All 24 objects pass native GCC and Clang callers. Independent compiler
references cover ordinary register binding; they omit Keel's balanced-stack
call extension. The Clang reference additionally binds the tied output
explicitly. The tested Clang otherwise updates R9 for an input-bound generic
tie but returns RAX without transferring the result. GCC and Keel pass the
input-bound case without that adaptation. Six broken output, read/write and
hidden-call controls return their expected nonzero exits.

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

<a id="c-restrict-initial-c-restrict-support"></a>
## Initial C restrict support

The frontend retains `restrict`, `__restrict`, and `__restrict__` in type
sugar, including pointer typedefs. A function definition emits
`restrict_params(0, 1)` for its own top-level restricted pointer-to-object
parameter objects. Indices include any ABI hidden result parameter. Prototype
qualifiers do not supply facts to a later unqualified definition, declarations,
call signatures, or indirect calls. Local/block restrict, inner pointer layers,
function pointers, and array-bracket restrict currently supply no optimization
facts. This is conservative; it does not diagnose every invalid C qualifier.

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

The memory-forward pass only retains an available exact-address, same-type load
across a store when both addresses are distinct eligible restricted entry
parameters. Any non-address use of such a root (including escape, comparison,
block argument, call argument, or pointer derivation), volatile access or atomic
access through it disables that root for the entire function. Calls, volatile
operations, atomics and unknown writes clear available memory facts. Unknown
addresses never disambiguate a store. Facts remain local to a basic block.

For `int a = *p; *q = 9; return a + *p;`, with both parameters eligible,
`load p; store q; load p; add` becomes `load p; store q; add` using the first
load twice. The first potentially faulting load remains. Without the contract,
the second load remains. Modified overlapping restricted accesses are invalid
C and are not used as differential correctness tests. The native fixture tests
separate writable objects, legal read-only restricted aliases, ordinary writable
aliases, and volatile reads against Clang. This demonstrates an instruction
change, with no runtime performance claim.

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

The evaluated-operand proof also guards static initializers, preserving final
weak-symbol binding rules. `__builtin_constant_p` remains an unevaluated,
conservative integer query. It may answer zero when another compiler proves a
constant. Clang's observed evaluation of a nested side-effecting offset index
inside `constant_p` is not reproduced; that wrapper is excluded from the shared
compiler-reference harness and checked separately against Keel's unevaluated
contract.

`cargo test -p keel-c --test runtime_offsetof` checks native layouts, private
Wasm arithmetic and execution, static rejection, index side effects and IR
round trips. Its fixtures compare computed offsets with actual member addresses
and independent unsigned arithmetic. X86 exports cover selectors, allocators,
normal/empty pools and optimization on/off, with text/typed/chunked object parity.

<a id="c-sections-retain-c-custom-sections-and-linker-retention"></a>
## C custom sections and linker retention

Keel supports GNU `section("name")` and no-argument `retain`, including
`__section__` and `__retain__`, on function definitions and
static-storage object definitions (including `_Thread_local` / `__thread`) on both native targets. File-scope objects,
tentative definitions, and static locals are supported. Attributes merge over
the entire translation unit, including declarations following definitions.
Conflicting section redeclarations diagnose. An attributed declaration must
have a definition in the same translation unit. Automatic locals, parameters,
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
are not qualified by this change. The same explicit-storage rules apply to
AArch64 ELF. An arbitrary custom name does not arrange a system linker's RELRO
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

This first milestone requires all definitions explicitly sharing a named
section in one translation unit to have the same effective storage class after
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
its executable section headers. Native `--gc-sections` now collects whole input
sections and roots retained contributions; without that option, selected supported
sections survive. See [native GC policy](../../linking/elf.md#opt-in-section-garbage-collection)
for roots, unwind handling and limits. ELF COMDAT selection is
a separate earlier step: supported assembler-authored groups coalesce before
layout, while C attributes and IR `weak_odr` plus section/retain combinations
continue to diagnose. See [native COMDAT scope](../../linking/elf.md#comdat-selection-before-native-garbage-collection).

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

<a id="c-sections-retain-registration-tables-and-cross-tu-sharing"></a>
### Registration tables and cross-TU sharing

The combined C registration-table regression in `crates/keel-c/tests/sections.rs`
compiles three independent translation units and links them in interleaved order.
It covers a late-attributed static record, a retained static-local record in an
uncalled function, an asm-renamed weak callback, a constructor in custom code,
32-byte record alignment, function/data pointer relocations, and separate
unplaced retained data outside the table. Both object routes run with debug and
optimization independently enabled/disabled. Darwin executes the iterator and
constructor through both native and Apple dead-strip links. ELF native and GNU
GC images are inspected for exact bounds and relocated record contents; the
GNU link uses `-z start-stop-gc` to ensure boundary references do not implicitly
retain the table. No x86-64 execution is claimed.

Darwin boundary names (`section$start$...` / `section$end$...`) are raw assembler
labels without a C ABI underscore. The current C declaration asm-label grammar
requires that underscore, so this test obtains their addresses with supported
inline assembly. Direct C declaration aliases for these raw Darwin labels
remain outside the supported grammar. The 32-byte record size matches the
strongest input alignment: section bounds include inter-input padding, and
bounds alone do not turn a table with alignment holes into a dense C array.

Sharing diagnostics for C storage classes apply within a translation unit.
Native ELF identifier-named registration groups additionally reject mismatched
input types or flags across translation units, including zero-fill versus
initialized data, readonly versus writable storage, code versus data, and
retention mismatches, with C-generated regressions for both object routes.
Dotted ELF input sections keep separate identities. Mach-O regular sections
do not encode every original C storage distinction, such as mutable versus
relocatable-constant data in `__DATA`, or custom zero versus initialized data.
Object linking cannot reconstruct those distinctions to enforce the frontend's
complete storage-class policy across translation units.

The ELF validator reserves both `.gnu.*` and `.gnu_*` names, because
`.gnu_debuglink`, `.gnu_debugaltlink` and `.gnu_debugdata` have special debug
metadata consumers; see the [GDB separate-debug-file format](https://www.sourceware.org/gdb/current/onlinedocs/gdb.html/Separate-Debug-Files.html)
and [GNU binutils debug-section handling](https://www.sourceware.org/binutils/docs/binutils.html).
Validating discarded sectioned pointer/string initializers does not emit their
otherwise unused referents.

`xcrun dwarfdump --verify` reports no errors for the generated ELF registration
objects and ELF/Mach-O custom-section objects; it still reports
empty-child-DIE warnings from the existing debug emitter.

<a id="c-sections-retain-tls-custom-placement-and-retention-milestone"></a>
### TLS custom placement and retention milestone

TLS uses the existing `DataObject.thread_local`, `section`, `retain` and
initializer fields together; no artifact dialect bump is needed. C declarations
merge attributes across the translation unit, including after definitions.
Every declaration of a given object must agree on TLS storage. Block-scope TLS
requires `static` or `extern`; functions cannot have TLS storage. Attributed TLS
imports without a definition in the same TU, weak TLS, and TLS COMDAT remain
diagnosed. An ordinary TLS import remains supported by object emission.

`section` always places **template storage**, never the public Darwin TLV
descriptor. The initializer determines initialized versus zero-fill placement,
including an explicitly initialized all-zero object. Named TLS sections cannot
share with ordinary storage, or mix initialized and zero-fill templates in a TU.
The earlier same-section ELF retention consistency rule also applies. The
existing safe-name syntax, reserved metadata namespaces and alignment limits
remain in force. Conventional `.tdata` / `.tdata.*` require nonzero initialized
TLS, and `.tbss` / `.tbss.*` require all-zero TLS; ordinary `.data`, `.bss`,
`.text` and `.rodata` names reject TLS. Arbitrary safe ELF names are supported.

ELF emits `SHF_ALLOC | SHF_WRITE | SHF_TLS` (`0x403`) with `SHT_PROGBITS` for
initialized templates and `SHT_NOBITS` for zero-fill. Retention adds
`SHF_GNU_RETAIN`, keeping the existing GNU OSABI convention. Symbols remain
`STT_TLS` with their original identity, size and alignment. Unplaced retained
TLS uses separate `.tdata.keel.retain.<symbol>` or
`.tbss.keel.retain.<symbol>` input sections. Defined accesses remain local-exec
`R_X86_64_TPOFF32`; imported accesses remain initial-exec
`R_X86_64_GOTTPOFF`. This contract targets executable TLS, not general-dynamic
TLS or arbitrary shared-library placement.

The native ELF linker still rejects TLS. Both defined and undefined `STT_TLS`
symbols, and allocated `SHF_TLS` storage without symbols, receive a diagnostic
requiring external ELF linking: native `PT_TLS` layout and TLS relocation
processing are unsupported. Accepting the section flag alone would omit the
loader's TLS image and misresolve thread-relative accesses. No bounded native
path was added because existing executable layout, GOT imports and relocation
processing do not implement those contracts together. Actual GNU system-link
regressions resolve both relocation forms, produce a `PT_TLS` program header,
exercise `--gc-sections`, and diagnose TLS/non-TLS cross-TU references. These
x86-64 images are inspected on the arm64 host; execution is not claimed.

Darwin TLS templates require `__DATA`. Arbitrary safe names receive
`S_THREAD_LOCAL_REGULAR` (`0x11`) or `S_THREAD_LOCAL_ZEROFILL` (`0x12`);
custom zero-fill consumes virtual size without file bytes. Conventional
`__thread_data` and `__thread_bss` require their corresponding template class.
Other `__thread*` names, including descriptor/pointer/init-function sections,
remain reserved. Ordinary `__data`, `__const` and `__text` cannot host TLS.
The public symbol stays in compiler-owned `__DATA,__thread_vars`
(`S_THREAD_LOCAL_VARIABLES`, `0x13`), containing the bootstrap pointer, key and
relocation to the original `<symbol>$tlv$init` template. Accesses still use
TLVP relocations and the descriptor getter. Retain marks **both** descriptor
and template symbols `N_NO_DEAD_STRIP`; `used` remains the existing compiler-only
emission root. System `-dead_strip` tests prove an unreferenced retained pair
survives and a separate compiler-only TLS object disappears. Native Darwin
linking already lays TLS template sections contiguously, computes descriptor
offsets and preserves alignment relative to the template base. Native dead
stripping now preserves live descriptor/template pairs through its section graph.

Requested TLS template alignment is preserved in objects and linked template
layout. Darwin runtime over-alignment above the allocator's ordinary guarantee
is an existing limitation affecting Clang too: dyld allocates per-thread copies
with malloc/calloc and adds descriptor offsets. Object section alignment cannot
force allocation-base alignment. The first TLS32/64 execution fixture reproduced
this boundary; it was not treated as a passing alignment test. See the existing
[`tools/tls-alignment` investigation](../../../tools/tls-alignment/README.md).
Object tests retain 32/64-byte requests; passing execution tests use 8/16-byte
alignment and assert addresses, initialization, repeated mutation, static locals,
Clang-to-Keel cross-TU access and isolation across four concurrent pthreads plus
the main thread. Debug/optimization independently on/off and direct/text object
routes execute through Apple dead-strip and native links. An independent Clang
producer at `-O0`/`-O2` also executes through both linkers with the same harness.
Clang requires explicit `thread_local_regular` / `thread_local_zerofill` in its
custom C section spelling; bare arbitrary names produce regular template
storage and can fail system linking. Keel derives the TLS section types itself.
Keel's late-attribute merging and compiler-only `used` contract are intentional
and are not claims of identical Clang behavior.

Cross-TU Darwin TLVP references to ordinary definitions are rejected by the
native linker. The installed Apple linker accepts that incompatible combination;
the regression records acceptance without executing its invalid getter call.
Mach-O nlists do not encode a TLS type on undefined symbols, so an
ordinary non-TLS address reference to a descriptor cannot in general be
recognized as an incompatible C declaration; that direction still requires
consistent source declarations. Neither format carries all frontend
same-section class distinctions between TUs. Artifact encode/decode repeats
target placement validation; LLVM and Cranelift reject the combined TLS/custom
section/retain contract explicitly. Ordinary TLS, weak, bounds, lifecycle,
cleanup and K&R regression suites remain part of broad validation.

Primary ABI details checked for TLS:

- [ELF TLS ABI, sections 2–4 and x86-64 access models](https://www.akkadia.org/drepper/tls.pdf):
  TLS section types/flags, `STT_TLS`, `PT_TLS`, thread-relative values and local/
  initial-exec relocation requirements.
- [Apple Mach-O section type definitions](https://github.com/apple-oss-distributions/xnu/blob/main/EXTERNAL_HEADERS/mach-o/loader.h):
  distinct initialized template, zero-fill template and descriptor section types.
- [Apple dyld TLV implementation](https://github.com/apple-oss-distributions/dyld/blob/main/libdyld/ThreadLocalVariables.cpp):
  `findInitialContent` consumes contiguous sections by **type**,
  `initializeThunksFromDisk` uses template-relative offsets, and per-thread
  instantiation allocates and copies the template. This explains why arbitrary
  regular sections cannot implement named TLS storage.

Focused tests:

```sh
cargo test -p keel-c --test sections
cargo test -p keel-ir --test sections -p keel-toolchain --test foundations \
  -p keel-llvm --test export
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked \
  --target-dir target/keel-cranelift
```

Retained fixture objects and system ELF TLS images are written below
`target/tmp/tls-arm64` and `target/tmp/tls-elf` by the focused tests.
`dwarfdump --verify` finds no errors in the TLS Mach-O and ELF debug objects.

<a id="c-sections-retain-tls-interaction-with-native-comdat"></a>
#### TLS interaction with native COMDAT

- Darwin's generated `<symbol>$tlv$init` template label could otherwise collide
  with a user asm label. The producer appends `$` until the template name avoids
  all declared data and function symbols. The descriptor keeps its public
  identity, and its template relocation and retention directive use the chosen
  private label. Regressions occupy two successive candidate names, preserve
  matching data/function imports, compare both object routes, and execute
  through Apple dead stripping and native linking with debug and optimization
  independently on/off.
- Native ELF applies its `STT_TLS` and allocated `SHF_TLS` rejection to the
  whole selected object, including sections in a losing COMDAT group. Tests
  require the external-linking / `PT_TLS` diagnostic for initialized and
  zero-fill TLS, with and without symbols or retention, in both winning and
  losing groups. An undefined TLS import referenced only from a discarded group
  also remains rejected. An unextracted TLS archive member remains outside this
  contract; archive extraction policy and ordinary COMDAT selection are
  unchanged.

Additional coverage checks const/volatile TLS with late placement, retention
and alignment declarations. The ARM64 multithread fixture now combines TLS
with a constructor, destructor and cleanup callback on every iteration; teardown
checks the main thread's final TLS state. All 20 producer/linker combinations
execute with four worker threads and the main thread. Clang uses
`-fno-use-cxa-atexit` for this combined fixture to match Keel's executable
`atexit` teardown policy. Its default destructor thunk still needs the unsupported
`___dso_handle`, as documented in [C lifecycle](extensions.md).

DWARF output uses the descriptor address plus `DW_OP_form_tls_address` on
Darwin and `R_X86_64_DTPOFF64` plus `DW_OP_GNU_push_tls_address` on ELF.

Remaining TLS limits: native ELF TLS images/relocations, TLS weak/COMDAT/export,
over-aligned Darwin runtime allocation, general-dynamic TLS, and x86-64
execution. The COMDAT checks here cover TLS interaction only; they are not a
full COMDAT audit.

<a id="c-stack-observers-c-frame-and-stack-observers"></a>
## C frame and stack observers

Keel supports bounded GNU frame queries and read-only x86 stack-pointer
bindings. These use existing IR frame and stack observers; they do not change
the calling convention or add a kernel code-generation mode.

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
binding. Keel represents it as an input-only reserved-stack dependency, with no
SSA stack result or writeback. The backend uses live physical RSP, even if a
later input expression allocates stack space after the dependency snapshot.
It also applies an ABI call barrier for caller-clobbered GPR and SIMD values.
See the [balanced asm-call contract](../../targets/x86_64/c-inline-asm.md#balanced-stack-calls).

<a id="c-stack-observers-inlining"></a>
### Inlining

The C frontend's mandatory inline phase rebinds frame and stack observations to
the emitted caller after expansion. This covers `always_inline` helpers in x86
translation units containing asm, including helpers used to form asm immediates.
The ordinary IR inliner still refuses frame and stack observers. Stack
allocation and restoration remain outside the mandatory inliner's admitted
intrinsics. No public intrinsic IDs or backend encoders change.

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

<a id="c-system-header-compat-c-system-header-compatibility-extended-floating-types"></a>
## C system-header compatibility: extended floating types

Recent glibc headers declare GNU extended floating types (`_Float32`,
`_Float128`, complex binary128 via `mode(TC)`, and others) even in programs
that never use them. This document describes what Keel accepts today on
x86-64 Linux, why the remaining header failures occur, and the planned next
steps. See [the implemented boundary](#implemented-declarationlayout-checkpoint)
for the current support, and the object and application results
for the twelve representative inputs and native application runs.

<a id="c-system-header-compat-background-why-glibc-headers-failed"></a>
### Background: why glibc headers failed

The representative inputs are QuickJS `libregexp.c`, Lua `lvm.c` and SQLite
`sqlite3.c` on x86-64 Linux with glibc 2.44, preprocessed by GCC 16 and Clang 23
with each project's original flags:

```sh
gcc -E -std=gnu11 -D_GNU_SOURCE '-DCONFIG_VERSION="2024-01-13"' \
  -DCONFIG_BIGNUM -I. libregexp.c -o libregexp.i
gcc -E -std=gnu99 -DLUA_USE_LINUX lvm.c -o lvm.i
gcc -E -std=c11 -DSQLITE_THREADSAFE=1 -D_GNU_SOURCE \
  -DLONGDOUBLE_TYPE=double sqlite3.c -o sqlite3.i
```

Compilation uses `keel cc -O --opt-tier none --target x86_64-unknown-linux-gnu
--cpu-level v1 INPUT --emit-object -o OUTPUT`.

<a id="c-system-header-compat-__tc__-a-real-complex-binary128-contract"></a>
#### `__TC__`: a real complex binary128 contract

The reduced declaration is:

```c
typedef _Complex float C __attribute__((__mode__(__TC__)));
```

`__TC__` is GNU's complex binary128 machine mode, not an unexpanded macro. It
comes from `/usr/include/bits/floatn.h`; glibc selects this typedef for
compilers without native `_Complex _Float128` syntax, including Clang and Keel
(whose Clang-compatible identity macros select the same path). Before the
checkpoint, Keel reported `unsupported integer mode '__TC__'` here for raw
sources and for Clang- and Keel-preprocessed input.

Native Clang and GCC both give this type 32-byte size, 16-byte alignment, an
identity distinct from complex float and complex double, and binary128
arithmetic. Ignoring the mode would change layout and values even if the
typedef were unused in a particular TU.

<a id="c-system-header-compat-_float32-gcc-builtin-versus-clang-header-typedef"></a>
#### `_Float32`: GCC builtin versus Clang header typedef

GCC-preprocessed QuickJS and SQLite fail at `/usr/include/stdlib.h:145`:

```c
extern _Float32 strtof32(const char *restrict, char **restrict);
```

An unbound `_Float32` in a declaration or type-name context reports
`unsupported extended floating type '_Float32'` (rather than the earlier
misleading `expected ';', found 'strtof32'`). Ordinary identifiers with this
spelling and explicit header typedefs keep their lookup behavior; no builtin
type or implicit alias is introduced.

In GCC, `_Float32` has size/alignment 4/4 but is incompatible with both `float`
and `double`. Clang rejects the builtin spelling; the Clang glibc path instead
supplies `typedef float _Float32`, which Keel accepts with that typedef's float
identity, arithmetic and ABI. GCC rejects that typedef, so it cannot substitute
for the GCC builtin.

GCC-preprocessed Lua first encounters a different member of the family,
`extern int __fpclassifyf128 (_Float128 __value);`, and still fails with
`expected ')', found '__value'`.

In the GCC `.i` files, `_Float32`, `_Float64`, `_Float32x`, `_Float64x` and
`_Float128` appear 3/3/3/3/3 times in QuickJS, 0/0/0/0/8 in Lua and
455/467/460/472/487 times in SQLite, all inside system-header regions. GCC
input therefore needs five distinct builtin identities for QuickJS/SQLite and
one for Lua just to cover the declarations; the Clang/Keel header path already
supplies its own typedefs, which is why `TC` declaration support was the
smaller first step.

<a id="c-system-header-compat-native-identity-layout-and-abi-facts"></a>
#### Native identity, layout and ABI facts

[GCC's additional floating types documentation](https://gcc.gnu.org/onlinedocs/gcc/Floating-Types.html)
specifies `TC` for complex binary128 and the x86 `__float128`/`_Float128`
alias, and the `_FloatN`/`_FloatNx` identities remain distinct from ordinary C
floating types even for equal encodings. Native GCC on x86-64 Linux gives:

| Type | Size / align | Notes |
| --- | --- | --- |
| `_Float32` | 4 / 4 | distinct from `float` |
| `_Float64`, `_Float32x` | 8 / 8 | distinct from each other |
| `_Float64x` | 16 / 16 | x87 encoding, distinct from `long double` |
| `_Float128` | 16 / 16 | binary128, compatible with `__float128` |
| `TC` | 32 / 16 | compatible with `_Complex _Float128` |

The `extended-float-layout-oracle.c` fixture checks TC size/alignment, array
stride 32, real/imaginary order, offsets 16 and 48 in a char/TC/char wrapper,
total size 64 and sentinel bytes around a 32-byte copy with both native
compilers:

```sh
gcc -std=gnu11 -march=x86-64 crates/keel-c/tests/fixtures/extended-float-layout-oracle.c -o layout-gcc
clang -std=gnu11 -march=x86-64 crates/keel-c/tests/fixtures/extended-float-layout-oracle.c -o layout-clang
./layout-gcc && ./layout-clang
```

The [AMD64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex)
classifies real binary128 as SSE/SSEUP and complex binary128 as a pair of
those; the 32-byte pair becomes MEMORY after aggregate cleanup. It must not
inherit complex float/double register transport or `COMPLEX_X87` return rules.
Knowing the layout grants no call ABI.

`extended-float-aggregate-abi.c` (a `struct box { __float128 x; }` returned by
value) shows why: before the [unsupported-value checkpoint](extensions.md),
Keel emitted an object using integer registers, and native GCC and Clang
callers linked with it (`extended-float-aggregate-harness.c`) failed. Keel now
rejects it before object emission.

<a id="c-system-header-compat-next-steps"></a>
### Next steps

1. Specify and implement a distinct `_Float32` scalar identity through type
   compatibility, generic selection, arithmetic rank, casts, literals and
   diagnostics. Specify promotions deliberately (`_Float32` does not get
   `float`'s default `double` promotion; see
   [GCC's rationale](https://gcc.gnu.org/pipermail/gcc-patches/2016-August/455079.html))
   and x86 SysV argument/result/varargs behavior. Sharing F32 storage/IR
   requires preserving frontend identity. This alone will not accept the other
   `_FloatN` declarations in these headers.
2. Separately specify real binary128 values and ABI, including conversions,
   constants and the necessary compiler-runtime helpers. Add complex binary128
   operations and ABI only after that real-type contract exists. Native
   Clang/GCC cross-call and arithmetic oracles must cover `mode(TC)` itself.
3. Re-run the original sources and GCC/Clang/Keel preprocessed inputs after each
   stage to expose the next genuine boundary, without removing prototypes or
   changing headers.

Neither stage implies arbitrary `_FloatN`, complex/tgmath, vendor-header,
automatic-vectorization or extended floating-environment completeness.

<a id="c-unsupported-values-remaining-limits"></a>
### Remaining limits

This boundary rejects unsupported values, not the broader
[extended-float declaration proposal](extensions.md). It adds no
`TC`, `_FloatN`, F128 arithmetic, IR type, helper, ABI class or layout redesign.
Existing opaque layout limitations remain; a successful layout query does not
establish value support. Ordinary aggregate, complex and vector behavior is
covered by existing regression controls; backend F80 support is unchanged.
Native execution evidence here is finite x86-64 Linux evidence. It is not an
application-compatibility result or an exhaustive ABI/FP/memory proof.

<a id="c-weak-symbols-ordinary-c-weak-symbols"></a>
## Ordinary C weak symbols

Keel supports `__attribute__((weak))` (and `__weak__`) on external function
and object definitions, tentative object definitions, and undefined references.
The supported native targets are AArch64 Mach-O and x86-64 ELF. The attribute
must have no arguments. Static symbols, automatic variables, members, parameters,
typedefs and type names cannot carry it. Weak TLS is rejected. Hidden/internal
weak **definitions** work within a linkage unit; hidden/internal weak undefined
references are rejected. `weak_import`, `weakref`, aliases and ifuncs remain explicit errors.
[Bounded executable lifecycle](extensions.md) and
[custom sections and retention](extensions.md) are implemented separately. Unknown `weak*`
attribute forms also diagnose rather than silently producing strong symbols.

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
The experimental `keel-toolchain` artifact dialect is now `KEEL-IR-UNIT 4`;
versions 1–3 are rejected. Version 4 preserves weak and lifecycle/compiler-used
metadata and adds typed custom placement and linker-retention metadata.
There is still no whole-program IR linker or LTO weak-resolution implementation.

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

<a id="generic-overflow-builtins-lowering-and-correctness"></a>
### Lowering and correctness

The generic AST node retains both original operand types and the pointer's
qualifier metadata. The existing builtin evaluation loop evaluates the two
operands and the destination expression once each, in frontend order, before
performing the store. The store also occurs when the return value is unused.
Aliasing a result pointer with an operand's storage therefore uses the operand
values captured before the store.

The lowering converts each operand to an unsigned work type by extending its
original signed or unsigned value, then extracts sign and unsigned magnitude.
The work type is i64 when all three integer types are at most 32 bits, otherwise
i128. Integer IR arithmetic wraps, so taking a signed minimum's unsigned
magnitude by modular negation is defined.

For addition/subtraction, equal mathematical signs add magnitudes and retain
the unsigned carry; different signs subtract the smaller magnitude from the
larger and select that operand's sign. Subtraction reverses the second sign.
A carry means the magnitude exceeds every supported destination's range.

Multiplication of operands at most 64 bits fits in the unsigned i128 work
value (32-bit operands fit in i64). For an i128 operand, split each magnitude
into 64-bit limbs `a0,a1,b0,b1`. Compute the three full products `a0*b0`,
`a0*b1`, and `a1*b0` in i128. Add the two shifted cross-products to `a0*b0`,
retaining both carries. Any nonzero upper half of a cross-product, either
carry, or both `a1` and `b1` being nonzero proves a magnitude above `U128_MAX`.
The low sum remains the correct low 128 product bits. This requires neither
an i256 value nor an i128 multiply-overflow instruction.

Normalize negative zero, compare the magnitude with the destination limit
(`2^(n-1)-1` for positive signed, `2^(n-1)` for negative signed, `2^n-1` for
unsigned), and combine that comparison with the retained carry. Negative
nonzero values also overflow unsigned destinations. Finally apply the sign
by modular negation and truncate for the store. Every generated shift is
strictly below its work type's width. No C signed-overflow expression,
division, wider IR type, backend modification, or runtime helper is involved.

Synthesis is a fixed sequence per call. The full i128 multiplication path
uses three i128 low multiplies, two unsigned add-with-carry checks, and limb,
sign, and range operations. It emits more operations than the existing
fixed-type hardware-flag path. No runtime benchmark has been run.

<a id="generic-overflow-builtins-testing"></a>
### Testing

```sh
cargo test -p keel-c --test generic_overflow
cargo test -p keel-c --test run overflow_builtins_compile_on_both_targets
```

The suite runs natively on AArch64 macOS and x86-64 Linux and covers:

- Mixed input pairs across 12 integer types, rotated destination types, and
  every destination type for all four signed/unsigned i128 operand pairs.
  Each of 192 cases checks add/sub/mul on a 26-by-26 boundary grid and a
  deterministic random grid.
- Both the overflow flag and all 128 stored bits contribute to output hashes.
  Cases include zero, signed minima/maxima, unsigned maxima, limb boundaries,
  products with nonzero high halves, negative zero, and narrowing/widening.
- Clang reference executables at `-O0` and `-O2`; Keel with both Clang and
  native preprocessing and with IR optimization disabled/enabled. All outputs
  must agree. Existing fixed-type families run through the same differential
  helper.
- Side-effecting operand/pointer functions, indexed expressions, aliasing,
  ignored return flags, volatile stores, boolean/enum operands, and `_Bool`
  result typing.
- Source-located diagnostics for noninteger operands, invalid result types
  (including enum/const typedefs and atomic destinations), and wrong arity.
- A 432-function type matrix that verifies IR and code generation on AArch64
  and x86, before/after optimization, and asserts all volatile stores remain
  present.

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
The separate x86-64 Linux [TC declaration/layout subset](extensions.md#c-system-header-compat-implemented-declarationlayout-checkpoint)
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
  [three SSE2 conversion wrappers](../../targets/x86_64/intrinsics.md#c-x86-intrinsics-bounded-sse2-numeric-conversions)
  preserve architectural rounding/invalid/status behavior through dedicated IR.
- Bounded `__builtin_shufflevector` permutations with matching input/result
  shapes and constant in-range indices; see the
  [C x86 intrinsic subset](../../targets/x86_64/intrinsics.md) for the builtin and header contract.

Two vector operands must have the same C vector type; implicit conversions
between distinct vectors are diagnosed. The bounded scalar broadcasts below
also preserve that vector type. Integer vector division/remainder, floating bitwise operations, vector logical
operators/conditions, vector `!`, and reading vectors with `va_arg` are
unsupported. Initializer designators into vector lanes are unsupported.
Full optimization can recognize an exact four-lane 32-bit wrapping sum after
C lowering exposes matching extract/add SSA operations. This bounded
reduction pilot does not bypass C promotions,
reassociate floating arithmetic, recognize general shuffles/widening or
promise arbitrary automatic vectorization.

The backend may expand an operation into scalar **lanes**, preserving the full
vector type, layout and ABI; that is distinct from ignoring the type attribute.
Debug information currently presents a vector as an array of its elements.

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
Clang additionally accepts some out-of-range integer constants with a warning;
Keel diagnoses them instead of silently truncating. Numeric lane conversion
builtins and automatic vectorization are separate work. The existing bounded
FP contract does not promise signaling-NaN payloads, exception flags/traps or
strict floating-environment compatibility.

`gnu_vector_broadcast` checks conversion boundaries, compounds, distinct-vector
rejection, all ten signed/unsigned/FP lane types at both widths, real x86 ELF
object emission at v1 (16 bytes) / v3 (32 bytes), and disabled-AVX rejection at v1. The expanded `gnu_vector_abi`
Clang oracle executes broadcasts over all ten 16-byte lane types, arithmetic,
bitwise operations, all comparisons, compound assignments, unsigned values,
NaNs/infinities/signed zero, operand/lvalue evaluation counts and adjacent
memory sentinels under three selection/allocation/optimization configurations.
Runtime execution is on Apple arm64 with Apple Clang; Linux AArch64/x86 objects
are cross-emitted, not executed. No native GCC oracle run is claimed; GCC's
documented safe-conversion rule was used as the reference.

```sh
cargo test -p keel-c --test gnu_vector_broadcast --test gnu_vector_abi \
  --test gnu_types --test x86_intrinsics
```

The optional [legacy PCLMULQDQ subset](../../targets/x86_64/intrinsics.md) reuses the existing
`i64x2` representation and 128-bit ABI through an explicit intrinsic and small
C wrapper. It adds no vector type, raw-wide ABI or automatic polynomial recognition.

<a id="gnu-vector-mode-32-byte-extension-and-16-byte-audit"></a>
### 32-byte extension and 16-byte audit

The 32-byte extension reuses the 16-byte type/layout, initialization, lane,
arithmetic, comparison, memory, debug and IR lowering machinery. The audit found
two deliberate width limits to change: attribute shape selection and the
16-byte cast check. The latter now requires equal widths; the existing
`__int128` bitcast remains limited to 16 bytes. Aggregate ABI and vector
`va_arg` restrictions are retained. The bounded scalar broadcasts above apply
at both widths. No integer division, volatile lane access, numeric lane
conversion or initializer-designator support is inferred from the wider type.
Whole-vector accesses keep their width and memory flags; header unaligned copies use exact-size byte-safe memory builtins.

`x86_intrinsics` adds layout/feature diagnostics, all-header object emission,
and wide ABI/value/footprint checks with GCC/Clang oracles. The existing
`gnu_types` and `gnu_vector_abi` tests continue to cover the 16-byte subset.
See [C x86 intrinsics](../../targets/x86_64/intrinsics.md) for the finite supported names,
unsupported boundaries and validation commands. This extension does not claim
complete vendor header compatibility or general auto-vectorization.
