# LLVM global-object milestone

Run `suite/llvm_import_globals_validate.py --libllvm LIBLLVM23 --keel PRIVATE_KEEL --out NEW_ARTIFACT_DIR` through `suite/fast_opt_locked.py LOG ...`. The validator does not acquire a nested lease. The parent aggregate runner can invoke it with the same three arguments inside its lease. Build Keel with a private `CARGO_TARGET_DIR` and `cargo build --offline --locked -j2 -p keel` under the lease.

The importer helper `suite/llvm_import_globals.py` supports Apple AArch64, little-endian, address-space-zero objects with scalar i1/i8/i16/i32/i64, f32/f64 and pointer fields, arrays, nested nonopaque structs, and packed structs. LLVM TargetData determines allocation sizes, field offsets, array strides, and natural alignment. Explicit padding preserves object layout; padding bytes may be zeroed. Nonempty objects must fit u32; oversized aggregate layouts are rejected before potentially overflowing LLVM size queries. Objects retain constant/writable status and external/internal/private and ODR linkage; declarations emit `decl data`. Unused objects are retained and validated too.

Definitions use the greater of natural and requested alignment, up to Keel's 4096 limit. Nulls and aggregate zero initializers need no relocation. Nonzero pointer initializers require an eight-byte-aligned field offset and object alignment of at least eight: a packed pointer at offset zero can work with explicit align 8, whereas a packed pointer at offset one is rejected. Packed null pointers do not need loader relocations and remain supported.

Function and data addresses work as instruction operands and pointer initializers. Constant GEP expressions support integer indices, LLVM struct offsets/array strides, and named bases; accepted byte addends are unsigned u32 and may reach exactly one past a defined data object's end. Defined-object overrun, negative/oversized addends, function addends and non-GEP symbolic expressions are rejected. `inrange` promises are omitted while retaining exact address arithmetic and bounds checks. External-object bounds are unknown, as in Keel's declaration contract. No symbol subtraction, integer relocations, absolute nonzero pointer initializers, pointer casts, or TLS relocations are implemented. LLVM may canonically fold identity expressions before the importer sees them.

TLS, common, weak-any, linkonce-any, available_externally, aliases, ifuncs, COMDAT, custom sections, unsupported visibility/DLL storage, externally_initialized, reserved `llvm.*` objects, global metadata, sanitizer/partition/code-model attributes, opaque/empty-defined/vector/unsupported-width objects, and undef/poison initializers are rejected for the entire module. A failure, including final Keel verification, publishes no stdout or partial output and preserves an existing destination. The fixture validator checks input bytes on both success and failure, existing and missing destination behavior, and complete stdout/file agreement.

`objects.c` and `objects.cpp` are compiled with actual Clang C/C++ frontends (`-O0 -fno-common -S -emit-llvm`); those emitted inputs are imported unchanged and also compiled by Clang for the reference run. Common linkage is deliberately not implemented. `layout.ll` adds nested aggregate, i1 memory, recursive self pointer, one-past, external addend, private storage, and packed relocation cases. Both native drivers contain independent assertions for values, ABI offsets/size/alignment, address equality, nulls, external resolution, writable state, and function pointers. Each runs against LLVM native output and Keel None/Fast/Full output. The imported LLVM has no indirect calls; the separately compiled native driver calls the exported function pointers.

Floating-point globals, zero initializers, nested FP fields and external FP
objects are supported after merging the scalar FP track. LLVM constant bitcasts
produce integer data atoms without host floating-point conversion. `fp.ll` and
`fp_driver.c` assert exact negative-zero, subnormal and signaling-NaN bits,
nested layout, constant GEP pointers, mutations and external resolution. A
global function pointer also exercises indirect FP calls through both native
and Keel code. Half/bfloat/x87/FP128 and unsupported FP aggregate forms remain
rejected. The validator has 45 negative cases and 12 native tier executions.

`suite/frontend_validate.py` invokes this validator under its host lease. The
old scalar validator now rejects weak global storage rather than ordinary
globals; its remaining guards stay active.

Hidden class/si ODR RTTI and matched type-name strings have a narrow validated
visibility exception; `rtti_name` retains the Apple high-bit tagged relocation.
Generic hidden objects and integer relocations remain rejected. See
[ODR/RTTI transport](../llvm_import_odr_contract.md). Zero-sized external
declarations are accepted for runtime vtables; zero-sized definitions still fail.
