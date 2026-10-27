# C language implementation plan

Status: **bounded VM/VLA and parameter scopes, floating/generic atomics,
float/double complex arithmetic, automatic-local cleanup, explicitly typed K&R
definitions, ordinary weak binding, executable lifecycle registration and
compiler-only `used` emission are implemented**. These stages build on the GNU
integer-mode/vector and AArch64 inline-assembly subsets. Remaining features below are planned, not cancelled;
this is not a claim of full C11 or GNU C compatibility.

Toolchain boundaries remain explicit:

- Thin arm64 Mach-O executable ad-hoc signing runs in process. Developer ID,
  notarization and general-purpose re-signing are outside that implementation.
- Complex multiply/divide can require the target compiler builtins runtime.
  The tested Darwin archive links through Clang or Apple `ld`; Keel's native
  linker cannot yet consume it. See stage 6 for exact dependencies.
- Darwin executable destructors use libSystem `atexit`; ELF lifecycle dispatch
  requires external startup/CRT. See [lifecycle contracts](c-lifecycle.md).
- [Sanitizer and LTO foundations](sanitizer-lto-foundations.md) provide explicit
  memory-check instrumentation and a verified IR artifact, currently version 4.
  They do not supply a sanitizer runtime or production LTO/module merging.
- Native execution tests run on Darwin arm64. x86-64 tests cover IR, objects
  and selected linked images; native x86-64 runtime coverage is outstanding.

Driver/preprocessor flag compatibility remains deferred. Unsupported calling
conventions, including `ms_abi`, continue to diagnose.

## 1. Automatic block VLA storage — implemented

The type model distinguishes incomplete arrays (`len = None`, `vla = None`)
from VLAs (`len = None`, `vla = Some(hidden_local)`). The hidden `size_t` local
captures each dimension's bound once whenever the declaration executes. Nested
array descriptors retain both constant dimensions and captured runtime counts;
byte sizes/strides are products of these counts, computed when consumed. No
bound expression is stored in persistent type records. Bounds use strict
constant-expression evaluation: `(n++, 5)` must evaluate `n++` at runtime even
though its result is known. Runtime bounds must have integer type.

Implemented:

- Multidimensional automatic arrays with mixed constant/runtime dimensions of
  complete, supported object element types,
  including scalar, struct and qualified elements, at alignment up to 16 bytes.
- Declaration-time `stack_alloc`, array decay, element access, ordinary element
  pointer arithmetic/difference and calls through decayed pointers. Runtime
  element strides support multidimensional indexing (see stage 2).
- Runtime `sizeof a` and subarray sizes from captured bounds; subsequent changes
  to source bounds do not change them. Alignment queries remain compile-time values.
- `stack_save` immediately before each VLA allocation and
  `stack_restore` on normal exits, break, continue and return, including nested
  blocks, while/do/for loops, for-initializer scopes and statement expressions.
  A return value is evaluated (or an aggregate result copied) before restoration.
- Diagnostics for the staged-out cases listed below. The conservative
  `__STDC_NO_VLA__` macro remains defined because support is incomplete.

Not implemented yet (diagnosed): VM return declarations; runtime VM type names
nested inside parameter declarators; `[*]` in definitions; VM `typeof`; VM `_Generic`
selectors; atomic VM pointers; VM structure/union members; VM compound literals;
VM types in other type-name contexts (for example generic associations and type
compatibility builtins); static/extern or file-scope VM declarations, including
fixed-size pointers to VM types; VLA initializers; over-aligned VLAs. Functions
encountering VM declarations/bounds reject direct/computed goto, switch and
evaluated `alloca`, even with only a VM typedef/pointer and no dynamic allocation.
This conservative gate also applies to VM bounds parsed in unevaluated contexts.
Incomplete/function/void VLA elements are rejected recursively. Fixed-size
parameter arrays and existing fixed arrays remain supported alongside stage 2's
runtime parameters. Nonpositive
evaluated runtime bounds and allocation-size overflow have C's undefined
behavior; this stage does not add bounds sanitization.

Dynamic array variables are not yet exposed as dynamic DWARF array locations;
ordinary locals and bound evaluation still compile with debug information.

Validation in `crates/keel-c/tests/vla.rs`: semantic rejection cases; IR
verification before and after optimization, with/without debug information;
assembly and object generation for AArch64 Darwin and x86-64 Linux; native
AArch64 Darwin differential execution against Clang at `-O0` and `-O2`.
The runtime tests cover captured/side-effecting bounds, runtime sizeof, nested
scope exits, for-init lifetimes, early returns, structs, qualified arrays and
statement expressions. Repeated 4 KiB allocations over tens of thousands of
iterations exercise restoration under normal loop exits and continue.

## 2. VM types and control flow — bounded VM stage implemented

Implemented on top of stage 1:

- Runtime strides for nested arrays, pointer addition/subtraction/difference,
  prefix/postfix increment/decrement and pointer compound assignment. Explicit
  `&array`, qualified pointers to VLAs, pointers to those pointer objects, and
  fixed/runtime arrays of VM pointers preserve their distinct storage models.
  Pointer objects have static size; only runtime-sized arrays allocate storage.
- Separate bound evaluation from allocation. Block VM typedefs capture once at
  their declarations; typedef aliases, subsequent objects and pointer declarations
  reuse those captures. Re-entering a loop declaration captures fresh bounds.
  All new declarator bounds run before its initializer; later declarators see
  earlier declarations, and a name's own bound resolves before that name is bound.
- Runtime abstract type names in casts, `sizeof(type)` and alignment queries,
  including nested type names in bounds. Captures are sequenced into expression
  ASTs at their evaluation site; outer unevaluated expressions discard them.
- `sizeof` evaluates VLA expression operands, including `sizeof *p++` and runtime
  rows `sizeof a[i++]`; fixed-size rows and pointer operands remain unevaluated.
  Alignment queries do not execute bound expressions. Where C permits a bound
  evaluation to be omitted because it cannot affect `sizeof(type)`, this subset
  chooses to evaluate new type-name bounds. Tests accept either permitted result
  from Clang for that optional case rather than requiring its choice.
- Negative coverage for inherited VM typedefs leaking into members, static/extern
  objects and function return types, nested incomplete dimensions, unsupported VM type
  operators, and the conservative jump/alloca gates. Type records store only
  captured local IDs, never body-arena expression nodes.
- Function declarators introduce isolated prototype scopes. Earlier parameter
  names bind adjusted object types for later bounds, shadow typedef names, and
  do not escape the declarator. Nested callback prototypes have separate scopes.
  Definition parameter/tag/enum bindings continue into the outer function body.
- Definitions evaluate all newly written bounds once on entry, in parameter and
  dimension order, after receiving ABI arguments and before body statements.
  Even an outer bound removed by array-to-pointer adjustment retains its effects.
  Inner captures supply multidimensional `sizeof`, indexing and pointer strides
  after source parameter variables change. Hidden captures are locals, never ABI
  arguments. Fixed array typedefs adjust normally; block VM typedefs can be used
  in local prototypes and callback types without recapturing their bounds.
- Prototypes type-check bounds without evaluating them or allocating runtime
  storage; `[*]` is supported in all parameter array dimensions in prototypes,
  including nested callback types, and diagnosed in a definition's own list.
  Persistent ABI signatures erase capture identities recursively; expressions
  parsed in an enclosing function's prototype use the translation-unit arena,
  never that function's ephemeral body arena. Compatible redeclarations may use
  different bound expressions/names, `[*]`, fixed dimensions or pointer syntax;
  conflicting adjusted parameter types, arity and variadics are diagnosed.
- `static` with a bound and bracket `const`/`volatile`/`restrict` are supported
  only on the outermost parameter array. Qualifiers apply to the adjusted pointer
  object, separately from element qualifiers. Explicit `restrict` uses the
  existing definition-only restrict model. `static` alone supplies no nonnull,
  alignment, aliasing or bounds-check assumptions. Annotated/typedef `void` as
  the sole unnamed parameter correctly forms an empty prototype.

Parameter limits (diagnosed): VM return types; function definitions through a
function typedef without a direct parameter list; VM cast/sizeof/alignment type
names nested inside parameter declarators; atomic bracket qualification;
`static`/bracket qualifiers on inner dimensions or pointer-to-array parameters;
star bounds on a definition's own parameters; incomplete/void/function elements;
and invalid parameter storage classes. Prototype bounds do not introduce a VM
region into the enclosing function. Existing VM definition/block goto, switch
and evaluated-alloca gates remain in place. Dynamic array DWARF descriptions and
runtime validation of nonpositive/overflowing bounds remain outside this stage.

Remaining stages, not implemented:

1. Add VM `typeof` operand/bound evaluation and type-name contexts beyond casts,
   sizeof and alignment; implement VM compatibility for generic selection/type
   queries and lift the staged atomic VM pointer restriction with focused tests.
2. Record VM declaration regions at labels and jump sites. Reject jumps into a
   VM scope; restore exactly the exited regions on outward/backward gotos.
   Apply the same rules to switch dispatch and computed-goto targets. Replace
   the current whole-function diagnostics only with verified edge handling.
3. Reconcile function-lifetime alloca with lexical VLA restoration: either keep
   separate storage regions or preserve intervening alloca objects explicitly.
   Add dynamic debug locations, alignment greater than 16, and documented
   nonlocal-exit/returns-twice interactions. Review legal block static VM pointers
   separately from forbidden static VLAs before relaxing storage diagnostics.

Acceptance for the remaining work: Clang differential matrix for evaluation order,
side effects, parameter declarations/definitions, VM typeof/compatibility,
legal/illegal jump edges and stack exhaustion stress, both optimization modes;
verify and generate both targets, execute natively on each available target.

## 3. Reusable scope-exit actions and cleanup — bounded milestone implemented

GNU `__attribute__((cleanup(fn)))` and `__cleanup__` now bind a declared
function to an automatic block object. Declaration-specifier and named-declarator
attributes are supported, including pointer declarator layers and declaration
lists. The callback must have a prototype with exactly one nonvariadic parameter:
a pointer to a compatible object type, or `void *` (pointee qualifiers do not
change type compatibility). The callback's supported return value is discarded,
including struct/complex results through the ordinary ABI. The compiler marks
the local as address-taken even without an explicit source reference; an unused
local still invokes and retains its callback when its containing function runs.
Callbacks retain ordinary weak binding and visibility semantics, including
attributes added after their use. Incompatible callback return-type
redeclarations are rejected before they can change the call ABI.
No external cleanup runtime or helper is required.

Implemented behavior:

- Register an exit action after the declaration's initialization completes, or
  after an uninitialized object's declaration/allocation executes. Callbacks
  also run for uninitialized locals; reading their indeterminate values remains
  subject to the ordinary C rules. A return from an initializer does not register
  that declaration's callback. Re-entering a loop declaration registers a fresh
  object execution, with one callback on its exit.
- Run actions in reverse declaration order at normal scope exit, explicit or
  implicit function return, break and continue. Nested blocks, while/do/for
  loops, for-initializer scopes and GNU statement expressions are supported.
  Continue preserves the for-init objects; break leaves them once after the body.
- VLA allocation/restoration and cleanup share one ordered action list. Each
  VLA declaration saves the stack immediately before allocation; its cleanup
  runs before its restore. Later objects are cleaned first, then the VLA is
  restored, then earlier objects are cleaned. Outer VLAs remain live for inner
  callbacks. A bound-only VM declaration adds no stack action.
- Evaluate scalar return values and copy struct/complex return bytes before
  callbacks can change their storage. Statement-expression values are likewise
  captured before their scope actions. This milestone always freezes struct
  returns, including named locals. Clang's default C return-copy elision may
  expose callback mutations of a named struct return local; the differential
  oracle uses `-fno-elide-constructors` to disable that behavior at both `-O0`
  and `-O2`.

Diagnosed: cleanup on file-scope, static, extern or thread-local objects,
functions, typedefs, parameters, members and type names; undeclared callbacks or
function-pointer variables as callbacks; incompatible pointer parameters,
unprototyped/variadic/wrong-arity callbacks and incomplete return types;
redeclarations of automatic locals and multiple cleanup attributes on one
declaration (even identical ones). Explicit noreturn/returns-twice callbacks and
known nonlocal-exit callbacks are rejected, including contracts added later.

Functions containing cleanup locals conservatively reject direct/computed goto
anywhere in their bodies, including before the declaration and in unused
static/inline bodies. Switches use the existing scope-exit actions: case
fallthrough, switch break, enclosing-loop continue and return preserve callback
order. The `cleanup_switch` fixture checks those paths against native references. Known direct nonlocal-exit/returns-twice calls
(setjmp-family, longjmp-family and annotated functions) are diagnosed, including
annotations added after calls in discarded static/inline bodies, and known
runtime entry points reached through asm linker labels. This
milestone provides lexical cleanup only: no exception or longjmp unwinding,
nonlocal exits hidden behind indirect calls/callees, or cleanup after process
termination is promised. Callbacks must return normally. Existing VLA/alloca
restrictions remain. Some conservative gates also see parsed unevaluated code.
`__has_attribute(cleanup)` and `__has_attribute(__cleanup__)` remain **0** so
libraries with general GNU control-flow requirements keep their fallback paths;
explicit use in the supported subset works. Aliases remain diagnosed; bounded
section placement, retention and lifecycle support are described below.

Validation in `crates/keel-c/tests/cleanup.rs`: declaration/type/placement and
control-flow rejection cases on both targets; callback retention for an unused
local; verified IR before/after optimization, debug on/off; AArch64 Darwin and
x86-64 Linux assembly and actual object generation. Native AArch64 Darwin
execution compares both assembly-linked and directly emitted objects against
Clang, with optimization/debug on/off. Cases cover order, side effects, address
escape, scalar/struct/float-and-double-complex return freezing, callbacks returning
aggregates, statement expressions, interrupted initializers, declaration lists,
shadowing, nested loops, early exits, VLA cleanup and 20,000 repeated 4 KiB VLA
allocations. x86-64 objects are not executed on the arm64 host.

Remaining: declaration-region annotations for legal goto/computed-goto
edges, verified nonlocal-exit/exception policy, and eventually a broader feature
query promise after the conservative whole-function gates can be lifted.

## 4. Symbol binding, placement and retention — bounded milestones implemented

Existing support includes hidden/internal visibility, ordinary local/global
symbols, and [ordinary weak definitions and references](c-weak-symbols.md).
Aliases remain staged. [Custom section placement and distinct linker retention](c-sections-retain.md)
are implemented for static-storage objects, bounded TLS templates and function definitions.
[Default-priority executable lifecycle and compiler used emission](c-lifecycle.md)
are implemented with typed registration sections and target-specific shutdown.

1. Implemented: explicit IR ordinary weak binding metadata, weak definitions
   and undefined weak references for ELF and Mach-O, including null unresolved
   references, archive extraction and duplicate resolution. Validated using
   external Clang objects and object-symbol inspection. See the scope above for
   TLS/visibility restrictions and format-specific archive demand.
2. Implement `alias` using symbol identity and target-specific alias emission,
   with declaration merge/type checks and data/function/TLS boundaries. Gate
   unsupported alias target/linkage combinations explicitly.
3. Implemented bounded custom section placement and distinct `retain` metadata
   in typed IR, artifact v4 and both structured/text object paths. System GNU GC
   and Darwin dead stripping distinguish `retain` from compiler-only `used`.
   Weak identity and ordinary archive demand are preserved; native linkers accept
   the metadata and now offer opt-in whole-input-section GC. TLS placement is
   supported within its documented ABI subset; TLS COMDAT and special forms
   remain diagnosed. See
   [exact scope and backlog](c-sections-retain.md).
4. Implemented bounded constructor/destructor registration: defined normally
   returning `void(void)`, default priority only; other priorities diagnose.
   Darwin uses dyld init pointers and libSystem `atexit`; ELF uses typed init/fini
   arrays with existing loader/libc startup requirements. `used` forces compiler
   emission; `retain` and user placement have the separate bounded scope above. See lifecycle scope
   and tests above. General priorities and DSO/static startup remain.

Lifecycle-specific typed sections support registration alongside bounded custom
placement without claiming linker retention for `used`. Weak/alias work
must preserve interposition assumptions throughout optimization and linking.

## 5. Atomic floats and generic atomics — bounded native stage implemented

Both targets share native 1/2/4/8-byte integer atomic representations. A shared
size/alignment check gates the supported subset; no operation introduces an
external libatomic dependency.

Implemented:

- `_Atomic float` and `_Atomic double`, ordinary reads/assignments and C11
  load/store/exchange/strong or weak compare-exchange builtins (also exposed by
  the existing `<stdatomic.h>` macros). Float/double values cross integer atomic
  IR through bitcasts, preserving signed zero and NaN payloads. CAS tests integer
  representations rather than floating equality, including for NaNs.
- Floating `+=`, `-=`, `*=`, `/=`, prefix/postfix `++` and `--` use seq_cst CAS
  loops. The object address and RHS are evaluated once outside the retry loop;
  arithmetic uses the usual float/double calculation type and rounds back to
  the object type before CAS. The result is from the successful iteration.
  This inherits the frontend's ordinary floating environment behavior; dynamic
  floating-environment/exception suppression across failed retries is not added.
- GCC's buffer forms `__atomic_load(p, out, order)`,
  `__atomic_store(p, in, order)`, `__atomic_exchange(p, in, out, order)` and
  `__atomic_compare_exchange(p, expected, desired, weak, success, failure)`.
  Buffers must point to the non-atomic underlying object type. Scalars and structs/unions with total size
  1/2/4/8 and alignment at least that size use a single integer atomic access.
  This includes all representation bytes, including padding. Ordinary buffers
  use ordinary accesses, retaining their volatile qualification.
- Expected buffers are updated only on failure, including volatile buffers.
  All argument expressions, including weak and both orders, are evaluated once.
  Weak CAS is implemented with strong CAS. Consume uses acquire. The existing
  IR has one CAS order, so the success order also supplies a possibly stronger
  failure order; release CAS failures remain relaxed. Nonconstant success or
  failure orders conservatively use seq_cst. Invalid constant orders and
  failure orders stronger than success are diagnosed.
- Lock-free size queries cover these four widths; GCC pointer forms also check
  the declared/proven alignment, and `is_lock_free` retains argument evaluation.
  C11's one-size-argument query remains supported. Null GCC pointers request the
  usual native alignment. Unknown sizes conservatively return false.

Alignment is checked from the addressed object's layout when visible, otherwise
from the pointer's pointee type. Arbitrary casts do not prove runtime alignment;
callers must supply properly aligned storage, as for other typed atomic accesses.
An 8-byte struct with only 4-byte alignment is rejected unless its declaration or
visible object provides 8-byte alignment. Packed/under-aligned objects, unsupported
widths (including 16-byte integers), arrays and incomplete objects are diagnosed.
`_Atomic` struct/union types remain unsupported: this stage enables aggregate
buffer builtins, not aggregate atomic C lvalue conversions. Floating fetch
arithmetic builtins remain explicit diagnostics; use C compound arithmetic.

Validation in `crates/keel-c/tests/atomic_representations.rs` covers all four
widths and buffer operations, NaNs, signed zero, padding, promotion/rounding,
argument side effects, expected writeback and volatile failure-only writes;
IR verification and both-target assembly/structured code generation with and
without optimization/debug info; Clang differential execution on native arm64;
and mixed Clang/Keel pthread contention (four workers, 80,000 updates per counter,
three repetitions at each optimization setting). Direct CLI object validation
also generates eight Mach-O/ELF objects and executes the four arm64 variants,
plus two directly emitted stress objects linked with Clang workers. Undefined
symbol inspection confirms there are no atomic runtime helper dependencies.
x86-64 native execution is not covered.

Next stages, still requested:

1. Add aggregate `_Atomic` type/lvalue representation and explicit target lock-free
   capability metadata, including atomic layout/alignment distinct from ordinary
   aggregate layout where required. Extend floating fetch arithmetic and floating
   environment semantics with corresponding interoperability tests.
2. Choose and document an explicit libatomic-compatible or Keel runtime ABI for
   unsupported sizes/alignment. Implement indivisible locked fallback operations,
   expected updates, ordering, volatile behavior and lock-free queries together;
   provide/build/link the runtime explicitly. Never substitute plain memcpy or
   silently depend on an undeclared external runtime.
3. Validate runtime/native mixed-object interoperability, padded and large
   aggregates, alignment failures, contention and fallback symbol linkage on
   both native targets before widening the advertised subset.

## 6. Complex arithmetic — first float/double subset implemented

`float _Complex` and `double _Complex` have explicit component types, size twice
that of the component and the component's alignment. Real precedes imaginary
in memory. Internally they are address-backed pairs using the existing aggregate
copy and ABI machinery, while remaining arithmetic/scalar C types. Value reads
capture both components before later evaluation can overwrite their storage.

Implemented on AArch64 Darwin and x86-64 Linux:

- Construction with `__builtin_complex(real, imag)` (two matching `float` or
  `double` operands), GNU floating imaginary suffixes (`i`/`j`, either case,
  with `f` before or after the imaginary suffix), and GNU `__real__`/`__imag__`
  (also the spellings without trailing underscores). Components of lvalues are assignable/addressable;
  extraction also works on temporaries and preserves operand side effects.
- Real/integer to complex conversion with positive-zero imaginary part;
  complex to real/integer conversion discards the imaginary part; conversions
  between component precisions; `_Bool` tests **both** components. Existing
  scalar conversion limits, including unsupported floating/128-bit integer
  conversions, still apply. Complex/pointer conversions diagnose.
- Local/static/global initialization, zero initialization, arrays and aggregate
  members, assignment, unary `+`, unary `-`, logical `!`, addition/subtraction,
  multiplication/division and `+=`/`-=`/`*=`/`/=`, `==`/`!=`, conditions,
  logical operators and conditional expressions.
  Mixed real/complex addition/subtraction leaves an untouched imaginary component
  untouched, including its zero sign. Equality compares both components;
  NaNs are unequal and truthy. Negation changes both component signs.
- Prototyped direct and indirect arguments/results, including exhaustion of the
  floating register bank. Naturally laid-out structs/arrays containing complex
  values use exact recursive ABI layouts (including mixed integer/FP fields).
  Complex-containing unions, bit-fields and non-natural layouts passed by value
  diagnose. Ordinary access to packed complex members remains supported.

ABI basis: [AAPCS64 section 10.1.1](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst#1011-arithmetic-types)
maps these complex types to real/imaginary pairs; the HFA rules pass/return them
in two scalar FP registers on arm64. Apple platform behavior is checked against
Clang. [AMD64 psABI section 3.2.3](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex)
explicitly classifies complex float/double as component pairs: complex float
occupies one SSE eightbyte/register, complex double two SSE registers. Complex long double has a distinct x87 classification and is excluded.

Arithmetic contract and runtime dependencies:

- Complex-by-complex multiplication calls `__mulsc3`/`__muldc3`; division by
  a complex denominator calls `__divsc3`/`__divdc3` (also for a real numerator,
  with positive-zero imaginary part). These are the explicit four-real-argument,
  complex-result compiler-rt/libgcc ABI, lowered through the existing complex
  return classification on each target. Generated declarations are strong;
  source declarations/definitions or asm aliases colliding with a needed helper
  diagnose rather than changing its ABI/binding. No external compiler is invoked
  by C compilation or IR generation.
- Semantics follow the selected target runtime, tested against Clang without
  FP contraction. Primary algorithm sources:
  [compiler-rt multiplication](https://github.com/llvm/llvm-project/blob/main/compiler-rt/lib/builtins/muldc3.c)
  and [scaled division](https://github.com/llvm/llvm-project/blob/main/compiler-rt/lib/builtins/divdc3.c).
  Multiplication recovers infinities when both provisional components are NaN;
  division scales the divisor by its binary exponent, then recovers division by
  zero, infinite numerators and infinite denominators. Ordinary finite rounding,
  signed zeros, NaN/Inf classes, extremes and subnormals are differential-tested.
  **This is compiler-runtime compatibility, not an all-range mathematically
  accurate algorithm**: multiplication can lose cancellation through intermediate
  overflow/underflow; division can overflow its numerator sums or lose tiny
  terms despite divisor scaling. There is no guarantee of correctly rounded
  complex results, NaN payload/sign preservation, signaling-NaN handling,
  exception flags, trapping behavior, alternate rounding modes or full Annex G.
- Multiplication with one real operand, and complex division by a real operand,
  use separate component operations in the common component precision. This
  avoids artificial zero products and matches Clang's mixed-domain zero/NaN
  behavior. Compound operators use that same calculation followed by conversion
  back to the lhs type, with one evaluation of each operand/place.
- **Native AArch64 Darwin link validation:** the Clang driver resolves helpers
  automatically. On the tested Apple Clang 21/macOS SDK, libSystem's arm64 TBD
  reexports the multiplication helpers; division comes from
  `libclang_rt.osx.a` (located in tests with `clang -print-libgcc-file-name`).
  Tests also invoke Apple `ld` directly with the actual archive, SDK sysroot and
  `-lSystem`, and execute the result. Thus division is a new explicit compiler
  builtins runtime dependency, not a silently unresolved symbol or an assumption
  that libm/libSystem alone suffices. Runtime-internal libm dependencies resolve
  through libSystem. Keel's own Mach-O linker currently rejects this SDK archive's
  universal wrapper and combined macOS/Mac Catalyst object build records; use
  the Clang driver or Apple `ld` for these programs. This milestone does not add
  automatic runtime discovery to `keel link`.
- x86-64 Linux objects reference the analogous compiler-rt/libgcc builtins.
  A native compiler driver must supply its builtins runtime (and libm if its
  runtime requires it); freestanding linking must supply them explicitly.
  Availability/resolution and execution there are **not yet verified**.
- Static folding uses component-precision operations, rounding each intermediate
  independently (including float), and the same divisor scaling for ordinary
  finite expressions. Nonfinite operands, subnormal/nonfinite intermediates,
  multiplication/division underflow to zero, zero/subnormal divisors, and scales
  outside the normal component range are conservatively refused in static
  initializers. Executable expressions remain supported through the runtime.
  This intentional constant-expression gap avoids claiming that host math or
  Clang's static exceptional folding models every target runtime version.
  Final weak binding still governs constant address truth, including inside
  these expressions. The bounded constant evaluator adapts compiler-rt;
  attribution and Apache-2.0 WITH LLVM-exception license are retained in
  `crates/keel-c/src/complex_eval.rs` and `third-party/licenses/LLVM.txt`.
- Imaginary literals retain their real literal's float/double precision (decimal
  float parses directly to float; hex rounds directly to the destination IEEE
  precision), with a positive-zero real component before unary operators.
  Hexadecimal floating spellings require a `p`/`P` exponent, including imaginary
  literals; malformed and repeated suffixes diagnose against the Clang oracle.
  Raw native platform `<complex.h>` is tested using `I`, `_Complex_I`, `creal`
  and `cimag`; this is not a claim that all its functions/builtins work.

Not implemented (diagnosed): ordered comparisons, integer-only operators, GNU
complex conjugation with `~`, increment/decrement, complex variadic arguments
and `va_arg`, `_Imaginary`, integer imaginary literals, and long-double
imaginary literals/`_Complex long double` **on both targets**, including Darwin's
double-width long double alias. Extended complex declarations still parse for
headers, but values do not. `__STDC_NO_COMPLEX__` remains defined because the
implementation does not provide full C complex/Annex G/floating-environment
support. Debug information describes a complex value as a two-element component
array rather than a dedicated DWARF complex base type.

Validation: `cargo test -p keel-c --test complex` compares Keel with Clang for
finite values, subnormal/range extremes, both zero signs, NaNs/infinities,
all four independently varied components for multiplication/division, mixed
scalar domains, compound expression values/single evaluation, self-aliasing and
mixed-precision compound assignments, literal precision/lexical diagnostics,
conservative static-fold rejection/final weak binding, conversions, side effects,
component access, initialization and calls in both directions. It verifies IR and generates Mach-O/ELF objects for both targets in
baseline/full, trees/fast with debug info, and optimized/full configurations.
Native AArch64 Darwin execution is tested; x86-64 runtime interoperability is
**not yet tested**. Assembly register probes against Clang
also check the distinct complex-float/double argument and return classifications.
Cross-object generation and classification checks are not a substitute for native execution there.


Remaining limits are the runtime/linker dependencies above, unexecuted native
x86-64 interoperability, conservative static folding, and the explicitly excluded
Annex G/fenv, long-double and variadic behavior.

## 7. x86 extended long double — not implemented

Apple AArch64 long double already maps to double. x86 long double is currently
opaque storage with target layout, not x87 computation. Introduce an explicit
extended floating type and constant representation without narrowing through
host f64. Add IR arithmetic/conversions/comparisons, verifier and optimizer
rules, then x87 stack lowering or supported helper-based lowering. Implement
SysV argument/return/varargs classification and 16-byte storage padding. Test
excess precision, rounding, constants, NaNs and mixed Clang calls before enabling
arithmetic. Extended complex values depend on both stages 6 and 7.

## 8. K&R definitions — explicit fixed-size parameter milestone implemented

C11-oriented parsing accepts identifier-list definitions with separately typed
parameters, including declaration lists in a different order and comma-separated
parameter declarations. The identifier list fixes ABI argument order; each name
enters ordinary scope after its own declaration's declarator. Duplicate list
names, repeated declarations, extra declarations, missing explicit types,
initializers, invalid storage classes and body redeclarations diagnose. A
visible typedef-name starts a prototype and is never reinterpreted as a K&R
identifier; typedef-names later in an identifier list also diagnose.

Supported parameter objects are integers up to 64 bits (including `_Bool`, char
and short), float/double and pointers, with ordinary qualifiers, typedefs and supported
parameter attributes. Fixed-size arrays (including multidimensional arrays)
and function parameters adjust to pointers. Function declaration/return
attributes and direct-list scope ownership continue through the definition,
including functions returning function pointers and nested callback prototypes.

`FuncTy` records whether an unprototyped type describes a definition and retains
its promoted ABI types. `FuncDef` locals retain their declared types and sizes.
Entry lowering converts int to char/short/bool and double to float **before**
SSA binding or storing to addressed, volatile or debug parameter slots. Calls
without prototypes still perform default promotions, including direct calls
parsed before the definition and indirect function-pointer calls. Compatible
prototypes before/after definitions compare the adjusted, promoted parameter
types and argument counts. Parameter contracts retain pointee qualifiers through
array adjustment, typedefs and nested callbacks, while ignoring top-level
parameter qualifiers; mismatched pointee qualifiers diagnose before/after the
definition. Bare unprototyped declarations accept only promotion-compatible, nonvariadic prototypes. Taking an external unprototyped
function's address before its first call no longer freezes an incomplete IR
signature, including aggregate results with a hidden result-address parameter.
Actual zero-argument calls establish a contract separately from address-only
references. Direct unprototyped calls that disagree with a known definition,
a later prototype, or another call's ABI diagnose instead of producing invalid
IR; this is a compile-time subset check, not a runtime argument check.
The parser's pre-existing policy for unrelated return-type redeclarations is
unchanged; this milestone tightens parameter compatibility.

Tests in `tests/old_style.rs` use a Clang C11 syntax oracle, verify IR and assemble
actual objects for AArch64 Darwin and x86-64 Linux with optimization/debug
combinations. Native arm64 runtime differential tests cover declaration order,
SSA and addressed entry conversions (including observable float rounding),
array/function/pointer adjustment, side effects, scopes, function attributes,
prototype/unprototyped calls and Clang ABI linkage in both directions.
Regressions also cover unsigned char/short versus unsigned int promotion contracts,
prior/later prototypes, aggregate-return address-before-call imports, and array
element completeness and qualifier compatibility. A combined runtime case
exercises modern VM parameter capture, body VLAs, cleanup callbacks, weak definitions,
atomics, vectors and complex arithmetic/literals inside an old-style definition.
x86-64 runtime execution is not yet tested.

Arrays with incomplete element types, pointee-qualifier mismatches in function
parameter compatibility and unprototyped call ABI mismatches (including actual
zero-argument calls) diagnose rather than escaping as verifier errors.
Tags/enumerators introduced inside declaration-list declarators do not leak, and
nested callback prototype tags/enumerators retain their own scopes. A prototype
retained before an old-style definition still enforces call arity; Clang may
only warn for that ordering, so those cases are separate from strict syntax
rejection comparisons.

Remaining/diagnosed: implicit int in parameter types or undeclared parameter
defaults; VM/VLA parameter declaration lists; array `static`/bracket qualifier
contracts in these lists; aggregate, complex, vector, atomic, half and 128-bit integer parameters by
value; new tags/enumerators introduced by a declaration-list type specifier or
declarator outside a nested callback prototype (their narrower Clang visibility
is not approximated as body scope); variadic prototype/K&R definition
combinations; nested functions. Predeclared struct pointers and enum
types work. Empty-list definitions have a known zero-parameter definition
contract; a later incompatible prototype diagnoses even where Clang only warns.
Indirect calls through an unprototyped type have no new runtime checks; the
frontend does not prove that a function pointer's target matches its call ABI.
General `-std` driver selection remains deferred; C11 mode includes old-style
definitions.

## 9. C inline assembly — first AArch64 frontend milestone implemented

The [checked GNU C subset](c-inline-asm.md) now parses basic empty/nop assembly
and extended statements with scalar register outputs/inputs, read/write and
matching operands, named substitutions, integer immediates, memory inputs and
register/memory/cc clobbers. Output locations are evaluated once and reused.
All statements lower to pinned `InlineAsm` IR, with the existing AArch64
register-pool and clobber-aware allocation contract. Unsupported constraints,
lvalues and forms diagnose rather than disappearing. Native Clang differential
tests verify IR, emit Mach-O objects and execute opt/no-opt with/without debug.

Remaining: memory outputs, fixed/local-register constraints, broader constraint
and template syntax, vector operands, and x86 constraint/selection and object
assembly support. The x86 C frontend explicitly rejects asm statements.
Asm-goto remains diagnosed and depends on stage 2's edge/scope-exit model.

## 10. GNU nested functions — not implemented

Depend on lexical environment capture, nonlocal control-flow analysis and the
ABI/constraint work above. Start with direct calls using an explicit static
chain and captured frame references, then recursion and nonlocal variable
mutation. Escaping function addresses require a target-specific trampoline or
closure ABI and executable-memory policy; diagnose these until implemented.
Nonlocal goto must share the scope-exit/VM legality infrastructure. Test nested
recursion, frame lifetimes, VLA capture and mixed captures before enabling
function-pointer escapes.

Unimplemented portions of these stages remain requested future work. Dependencies
constrain implementation order; they do not remove features from the backlog.

## Tests

Each stage has a focused suite in `crates/keel-c/tests`:

```sh
cargo test -p keel-c --test vla                       # stages 1-2
cargo test -p keel-c --test cleanup                   # stage 3
cargo test -p keel-c --test weak_symbols --test sections --test lifecycle  # stage 4
cargo test -p keel-c --test atomic_representations    # stage 5
cargo test -p keel-c --test complex                   # stage 6
cargo test -p keel-c --test old_style                 # stage 8
cargo test -p keel-c --test inline_asm                # stage 9
```

The VM/VLA fixtures run the target x optimization x debug matrix: IR
verification before/after optimization, textual assembly and actual Mach-O and
ELF object assembly. Native execution from textual assembly and directly
assembled objects is compared with Clang `-O0`/`-O2`. Coverage includes 2D/3D
mixed dimensions, captured row strides, qualified VM pointers and pointer
arithmetic, VM pointer arrays, typedef capture/reuse/shadowing/loop re-entry,
abstract cast/type-name bounds, required and omitted `sizeof` operand effects,
conditional/short-circuit captures, declarator sequencing and scope, parameter
bound effects and capture order, static bounds, nested callback declarations,
qualifier preservation through array typedefs and decay, and const
modification diagnostics. IR assertions ensure typedef/pointer-only and
parameter-only declarations capture bounds without dynamic allocation or stack
snapshots.

The atomic suite also builds Mach-O/ELF objects with `keel cc --verify-each
--emit-object`, links arm64 variants and pthread stress objects with Clang, and
checks with `nm -u` that no atomic helper symbols are referenced.

x86-64 objects are generated and verified, but native x86-64 execution is not
part of these suites. None of this is a claim of full VLA, atomic or C11
conformance.
