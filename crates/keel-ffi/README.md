# Keel C API

`keel-ffi` exposes Keel's Rust IR, optimizer and native backend through a C ABI.
It builds static and shared libraries. Typed builders cover common frontend work;
complete textual IR input also reaches core syntax without a typed C wrapper.
Other languages can bind the declarations
in [include/keel.h](include/keel.h). The header also works in C++.

This is experimental ABI revision 1. Check `keel_abi_version()` against the
header version. The interface can change before it is stabilized.

## Build and use

```sh
cargo build -p keel-ffi --release
cc -std=c11 crates/keel-ffi/examples/build_function.c \
  -I crates/keel-ffi/include -L target/release -lkeel_ffi \
  -Wl,-rpath,"$PWD/target/release" -o target/build-function
# macOS arm64; use linux-arm64 or linux-x86_64 for Linux objects.
target/build-function darwin-arm64 target/function.o
cc crates/keel-ffi/tests/consumer.c target/function.o -o target/ffi-consumer
target/ffi-consumer
```

The shared library is `libkeel_ffi.dylib` on macOS or `libkeel_ffi.so` on Linux.
The static library is `libkeel_ffi.a`; static consumers must also link the Rust
standard library's platform dependencies. The commands above use the shared
library and link the emitted object using the host system compiler.

The [example](examples/build_function.c) builds
`int64_t max_plus_seven(int64_t a, int64_t b)` with a comparison, conditional
branches and a merge block parameter. It verifies and optimizes the module,
then emits an object. The consumer executes both branch outcomes and signed
boundary cases.

## Compile a DSL from C

[dslc.c](examples/dslc.c) is a complete recursive-descent compiler written in C.
It reads a source file, emits SSA through the C API, verifies and optimizes the
module, then writes a native object. It does not invoke Keel's C frontend.

The [sample program](examples/functions.dsl) includes:

```text
fn score(x, y) = if x > y then x * 2 + y else y * 2 + x;
fn polynomial(x) = (x + 3) * (x - 2);
fn clamp(x, low, high) = if x < low then low else if x > high then high else x;
```

Build and run from the repository root:

```sh
cargo build -p keel-ffi --release
cc -std=c11 -Wall -Wextra -Werror -pedantic crates/keel-ffi/examples/dslc.c \
  -I crates/keel-ffi/include -L target/release -lkeel_ffi \
  -Wl,-rpath,"$PWD/target/release" -o target/dslc
target/dslc darwin-arm64 crates/keel-ffi/examples/functions.dsl target/functions.o
cc crates/keel-ffi/examples/dsl_consumer.c target/functions.o -o target/dsl-consumer
target/dsl-consumer
```

Use `linux-arm64` or `linux-x86_64` instead of `darwin-arm64` for Linux objects.
Execute the linked program on the matching host. The consumer prints
`DSL functions passed` after checking the generated functions.

Each function returns one i64. Parameters and expressions are also i64.
Arithmetic supports `+`, `-`, `*`, unary minus and parentheses, with conventional
precedence and left associativity. Arithmetic wraps modulo 2^64. Conditions
compare signed values with `==`, `!=`, `<`, `<=`, `>` or `>=`.
An `if` expression evaluates only its chosen branch. Nested conditionals lower
to separate blocks and pass their result into a merge block parameter.

Identifiers use letters, digits and underscores, starting with a letter or
underscore. `fn`, `if`, `then` and `else` are reserved words. `#` starts a line
comment. Decimal literals range from 0 through INT64_MAX; negative values use
unary minus. Function calls, local bindings, loops and division are outside this
example's grammar. A program contains one or more semicolon-terminated functions.

The compiler reports source paths, lines and columns for syntax and name errors.
It rejects duplicate names, overflowed literals and incomplete expressions.
Limits are 64 KiB of source, 16 parameters per function and 128 nested parser
expression/unary frames. It opens the output only after compilation succeeds,
so source errors preserve an existing output file. The grammar is beside the
parser at the top of the C source.

## Systems frontend APIs

Create fixed-arity C signatures with `keel_signature_create`. Each parameter and
return carries its type and ABI extension. An i8/i16 boundary must explicitly
choose `KEEL_ABI_SIGN_EXTEND` or `KEEL_ABI_ZERO_EXTEND`; other types use
`KEEL_ABI_NONE`. Extension is an ABI property, separate from integer casts inside
a function. Use `keel_module_add_function_with_signature` for definitions or
`keel_module_declare_function` for external/forward declarations.

A forward function symbol can be called before its body is built. Complete it
with `keel_module_define_function`; existing references retain the same symbol
ID. This also supports recursive and mutually recursive functions. Obtain a
symbol for a definition with `keel_function_symbol`. C-convention functions default
to exported linkage; `keel_function_set_linkage` can make them module-local.
Private fast-convention definitions are local and cannot be exported.

Direct calls take a function symbol. Indirect calls take a pointer value and an
explicit signature. Both require exact argument types and either zero or one
result, matching the signature. A void call uses a null result pointer with count
zero. `keel_build_symbol_address` takes the address of functions and globals.
These operations generate calls and addresses; they never invoke a target
function or dereference a generated pointer in the compiler process.

Use `keel_stack_slot_create` and `keel_build_stack_address` for fixed-size local
storage. Stack slots are uninitialized and valid until the generated function
returns. `keel_build_load` and `keel_build_store` accept scalar types, including
pointers, and explicit memory flags. `{1, 0}` makes no extra alignment or fault
promises. Volatile accesses are neither atomic nor thread fences. `NOTRAP` and
`TRAP_UB` must follow the source language's contracts; the API cannot establish
runtime pointer validity. No implicit bounds checks are emitted.

Casts expose integer sign/zero extension, integer truncation and the native
pointer/i64 conversions. `keel_build_ptradd` adds bytes without scaling by an
element size. Construct a null pointer by converting an i64 zero with
`KEEL_INT_TO_PTR`. Integer comparisons also accept equal-type pointers.

Globals can contain copied bytes, zero-initialized storage, a native pointer
relocation, or ordered initializer items mixing these with scalar integers.
`keel_module_add_global_items` supports records, pointer tables and view/string
records. It does not add padding between items. `keel_global_set_properties`
controls TLS, weak binding, hidden visibility, retention and ODR binding. Constant and local
flags control mutability and linkage. External globals can be declared first and
defined later under the same name, preserving their symbols. Byte initializers
are copied verbatim; the frontend supplies target byte order and aggregate
padding. Duplicate declarations/definitions and function/data name collisions
are rejected.

The [systems client](tests/systems_client.c) exercises these APIs without adding
a source-language parser. Its [C consumer](tests/systems_consumer.c) calls the
generated functions, supplies external functions/globals, checks memory writes
and callbacks, and executes an array loop. It covers signed and unsigned narrow
ABI values, register/stack call arguments, local function addresses and relocated
pointer globals. The integration test emits all three object formats before and
after optimization and links/runs the matching host objects.

## Ownership and errors

A module owns its functions, blocks, values, signatures, symbols, layouts, source
files and stack slots. IDs are passed by value and checked for the correct module
and, where applicable, function. Adding IR preserves existing IDs. Optimizer
invocations invalidate all IDs once they start, including on an optimizer error
or a no-op tier. Verification failure before optimization starts preserves them.
`keel_module_find_function`, `keel_module_find_symbol`, `keel_function_block_at`
and `keel_block_parameter_at` obtain fresh IDs after rewriting or loading IR.
Recreate immutable signature/layout handles when needed after rewriting.

Every status-returning operation supplies a module diagnostic through
`keel_module_error`. Success clears it. Outputs are written only on success.
An unexpected Rust unwind poisons the module; read its diagnostic and destroy
it. Further operations return `KEEL_POISONED`. Process faults, allocation failure
and builds using `panic=abort` can terminate the host process.
The Rust panic hook still runs before a caught unwind. An ordinary error does
not promise rollback unless the operation documents it. Load and merge provide
the transactional guarantees described below.

Output buffers have independent ownership and survive module destruction.
Borrow their bytes with `keel_buffer_data` and `keel_buffer_size`, then release
them with `keel_buffer_destroy`. Never free API allocations through the host
language's allocator. Serialize access to each module. Separate modules can
be used concurrently.

The [header](include/keel.h) defines the full pointer, lifetime and error contract.
Foreign pointer validity is the caller's responsibility. Checked IDs do not
make arbitrary pointers safe.

## Compiler API coverage

The C API exposes frontend construction and native compilation. It does not
mirror every public Rust helper, arena or pipeline trait as a C object. The
header keeps ownership, fixed-width tags and output lifetimes explicit.

| Area | Typed C access | Core IR input access |
| --- | --- | --- |
| Values | i8/i16/i32/i64/i128, f32/f64/f80, pointers, supported fixed vector shapes | Every type accepted by the core parser |
| Arithmetic | Integer/float binary and unary operations, exact float constants, casts, fused multiply-add, select, comparisons, result/overflow pairs | Additional vector conversions/comparisons and target wrappers |
| Control flow | Blocks/parameters, branches, switch, returns, trap | Tail calls, invoke/landing-pad and remaining core control forms |
| Memory | Loads/stores with access flags, stack slots, pointers, atomic load/store/RMW/CAS/fence | All parser-supported memory and special region forms |
| Intrinsics | Registry lookup by name with signature checks; named NEON operations | Same registry and immediate syntax |
| Inline assembly | Templates, constraints, input values, explicit result types | Full core inline assembly syntax |
| ABI | Immutable scalar/natural aggregate layouts, BYVAL/SRET, private/C conventions, varargs, returns-twice and x86 producer ABI policies | Remaining parser-supported explicit ABI forms |
| Globals | Byte/zero/integer/pointer items, local/constant, TLS/weak/hidden/retained/ODR properties | Explicit sections and other parser-supported object metadata |
| Function metadata | Flags/alignment, source instruction locations, x86 feature permissions | Call/value contracts, ordered optimize attributes, full debug type/scope graphs and sections |
| Optimization | Built-in tiers and named pass lists | Function-local optimization metadata |
| Native output | Assembly or objects, stable selection/allocation tiers, size/unwind/section/code-model policies | Same backend acceptance rules |

`keel_module_load_ir` accepts a complete UTF-8 module using `keel_ir::parse`,
verifies it and replaces the current IR only on success. Syntax or verification
failure preserves existing IR and IDs. Successful replacement invalidates all
IDs. `keel_module_merge_ir` uses the core module merger, including its current
restriction against merging debug-bearing modules. It also preserves the module
on ordinary failure. Both routes retain target checks; parser acceptance alone
does not establish that a backend can lower an instruction.

Named intrinsics follow the core registry, rather than a manually duplicated C
enum. This includes memory, dynamic stack, lifetime and target-specific operations.
Operand/result types are checked immediately. Immediate-value, feature and target
restrictions remain enforced by final verification and code generation.

`keel_signature_create_full` accepts natural C scalar/struct/array layouts,
BYVAL and first-parameter SRET contracts. Packed/union/raised-alignment ABI
classification is not supplied by these layout constructors. Explicit call
contracts that repeat fixed parameter metadata are normalized to the signature.
Aggregate varargs that require real operand overrides remain subject to backend
limits, including AArch64's current rejection of those overrides and aggregate
`va_arg` homes. The scalar `keel_build_va_arg` covers promoted scalar extraction.

Overflow instructions return a wrapped value and an i8 flag; they do not call a
panic handler. A frontend chooses its branch and runtime report. Division, shifts,
bounds checks and source-language conversions also need frontend lowering rules.
A trap instruction alone does not provide a language's panic report or cleanup.

Object emission supports AArch64 Mach-O, AArch64 ELF and x86-64 ELF. These native
targets use 64-bit little-endian pointers. Function-local x86 feature permissions
also reach the object encoder. Stable built-in pipeline policies are exposed;
custom Rust selection/allocation callbacks and structured assembly-program
callbacks remain Rust APIs. The standalone C frontend, linker, disassembler,
WebAssembly emission and JIT loading are outside this compiler-facing interface.
Experimental pressure scheduling and adaptive spill-repair policies have no C
option; compilation retains the Rust defaults. Caller-managed parallel codegen
jobs and executors also have no C wrapper.

## Tern integration requirements

[Tern's bootstrap](https://github.com/hxyulin/tern-lang/tree/54d52fedddc6bcd3e527d4a1fc3b38f749af71ae/toolchain/stage1/src)
currently parses and resolves names. It has no type checker or code generator.
Its Rust bootstrap can call the Rust crates directly; this C ABI is also usable
by a later self-hosted frontend or a frontend written in another language.

The [language design](https://github.com/hxyulin/tern-lang/blob/54d52fedddc6bcd3e527d4a1fc3b38f749af71ae/DESIGN.md)
requires floats, checked arithmetic, natural C record calls/returns, varargs,
function pointers, views, source locations and explicit volatile memory access.
The typed API supplies those building blocks for the current native targets.
The compiler client validates equivalent C ABI boundaries, not actual Tern
program lowering. Tern still needs semantic analysis, type/layout lowering,
source-aware panic paths, runtime/library integration and target qualification.

The UART example cannot be qualified through these bindings today: Keel represents
`interrupt`/`naked` function contracts but native codegen rejects their specialized
entry/return lowering. Exposing the flags does not implement interrupt frames,
register preservation or board startup. Packed/over-aligned foreign records and
bare-metal target profiles also need separate ABI/target qualification. This is
enough to start a hosted backend; it is not an end-to-end Tern support claim.

## Validation

```sh
cargo test -p keel-ffi
```

Unit tests exercise invalid inputs, cross-module and cross-function IDs, stale
IDs, ABI extensions/layouts, call validation, memory flags/orderings, symbol
transitions, typing, dominance, transactional IR input, pipeline policies,
attribute consistency, source locations, buffer ownership and panic containment.
Integration tests compile C clients and C++ header consumers, then link and run
generated code on the matching host. They require `cc` and `c++`, or executable
paths in `CC` and `CXX`, and the host POSIX thread library. The DSL integration test
also compiles and runs the sample, checks wrapping arithmetic and precedence,
and rejects malformed input without overwriting an existing object.

The [compiler client](tests/compiler_client.c) and
[consumer](tests/compiler_consumer.c) exercise overflow flags, float/NaN behavior,
C record arguments/returns, direct/indirect aggregate calls, incoming/outgoing
varargs, atomics, portable vectors, inline assembly, memory intrinsics, switch,
TLS isolation across threads, relocated global records and source locations.
They emit all three native object formats before and after optimization; only
the matching host objects are executed. These checks do not qualify every
intrinsic, SIMD shape, debug metadata graph or backend policy combination.
