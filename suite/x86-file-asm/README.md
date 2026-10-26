# File-scope assembly execution

Export Linux x86-64 objects with plain Cargo:

```sh
KEEL_X86_FILE_ASM_EXPORT=/tmp/keel-file-asm-objects \
  cargo test -p keel-c --test file_asm
```

Run on Linux x86-64 with GCC:

```sh
python3 suite/x86-file-asm/run.py \
  --objects /tmp/keel-file-asm-objects \
  --source crates/keel-c/tests/fixtures/x86_file_asm.c \
  --work-dir /tmp/keel-file-asm-native --compiler gcc
```

The 48 explicit-V1 exports cover three selectors, both allocators, ordinary and
empty pools, optional optimization on/off, and debug info on/off. Text, typed and
chunked object routes must agree. The fixture combines ordered basic asm,
macro expansion, numeric labels, a weak forward alias, an assembly-defined SysV
function, used static definitions, nested section stacks, absolute and PC-relative
relocations, initialized and zero-filled `.init.data`, and `.init.rodata`.

The C oracle checks 512 runtime input pairs, mutable global state, zero bytes,
assembly-defined data, function/data pointers and a signed relative record.
An independently GCC-compiled reference must pass the same oracle. Six controls
corrupt the function body, alias, pointer record, relative target, assembly data
or used object. Each must return normal FAIL/exit 1; faults and timeouts fail.
These are userspace checks, not kernel linking or boot qualification.

The assembler comparison checks section bytes, flags, alignment, symbol metadata
and relocation targets/addends against Clang and GNU as. GAS rewrites the weak
alias call through its `.set` target; Keel and Clang preserve alias identity.
The test names this difference and compares the other fields exactly.

At implementation `96680637`, all 48 exported objects and the GCC 13.3.0
reference passed. All six controls returned normal FAIL/exit 1. The fresh local
exports match the exact bytes executed on Linux. Native Clang execution was not
run. The independent Clang/GNU assembly comparisons passed with the explicit
weak-alias difference described above.
