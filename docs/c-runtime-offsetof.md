# Runtime offset designators

`__builtin_offsetof` accepts integer expressions in array member subscripts.
This includes nested arrays, anonymous members, union members and flexible
array members:

```c
struct Entry { long key; long value; };
struct Table { char tag; struct Entry entries[8]; };

unsigned long value_offset(int index) {
    return __builtin_offsetof(struct Table, entries[index].value);
}
```

The result uses the target's `size_t` representation: unsigned long is 64 bits
on native targets and 32 bits in the bounded Wasm model. Index arithmetic wraps
in that unsigned domain, including negative and large indices. Each evaluated
index expression runs once. Zero-size elements still evaluate their indices.
Index expressions retain their ordinary reads and side effects; no object of
the designated type is created or dereferenced.

Designators must name actual members and array elements. Pointer-member
traversal, bitfields, missing members and noninteger indices are rejected.
Constant designators remain integer constants for enums, static initializers
and static assertions. A runtime designator does not become an ICE merely
because its numerical result can be predicted while discarding side effects.

The evaluated-operand proof also guards static initializers, preserving final
weak-symbol binding rules. `__builtin_constant_p` remains an unevaluated,
conservative integer query. It may answer zero when another compiler proves a
constant. Clang's observed evaluation of a nested side-effecting offset index
inside `constant_p` is not reproduced; that wrapper is excluded from the shared
compiler-reference harness and checked separately against Keel's unevaluated
contract.

`cargo test -p keel-c --test runtime_offsetof` checks native layouts, private
Wasm arithmetic and execution, static rejection, index side effects and IR
round trips. Its fixtures compare computed offsets with actual member addresses
and independent unsigned arithmetic. X86 exports cover selectors, allocators,
normal/empty pools and optimization on/off, with text/typed/chunked object parity.
