# C executable lifecycle and compiler emission

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
`SHF_GNU_RETAIN`. [Custom sections and distinct `retain`](c-sections-retain.md)
are now implemented for a bounded non-TLS definition subset. Lifecycle
array references keep their callbacks reachable in included objects; system
Darwin linking is tested with `-dead_strip`. An archive still needs ordinary
symbol demand (or an explicit linker whole-archive option) to include its member.
Registration relocations and Darwin callback addresses reference the bound
symbol, preserving ordinary weak override/interposition behavior.

## Darwin arm64

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

## ELF x86-64

Constructors and destructors use pointer-aligned `.init_array`/`.fini_array`,
`SHT_INIT_ARRAY`/`SHT_FINI_ARRAY` and `SHF_ALLOC|SHF_WRITE`. The native linker
retains them and emits dynamic array tags for dynamic output. Dispatch requires
the loader/libc startup and shutdown protocol described in [ELF linker](elf-linker.md#executable-initialization-and-termination).
A syscall-only `_start` does not arrange libc lifecycle dispatch; static startup
must explicitly invoke the arrays. This change does not supply CRT objects,
standard libc driver startup, DSO output or legacy `.ctors`/`.dtors` conversion.
Actual ELF objects and native linked static images are inspected on the arm64
host; these x86-64 images are not executed there.

## IR, artifacts and validation

Typed function attributes record default lifecycle registration and function
`used` roots. Darwin C lowering represents shutdown as an ordinary IR `atexit`
thunk with constructor metadata; ELF retains destructor metadata directly.
Raw Darwin IR with a destructor flag diagnoses at native codegen unless it has
received the frontend atexit lowering. Text roundtrips preserve the metadata; artifact dialect `KEEL-IR-UNIT 4` rejects
older versions. LLVM and Cranelift export diagnose lifecycle metadata rather
than silently dropping it. LLVM export preserves function `used` through
`llvm.compiler.used`: optimized LLVM object emission keeps the function, while
system Darwin linking with `-dead_strip` may still remove it. Feature-query
macros remain conservative (zero).

`crates/keel-c/tests/lifecycle.rs` covers malformed contracts, late declarations,
compiler emission, optimized/debug IR and both text/direct object paths. Darwin
execution compares Clang with system and native linking, startup/shutdown order,
return/exit, multiple translation units and demanded archive members. Mixed Clang
strong callback overrides run through both linkers; mixed Clang lifecycle thunks
run through system linking and assert the native DSO-handle diagnostic. ELF tests
inspect section types/flags/alignment/relocations and native image callback
pointers, including ordinary weak strong overrides. Artifact tests preserve all
three function flags. Remaining lifecycle work: general priorities, broader archive policy and
DSO/static-startup support. Custom section/retention limits are documented separately.

## Lifecycle review validation

The review adds regressions for definition order versus forward-declaration
order in both lifecycle phases, compatible empty K&R definitions with an
explicit `void(void)` declaration, qualified `void`, unreferenced Darwin runtime
interface conflicts, pointer-type erasure, asm aliases and generated wrapper
symbol collisions, including pruned asm-labeled declarations still referenced
by inline assembly. Native arm64 execution compares the ordering regression with
Clang through system `-dead_strip` and native links. Mixed weak/strong callback
overrides run in both object input orders. A separate LLVM/Clang `-O2` object and
system `-dead_strip` executable check distinguishes compiler emission from linker
retention. The supported runtime scope and remaining limits above are unchanged.

Review validation on the macOS arm64 host uses the installed GNU cross
assembler for GNU x86 oracle checks; the system macOS assembler does not accept
GNU `--64`:

```sh
KEEL_GNU_AS=/opt/homebrew/bin/x86_64-elf-as \
KEEL_TEST_GNU_AS=/opt/homebrew/bin/x86_64-elf-as \
cargo test --no-fail-fast -p keel-c -p keel-ir -p keel-codegen \
  -p keel-asm -p keel-asm-ir -p keel-toolchain -p keel-llvm -p keel-opt -p keel-link
cargo test -p keel-c
cargo test --manifest-path crates/keel-cranelift/Cargo.toml --locked \
  --target-dir target/keel-cranelift
```
