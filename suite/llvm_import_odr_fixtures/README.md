# General exported ODR and user-defined RTTI validator contract

Owned scope: this directory and `suite/llvm_import_odr_validate.py`. This harness
is independent of the legacy `--localize-odr-functions` local-copy tests. Full
mode never passes that flag. No production importer, backend, shared validator,
or shared documentation changes belong to this fixture commit.

Run from the checkout root, using fresh private artifact directories:

```sh
python3 suite/fast_opt_locked.py /tmp/odr-reference-lease.jsonl \
  python3 suite/llvm_import_odr_validate.py --reference-only \
  --out /tmp/odr-reference-fresh

python3 suite/fast_opt_locked.py /tmp/odr-full-lease.jsonl \
  python3 suite/llvm_import_odr_validate.py \
  --libllvm /absolute/path/to/libLLVM.dylib --keel /private/target/debug/keel \
  --out /tmp/odr-full-fresh
```

The caller owns the shared host lease; the harness does not nest a lock. Build
Keel separately under that same lease with `cargo build --offline --locked -j2`
and a private `CARGO_TARGET_DIR`. Compiler and executable commands have bounded
180-second timeouts; waiting for the host lease has no timeout. No performance
claim follows from these tests.

## Positive contract

Both modes compile original C++ with Clang, save untouched emitted LLVM bytes,
and compile those exact modules with Clang for native reference objects. Flags:
C++17, `-O1 -fno-inline -fwrapv -ffp-contract=off`, and
`-Xclang -disable-lifetime-markers`. Assertions remain enabled. There is no
LLVM textual rewriting, pruning, internalization, or ODR localization.

Two mixed modules each define an explicit `weak_odr` template instantiation,
`linkonce_odr` inline helper, inline variable, constant-initialized function-local
static, inline function-pointer variable, constructors, virtual method, vtables,
and user-defined RTTI. Emitted linkage and function-pointer relocations are
asserted, inventoried and saved in `validation.json`. An unused explicit
`weak_odr` instantiation must also survive whole-module import.

The native driver independently checks:

- Inline/template function pointer identity across modules, including the native
  inline definition; helper results for six bounded inputs.
- Shared inline and static storage addresses, initial values and mutations.
- Function-pointer variable address identity, relocation target and calls.
- Vtable identity across both modules and a native object; RTTI address identity
  across both modules/native code; the vtable's RTTI relocation.
- Virtual calls through native objects and module-created objects, with native,
  left and right dispatchers/providers and direct cross-module wrappers.
- All nine pairs of native/left/right thrower and catcher, for normal return,
  user-defined `Error` typed catch and integer throw caught by catch-all: 27
  paths per executable, exact expected result and exactly two cleanups each.
  EH calls are direct; function-pointer calls are used only for ordinary virtual
  dispatch/providers. Indirect `invoke` is outside this contract.

The original `constexpr_runtime.cpp`, `virtual.cpp`, and empty `Error` `eh.cpp`
from `suite/cpp_coverage_fixtures` are compiled unchanged as three additional
modules/executables. Each has six independently expected results, including
unsigned wraparound in the constexpr case.

Reference mode runs **five native executables**: two mixed object link orders
(left-first and right-first, native driver last), and three original shapes.
The repeated mixed link order prevents a single native weak definition from
always masking the selected module's implementation. Reference-only establishes
Clang baseline behavior, **not Keel correctness**. Its report explicitly marks
Keel validation false and lists skipped checks.

Full mode imports all five modules without localization, compares file and
stdin/stdout import results, and requires every original function/declaration
in original order, including unused bodies. A synthesized `_Unwind_Resume`
declaration is permitted. LLVM bytes are rechecked after imports and at exit.
At each of None/Fast/Full it replaces left only, right only, and both modules,
in both mixed link orders, plus each of the three original shapes. This is
27 Keel-linked runs plus five native baselines = **32 native executables**.
Each run checks known values/identity/cleanup counts; stdout comparison is an
additional check, not the oracle.

## Rejection and atomic-output contract (full mode)

Ten narrow LLVM negatives cover weak-any functions/data, externweak
functions/data, generic hidden function/data visibility, protected function
visibility, function/data COMDAT membership and TLS. They include an otherwise
valid strong function. Each must fail with importer diagnostic, no stdout, an
existing destination byte-for-byte preserved and an absent destination still
absent. Two downstream verifier failures (`false` and a missing executable)
exercise the same protocol on positive ODR input. Success must replace the
sentinel destination. Negative IR is generated independently; positive LLVM
bytes are never edited to manufacture acceptance.

Generic hidden symbols remain negative. Compiler-generated `_ZTI*` and `_ZTS*`
RTTI with hidden visibility are part of the positive RTTI contract and need a
specific importer policy; accepting arbitrary hidden functions/data is not
required. COMDAT selection/group semantics, arbitrary weak interposition, TLS,
multiple/virtual inheritance, dynamic_cast, virtual destructors, dynamic local
static initialization, exception payload aggregates and indirect invokes are
not covered. This is an executable integration contract, not broad C++ support.

## Historical baseline and integrated validation

On Apple Clang 21.0.0 (clang-2100.3.34.2), arm64 macOS, the reference baseline
passes. Before integration, the importer at parent a367d868 rejected:

- left/right: `_Z15template_helperIiET_S0_`: unsupported linkage/visibility
  (actual `define weak_odr`).
- constexpr_runtime: `_Z6factorj`: unsupported linkage/visibility
  (actual `define linkonce_odr`).
- virtual: `_ZN7DerivedC1Ev`: unsupported linkage/visibility
  (actual `define linkonce_odr`).
- eh: `_ZTI5Error`: unsupported global linkage/storage/section/COMDAT
  (actual `linkonce_odr hidden` user-defined RTTI).

The full emitted mixed modules additionally require `linkonce_odr` data,
vtable pointer relocations, and hidden RTTI/type-name definitions. Apple Clang
emits tagged non-unique RTTI name pointers of the exact form
`inttoptr (i64 add (i64 ptrtoint (ptr @_ZTS... to i64), i64 -9223372036854775808) to ptr)`.
This cannot be represented by the existing unsigned-u32 symbol-addend path;
it requires explicit RTTI handling or a documented dependency limitation.
The original virtual and EH shapes also emit those tagged RTTI pointers.
Those importer/core dependencies are now integrated. Full mode passed all
32 native executables at `/tmp/keel-odr-integrated-native-r1/`, and again through
`target/frontend-odr-final-2/llvm-odr/`. The latter report sets
`keel_correctness_validated` to true. All 14 default C++ profiles also pass
without localization, including the original optimized virtual fixture with
its lifetime markers. This does not extend the fixture scope to multiple
inheritance, dynamic_cast, dynamic initialization or indirect invokes.
