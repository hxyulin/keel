# Keel C API

`keel-ffi` exposes Keel's Rust IR, optimizer and native backend through a C ABI.
It builds static and shared libraries. Other languages can bind the declarations
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
symbol for a definition with `keel_function_symbol`. Functions default to exported
linkage; `keel_function_set_linkage` can make a definition module-local.

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

Globals can contain copied bytes, zero-initialized storage, or one native pointer
relocation to another symbol plus a nonnegative byte addend. Constant and local
flags control mutability and linkage. External globals can be declared first and
defined later under the same name, preserving their symbols. Byte initializers
are copied verbatim; the frontend supplies target byte order and aggregate
padding. Duplicate declarations/definitions and function/data name collisions
are rejected. Pointer relocation arrays are not exposed by this slice.

The [systems client](tests/systems_client.c) exercises these APIs without adding
a source-language parser. Its [C consumer](tests/systems_consumer.c) calls the
generated functions, supplies external functions/globals, checks memory writes
and callbacks, and executes an array loop. It covers signed and unsigned narrow
ABI values, register/stack call arguments, local function addresses and relocated
pointer globals. The integration test emits all three object formats before and
after optimization and links/runs the matching host objects.

## Ownership and errors

A module owns its functions, blocks, values, signatures, symbols and stack slots.
Their IDs are passed by value and checked for the correct module and, where
applicable, function. Adding IR preserves existing
IDs. Optimization invalidates all IDs once it starts, including on an optimizer
error. Verification failure before optimization starts preserves them. There is
currently no lookup API for the rewritten IR; finish construction before
optimizing and emitting.

Every status-returning operation supplies a module diagnostic through
`keel_module_error`. Success clears it. Outputs are written only on success.
An unexpected Rust unwind poisons the module; read its diagnostic and destroy
it. Further operations return `KEEL_POISONED`. Process faults, allocation failure
and builds using `panic=abort` can terminate the host process.

Output buffers have independent ownership and survive module destruction.
Borrow their bytes with `keel_buffer_data` and `keel_buffer_size`, then release
them with `keel_buffer_destroy`. Never free API allocations through the host
language's allocator. Serialize access to each module. Separate modules can
be used concurrently.

The [header](include/keel.h) defines the full pointer, lifetime and error contract.
Foreign pointer validity is the caller's responsibility. Checked IDs do not
make arbitrary pointers safe.

## Supported surface

Construction supports i8, i16, i32, i64 and pointer values. Functions use the
fixed-arity C calling convention with zero or one scalar return. The convenience
constructor accepts i32/i64/pointer signatures. Explicit signatures also support
i8/i16 parameters and returns with sign or zero extension.

Integer constants, arithmetic, comparisons, casts, block parameters, branches,
loads, stores, fixed stack slots, symbol addresses, direct calls and indirect
calls reuse Keel's Rust IR builders. Pointer offsets are i64 byte counts. Entry
parameters are added in signature order. Add block parameters before instructions.
Loops use ordinary branches with loop-carried block arguments.

Verification checks complete control flow and dominance. Object emission always
verifies first and supports AArch64 Mach-O, AArch64 ELF and x86-64 ELF. These
native targets use 64-bit, little-endian pointers. Optimization calls
`keel_opt::optimize`; it uses the existing optimizer policy. IR text can be
exported during construction for inspection.

Floats, aggregate call ABIs, variadic calls, atomics, thread-local storage,
SIMD, debug metadata, custom optimization pipelines, C source compilation,
WebAssembly and JIT loading are not exposed yet. General relocation arrays,
weak linkage and custom sections also remain outside this interface. Keel's
Rust types remain internal; the C ABI uses explicit integer tags and checked IDs.

## Validation

```sh
cargo test -p keel-ffi
```

Unit tests exercise invalid inputs, cross-module and cross-function IDs, stale
IDs, ABI extensions, call validation, memory flags, symbol transitions, typing,
dominance, buffer ownership, panic containment and all three native object formats. The integration test compiles and links the example as C and
C++, then links and executes its emitted object on supported hosts. It requires
`cc` and `c++`, or executable paths in `CC` and `CXX`. The DSL integration test
also compiles and runs the sample, checks wrapping arithmetic and precedence,
and rejects malformed input without overwriting an existing object.
