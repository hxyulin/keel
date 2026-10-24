# C declarations under the wasm32 model

Implementation note for how `keel-c` parses declarations, GNU modes and string
initializers under the wasm32 ILP32 model, and which native-only features it
fences off. It follows the [C data model](wasm-c-layout-foundation.md) and
[integer evaluation](wasm-c-integer-evaluation.md); the public behavior is in the
[freestanding C profile](wasm-c-freestanding.md).

## GNU integer modes

GNU `mode(word)` and `mode(pointer)` resolve to the model's pointer width,
independently of the fixed-width `DI`:

| Mode | wasm32 | native LP64 |
| --- | --- | --- |
| `DI` | `long long` | `long` |
| `word`, `pointer` | `int` | `long` |
| `QI`/`byte`, `HI`, `SI`, `TI` | unchanged | unchanged |

Signedness is preserved (`unsigned long long`, `unsigned int` on wasm32), as are
double-underscore spellings, attribute positions and conflict checks. Because `DI`
and `word` differ on wasm32, declaring both on the same type is a conflict there,
while on LP64 they coincide.

## String initializers

String initializer element widths use the model. Four-byte storage alone does not
establish wide-character identity: on wasm32, `int` arrays accept wide strings while
`long` and `unsigned int` arrays are rejected, matching Clang. `_Bool` arrays
initialized from narrow strings are rejected. Wide literal elements are `int` on
wasm32. LP64 keeps its existing target-based element selection. Narrow strings,
inferred array lengths and the existing truncation and terminator rules are
unchanged.

## Parsing with a selected model

`Parser::declarations_with_layout(src, target, text, layout)` selects the model
before parsing, so the record table, sizes, alignments, array bounds, type
identities, offsets and initializer queries all use it. The `target` argument is a
native parser profile used for unrelated defaults, not a Wasm target or ABI.

## Native-only features

The following reject the wasm32 model instead of borrowing native layout or ABI
behavior:

- vector type construction and scalar broadcast;
- the x86 immediate-operand and vector-broadcast constant proofs (they return
  `None` before consulting native width or evaluator helpers);
- x86 builtin families;
- inline assembly and asm linker labels;
- `va_list` (other than the opaque declaration type described in the C profile),
  `long double` values and `mode(TC)`;
- packed records and bit-fields.

Native ABI classifiers keep their LP64 guards.

Initializer *parsing* (converted scalar values, inferred array types, flattened
member offsets) uses the model here. Initializer *serialization* into bytes and
relocations is covered in [pointers and data](wasm-c-pointer-data.md).

## Testing

Unit tests parse the maintained fixture
[`wasm-scalar-declarations.c`](../crates/keel-c/tests/fixtures/wasm-scalar-declarations.c)
in alternating LP64 and WASM32 order within one arena. They check typedef, literal
and conversion identities, struct and union layouts, nested array strides and
`offsetof`, literal-dependent bounds, designated initializer offsets, string array
completion, aligned recursive records and anonymous unions. Further tests cover GNU
mode signedness, spellings, attribute positions and redeclaration errors, the
native-feature rejections above, and wide-string identity. The same fixture is
checked by Clang for both wasm32 and the native target; its Clang command is at the
top of the file.

```sh
cargo test -p keel-c
```
