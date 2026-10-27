# C frame and stack observers

Keel supports bounded GNU frame queries and read-only x86 stack-pointer
bindings. These use existing IR frame and stack observers; they do not change
the calling convention or add a kernel code-generation mode.

## Frame addresses

`__builtin_frame_address(depth)` accepts checked integer constant expressions
from 0 through 64 on native LP64 targets. Depth zero returns the current frame
record, preserving the existing behavior. Each additional level loads the
saved caller frame pointer from the preceding record.

Nonzero depths require readable caller records using the target's saved-frame
ABI. Omitted frames, tail calls and invalid chains can make a query unsafe;
Keel does not manufacture records or replace invalid memory accesses with null.
The limit bounds emitted loads, not the number of records known to exist.
Wasm and runtime, negative or overflowing depth expressions are diagnosed.
The [GCC contract](https://gcc.gnu.org/onlinedocs/gcc/Return-Address.html)
also makes nonzero-depth queries dependent on the actual calling environment.

## Return addresses

On x86-64 Linux, `__builtin_return_address(0)` reads the saved continuation
address from the current frame. Nonzero or runtime depths remain unsupported.
`__builtin_extract_return_addr` and `__builtin_frob_return_addr` are pointer
identities on this target and evaluate their arguments once. Other targets
remain rejected because their stored return-address representations may differ.
The corresponding builtin capability queries are enabled only on x86-64 Linux.

## Read-only stack-pointer bindings

The x86-64 Linux frontend accepts this file-scope declaration:

```c
register unsigned long current_stack_pointer asm("rsp");
```

Every reference samples the current stack pointer. It creates no global data
object or symbol named `rsp`. Reads remain separate observations across dynamic
stack changes. Writes, increments, address-taking and asm output uses are
rejected. Other global registers, types, qualifiers, initializers and declaration
attributes remain unsupported. Matching declarations may repeat; conflicting
ordinary declarations are rejected. Ordinary local and parameter `register`
storage keeps its existing behavior.

Linux's `ASM_CALL_CONSTRAINT` uses a read/write `"+r"` operand naming this
binding. It is not an ordinary GPR output: it needs a reserved-stack operand
contract. Keel currently diagnoses that form explicitly. Supporting the
declaration's read-only uses does not provide this asm-call contract.

## Inlining

The C frontend's mandatory inline phase rebinds frame and stack observations to
the emitted caller after expansion. This covers `always_inline` helpers in x86
translation units containing asm, including helpers used to form asm immediates.
The ordinary IR inliner still refuses frame and stack observers. Stack
allocation and restoration remain outside the mandatory inliner's admitted
intrinsics. No public intrinsic IDs or backend encoders change.

## Validation

The `frame_address`, `x86_return_address` and `x86_stack_register` C test targets
cover optimized/unoptimized objects, IR round trips and text/typed/chunked
assembly. Native fixtures check caller records, exact CALL continuation labels,
dynamic and aligned frames, inlined observers, wide live values and canaries.
`required_frame_observers` in `keel-opt` checks that the C-specific expansion
does not relax ordinary IR inline guards or admit stack mutation.
