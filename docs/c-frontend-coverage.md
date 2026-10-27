# C frontend support summary

Keel's C frontend (`crates/keel-c`) compiles C11 with a bounded set of GNU
extensions for its two native targets, `aarch64-apple-darwin` and
`x86_64-unknown-linux-gnu`. This page summarizes what is and is not supported
and links to the detailed contract for each feature. It is not a claim of full
C11 or GNU C compatibility.

The general rule: a feature whose semantics a program depends on is either
implemented or diagnosed. Where Keel cannot honor an attribute or construct, it
reports an error instead of silently changing behavior. The main exception is
the permissive catch-all for unknown attributes (see below).

## Language features

| Feature | Status | Details |
| --- | --- | --- |
| Variable-length arrays and VM types | Bounded: multidimensional automatic VLAs, VM typedefs/pointers, runtime type names, dependent parameter bounds, prototype `[*]`. VM declarations combined with goto/switch/`alloca`, VM `typeof`/`_Generic`, VLA initializers and over-aligned VLAs are diagnosed. `__STDC_NO_VLA__` stays defined. | [Plan stages 1-2](c-language-implementation-plan.md) |
| `cleanup` attribute | Bounded: runs on scope exit, return, break and continue, ordered with VLA restoration. Functions with cleanup locals reject goto/switch and known nonlocal exits. | [Plan stage 3](c-language-implementation-plan.md#3-reusable-scope-exit-actions-and-cleanup--bounded-milestone-implemented) |
| Atomics | Native 1/2/4/8-byte integer atomics, `_Atomic float`/`double`, GCC generic buffer builtins, no libatomic dependency. `_Atomic` structs/unions and larger sizes are not supported. | [Plan stage 5](c-language-implementation-plan.md#5-atomic-floats-and-generic-atomics--bounded-native-stage-implemented) |
| Complex numbers | `float _Complex` and `double _Complex` arithmetic and ABI; multiply/divide call the compiler-rt/libgcc helpers. `_Complex long double`, `_Imaginary`, ordered comparisons and complex varargs are diagnosed. `__STDC_NO_COMPLEX__` stays defined. | [Plan stage 6](c-language-implementation-plan.md#6-complex-arithmetic--first-floatdouble-subset-implemented) |
| x86 `long double` | Opaque storage with the target layout; no x87 arithmetic. Apple arm64 `long double` is `double`. | [Plan stage 7](c-language-implementation-plan.md#7-x86-extended-long-double--not-implemented) |
| `__float128`, `_FloatN`, `mode(TC)` | Declarations and layout only (TC on x86-64 Linux); any value, call or ABI use is diagnosed. GCC builtin `_Float32` and friends are not supported. | [Unsupported values](c-unsupported-values.md), [system headers](c-system-header-compat.md) |
| K&R (old-style) definitions | Explicitly typed integer, float/double and pointer parameters with promoted ABI entry conversions. Implicit `int` and aggregate/complex/vector parameters are diagnosed. | [Plan stage 8](c-language-implementation-plan.md#8-kr-definitions--explicit-fixed-size-parameter-milestone-implemented) |
| Inline assembly | AArch64 GNU scalar subset. X86-64 Linux supports scalar constraints, explicit memory operands, flag outputs, basic/file-scope asm and input/clobber-only asm goto. Output-bearing asm goto remains diagnosed. | [C inline assembly](c-inline-asm.md), [x86 contracts](x86-c-inline-asm.md) |
| Frame and stack observers | Native frame addresses at constant depths 0..64; x86-64 Linux return address at depth 0 and read-only global `rsp` bindings. Exact `+r` RSP operands support balanced asm calls with an ABI call barrier. | [Frame contracts](c-stack-observers.md) |
| GNU vectors | `vector_size(16)` on all targets, `vector_size(32)` on x86-64 with AVX, plus bounded scalar broadcasts and `__builtin_shufflevector`. | [GNU vectors and modes](gnu-vector-mode.md) |
| Integer `mode` attribute | `QI`, `HI`, `SI`, `DI`, `TI` (and `byte`, `word`, `pointer`). | [GNU vectors and modes](gnu-vector-mode.md#integer-modes) |
| x86 intrinsic headers | Embedded `xmmintrin.h`, `emmintrin.h`, `smmintrin.h`, `avxintrin.h`, `avx2intrin.h`, `wmmintrin.h`, `immintrin.h` with a finite name list. | [C x86 intrinsics](c-x86-intrinsics.md) |
| `restrict` | Top-level restricted pointer parameters of a definition become an optimizer access contract. | [C restrict](c-restrict.md) |
| Overflow builtins | Fixed-type `__builtin_{s,u}{add,sub,mul}*` and generic `__builtin_{add,sub,mul}_overflow` through 128 bits. `_overflow_p` is not supported. | [Generic overflow builtins](generic-overflow-builtins.md) |
| Memory builtins | `__builtin_memcpy`/`memmove`/`memset` lower to IR intrinsics; fortified `_chk` variants call the checked runtime. | [Memory builtins](memory-builtins.md) |
| Nested functions | Not supported. | [Plan stage 10](c-language-implementation-plan.md#10-gnu-nested-functions--not-implemented) |
| Blocks | Block-pointer declarators (`^`) parse as pointers; block literals and the Blocks runtime are not implemented. | [Design](design.md#c-frontend) |

Driver/preprocessor flag compatibility (`-std`, `-fvisibility`, and similar) is
deferred; Keel uses a fixed C11 predefined-macro profile.

## Attributes

GNU `__name__` spellings are accepted wherever the plain spelling is.

### Implemented

| Attribute | Notes | Details |
| --- | --- | --- |
| `visibility("default")`, `visibility("hidden")`, `visibility("internal")` | See [visibility](#visibility). Internal is emitted as hidden. | |
| `weak` | External function/object definitions and undefined references, on both targets. | [C weak symbols](c-weak-symbols.md) |
| `section("name")`, `retain` | Static-storage objects (including bounded TLS) and function definitions. | [Sections and retention](c-sections-retain.md) |
| `constructor`, `destructor`, `used` | Default priority only; `used` is a compiler emission root, not linker retention. | [C lifecycle](c-lifecycle.md) |
| `cleanup(fn)` | Automatic block objects only. | [Plan stage 3](c-language-implementation-plan.md#3-reusable-scope-exit-actions-and-cleanup--bounded-milestone-implemented) |
| `always_inline`, `noinline`, `noreturn`, `nothrow`, `returns_twice`, C `_Noreturn` and `inline` | Map to IR function attributes; merged across redeclarations. | [Function attributes](function-attributes.md) |
| `keel_noninterposable` | Keel extension; advertised by `__has_attribute`. | [Function attributes](function-attributes.md#explicit-non-interposable-binding) |
| `vector_size`, `mode` | Bounded sizes and modes. | [GNU vectors and modes](gnu-vector-mode.md) |
| `aligned`, `packed` | Existing alignment and packing handling. | [Design](design.md#c-frontend) |
| `nonstring` | Retained on character-array/pointer declarations and members for checked attribute queries. No string-warning analysis. | [Feature queries](#feature-queries) |
| Function `alias` | X86-64 Linux only. Whole-unit checks require compatible function types/contracts and an ultimately defined target. Weak binding, visibility, chains, assembler names and target retention are preserved. Object aliases and lifecycle/retain/section attributes on aliases are rejected. `__has_attribute(alias)` remains conservatively false. | [Alias tests](../crates/keel-c/tests/function_aliases.rs) |

### Accepted without semantic effect

Diagnostic and advisory annotations are accepted and have no code-generation
effect: `format`, `nonnull`, `deprecated`, `warn_unused_result`, `unused`,
`malloc` and `alloc_size`.

Unknown attributes are also accepted silently. This permissive catch-all is
a known gap: the frontend has not classified every GNU/Clang attribute, so an
unknown attribute whose semantics a program relies on may be ignored.

### Diagnosed

| Attribute or form | Reason |
| --- | --- |
| Object `alias`, `weakref`, `weak_import`, `ifunc` | Need symbol identity semantics beyond the checked x86-64 Linux function-alias subset. |
| `visibility("protected")`, unknown or malformed visibility strings | `STV_PROTECTED` is not supported. |
| Conflicting explicit visibilities across declarations | Matches Clang. |
| `weak` on static symbols, TLS, typedefs, members, parameters, locals | Unsupported or meaningless placements. Hidden/internal weak undefined references are also rejected. |
| `constructor`/`destructor` with a non-default priority | Only priority 65535 is supported. |
| `ms_abi`, `sysv_abi`, `stdcall`, `fastcall`, `regparm`, `naked`, `interrupt` and other calling-convention attributes | Would otherwise silently use the C calling convention. |
| `ext_vector_type`, `transparent_union` | Distinct type and ABI semantics are not implemented. |
| Lifecycle, cleanup, section or visibility attributes in inapplicable positions | Rejected even where GCC/Clang only warn and ignore them. |

Diagnostics apply to prototypes, late redeclarations and unused code as well as
definitions.

### Feature queries

`__has_attribute` stays conservative: `visibility`, `weak`, `cleanup` and the
bounded vector/mode attributes report **0**, so feature-tested sources keep
their fallbacks. Explicit use of the supported subsets still works.
`keel_noninterposable` reports 1. Clang-preprocessed input reflects Clang's
answers instead and can enable semantics Keel does not support.

`__has_attribute(nonstring)` and `__has_builtin(__builtin_has_attribute)` report
1 for the checked `nonstring` query subset. `__builtin_has_attribute(expr,
nonstring)` produces an unevaluated `_Bool` constant. Declaration/member
metadata survives redeclarations and identity address expressions; casts,
pointer arithmetic and array elements do not acquire the object's annotation.
Unannotated types report false. Other queried attributes and malformed
`nonstring` annotations diagnose rather than returning a guessed result.

## Visibility

- Visibility is recorded on the symbol and merged across declarations: a
  definition inherits it from an earlier declaration (as with Lua's
  `LUAI_FUNC` and `LUAI_DDEC`), and a later redeclaration may add it.
- Nonlocal definitions with hidden or internal visibility get the IR `hidden`
  flag (functions and data, including zero-initialized, constant and
  thread-local objects). This is emitted as `.hidden` (ELF `STV_HIDDEN`) on
  x86-64 Linux and `.private_extern` on AArch64 Mach-O, in both assembly text
  and objects.
- Internal is emitted as hidden: linkers treat `STV_INTERNAL` as at least
  hidden, and dropping the extra promise is conservative.
- Static symbols stay local; visibility has no effect on them. References to
  symbols not declared hidden in the translation unit stay default.
- Strong hidden/internal function definitions on ELF permit direct-call body
  substitution. Darwin visibility alone supplies no optimization promise. See
  [C semantic interposition mode](function-attributes.md#c-semantic-interposition-mode).
- `#pragma GCC visibility` and `-fvisibility` are not supported.

## Real-world code

- Clang-preprocessed Linux Lua 5.4.7, QuickJS and SQLite inputs (44
  translation units, including `visibility("internal")` and unused `__weak__`
  prototypes) compile for `x86_64-unknown-linux-gnu` with and without `-O`.
  Their output equals that of the same inputs with those attributes removed,
  apart from the added `.hidden` directives / `st_other` bytes.
- A Lua interpreter built from those objects runs on x86-64 Linux; with
  `-Wl,-E` it exports the `lua_`/`luaL_` API and none of the internal `luaX_`
  functions.
- With the [binary128/TC declaration checkpoint](c-system-header-compat.md#implemented-declarationlayout-checkpoint),
  complete builds of all three applications from raw sources against glibc 2.44
  link with GCC and Clang and pass their correctness tests. GCC-preprocessed
  input still stops at GCC's builtin `_FloatN` types. See
  [the results](provenance/linux-binary128-tc-layout.md).

Libraries that unconditionally require protected visibility, other startup
priorities or unsupported section forms stop with a diagnostic. They need
source/configuration fallbacks or real support; removing those annotations
mechanically can change behavior.

## Tests

```sh
cargo test -p keel-c
cargo test -p keel --test preprocess --test hidden_visibility
cargo build -p keel
python3 crates/keel-c/tests/fixtures/check_attribute_semantics.py \
  --keel target/debug/keel --out-dir "$(mktemp -d)"
```

`crates/keel-c/tests/attribute_semantics.rs` checks rejected names, declaration
placement, late redeclarations, block declarations, unused inline functions,
weak declaration merging, malformed and conflicting visibility, and hidden
flags in IR and both code generators. `crates/keel/tests/hidden_visibility.rs`
checks hidden symbols in assembly text and objects for both targets and, on a
native host, links a Keel object with a Clang-compiled caller and checks that a
shared library does not export them.

The script compiles `crates/keel-c/tests/fixtures/attribute-library.c`, a
small synthetic library (default-visible API declarations, an
internal-visibility function, an unused weak prototype, format/allocation
prototypes, a deprecated wrapper and feature-query fallbacks), plus weak
probes, for both targets with and without optimization, using both native and
Clang preprocessing. On a supported native host it links and runs the results
against Clang, and it requires the expected CLI failures (with no output
object) for a weak static declaration, an alias, a constructor declaration,
file-scope cleanup and protected visibility. It needs a built `keel` CLI and
Clang. Passing it is not a claim that any complete external project works.
