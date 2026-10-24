# Ordinary C weak symbols

Keel supports `__attribute__((weak))` (and `__weak__`) on external function
and object definitions, tentative object definitions, and undefined references.
The supported native targets are AArch64 Mach-O and x86-64 ELF. The attribute
must have no arguments. Static symbols, automatic variables, members, parameters,
typedefs and type names cannot carry it. Weak TLS is rejected. Hidden/internal
weak **definitions** work within a linkage unit; hidden/internal weak undefined
references are rejected. `weak_import`, `weakref`, aliases and ifuncs remain explicit errors.
[Bounded executable lifecycle](c-lifecycle.md) and
[custom sections and retention](c-sections-retain.md) are implemented separately. Unknown `weak*`
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

## Validation

`crates/keel-c/tests/weak_symbols.rs` checks optimized IR roundtrip, structured
and textual object equality on both targets, symbol binding and symbolic
relocations against Clang on arm64 and cross-generated x86 ELF, ELF link output
and archive selection, and arm64 execution with both Apple and Keel linking.
The arm64 matrix uses native structured, native textual and mixed Clang objects
for duplicate weak definitions, strong overrides in both orders, pointer
identity, absent and provided weak hooks/data, and archive demand. Additional
IR, optimizer, codegen, exporter and artifact tests cover invalid combinations
and interposition conservatism. CLI fixtures compile and execute guarded weak
references and retain diagnostics for invalid static weak declarations.

x86 ELF objects and native ELF images are generated and inspected on arm64;
x86 execution is not claimed. Clang and `ar` are test dependencies; arm64 native
runtime tests additionally require macOS system linking and code signing.

Run the focused suite with `cargo test -p keel-c --test weak_symbols`.
