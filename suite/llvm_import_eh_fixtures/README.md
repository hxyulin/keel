# Bounded LLVM EH milestone

`catches.cpp` is compiled by the validator with Clang++ `-O1 -fwrapv
-Xclang -disable-lifetime-markers -S -emit-llvm`. Every byte of its output is
passed to the importer. `catches.clang.ll` is also an unmodified output of that
command (Apple clang 21.0.0, source path relative to the repository root).
The snapshot and freshly emitted module are independently imported and tested.
`driver.cpp` supplies native external int/double throws and asserts catch
selection, exception payloads, cleanup counts/arguments and rethrow propagation.
`edges.ll` separately exercises normal result PHIs and shared unwind PHIs,
including values live across the throwing call.

Run from the repository root, with a private Keel binary and new output path:

```sh
python3 suite/fast_opt_locked.py /tmp/eh-lock.jsonl \
  python3 suite/llvm_import_eh_validate.py \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel /tmp/eh-target/debug/keel --out /tmp/eh-validation
```

The validator has no internal host lease. Hold `fast_opt_locked.py` around it;
do not nest it inside `frontend_validate.py`. Builds use `cargo build --offline
--locked -j2 -p keel --target-dir /tmp/eh-target` under the same lease.

The importer accepts direct C invokes with ordinary scalar/call ABI checks,
unpacked `{ptr, i32}` landingpad results, ordered typed catches and catch-all,
optional cleanup, the typeid intrinsic for a named caught global, scalar
extractions, and acyclic PHIs merging only these pairs. Resume keeps the
original exception pointer and lowers to `_Unwind_Resume` followed by trap.
Normal edges are split so invoke results can reach LLVM PHIs after returning.
Shared unwind blocks with PHIs get one identical landingpad per incoming
invoke, followed by the original PHI copies and body. No clause is removed or
reordered. Both scalar components of an EH pair remain together across PHIs.

Only a direct external `i32 @__gxx_personality_v0(...)` runtime declaration is
canonicalized to Keel's symbolic personality declaration; any other use of
that symbol is rejected, including addresses in initializers. This does not
provide ordinary variadic calls. RTTI must be external named global objects;
definitions (including weak/ODR definitions) remain unsupported. The limit is
63 distinct caught types including catch-all per function.

Indirect invokes, filters, Windows funclets, other personalities, ordinary
aggregate SSA, cyclic EH pair PHIs, reconstructed/insertvalue pairs, and
undef/poison pairs remain rejected. This does not import arbitrary C++ modules:
ODR definitions, unsupported intrinsics and ABI contracts
still fail closed. No backend code is changed.

The original LLVM reference objects use Clang `-O1`. On the installed Apple
clang 21, an earlier version of `edges.ll` with identical constant PHIs on
normal and unwind edges emitted an unwind PHI reload from a stack slot only
written on normal return when compiled without optimization, giving a wrong
reference result. The final fixture uses distinct computed unwind inputs. `-O1` compiles the unchanged source correctly against the independent
assertions. This is a reference compilation constraint, not an importer IR
rewrite. All Keel tiers (None/Fast/Full) are checked against the passing
reference and the same independent assertions. Validation artifacts retain
all commands, diagnostics and source hashes in `validation.json`.
