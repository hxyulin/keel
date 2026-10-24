# wasm32 target identity and native-route gates

`keel_ir::Target::Wasm32UnknownUnknown` makes `wasm32-unknown-unknown` a recognized
target. Recognizing the target does not make every pipeline accept it: native code
generation, native output, the target-tagged artifact format and the general
optimizer reject it explicitly. Wasm output goes through the dedicated
[CLI route](wasm-cli.md) and the [freestanding C profile](wasm-c-freestanding.md).
This page records where the target is accepted and where it is deliberately
rejected. The design came from the
[layout plan](wasm32-layout-plan.md#variant-and-cli-migration-gates).

## Public contract

- `Target::Wasm32UnknownUnknown` parses exactly `wasm32-unknown-unknown`. There is
  no WASI, wasm64 or shortened alias.
- `Target::data_layout()` is a const storage query: Wasm returns
  `DataLayout::WASM32`, the three native targets return `DataLayout::NATIVE64`. It
  is not a C data model, ABI classifier, optimizer context or backend selector.
- `Target::ALL` lists all four **recognized** targets (the original three, then
  Wasm). It is not a list of usable codegen targets. `Target::NATIVE` lists the
  three native targets for native-only inventories and tests.
- `Target::host()` and `Default` are unchanged. `Module` has no target field.

## Native routes that reject Wasm

Unsupported routes return errors instead of falling back to native behavior or
panicking. Rejections leave caller-owned outputs untouched and create no reports
or output directories.

- **C frontend:** `keel_c` selects the Wasm profile only through its dedicated
  freestanding path. `profile::compile` rejects Wasm before lexing. Native parser
  fallbacks (va_list, char/long-double policy, aggregate classification, debug
  types, headers) are not reachable for Wasm.
- **Native codegen:** `keel_codegen` text, options, pipeline, typed-program and
  chunked-program APIs return `CodegenError` before instruction selection. CPU
  tuning, selector defaults, pipeline validation and optimizer capability
  projection reject Wasm too, and a permissive custom selector cannot bypass this.
- **IR sections and artifacts:** native section validation rejects Wasm, even for
  an empty module. Toolchain artifact encode/decode reject Wasm before verification
  or parsing. A target-tagged IR unit is not a Wasm object.
- **CLI:** `cc` and `compile` without `--emit-wasm`, and `opt`, reject an explicit
  Wasm target after option parsing and before tuning, input loading, include
  discovery, optimization or output. Native CPU, optimization, selection and
  allocation flags cannot enable Wasm. Target-free `opt` keeps its Portable
  context. `asm`, Keel's own `link` and `archive` also reject Wasm; assembler,
  linker and archive APIs use their own closed native enums.
- **Optimizer:** `keel-opt`'s `VectorTarget` describes SIMD capability, not memory
  layout, so its general APIs cannot check a target/layout pairing. Wasm modules
  only reach the optimizer through the fixed, WASM32-verified
  [basic profile](wasm-basic-optimization-audit.md).

The LLVM exporter and the experimental Cranelift crate keep their own native-only
target profiles.

## Testing

| Crate | Tests |
| --- | --- |
| `keel-ir` | `targets`, `data_layout`, `sections` |
| `keel-codegen` | `wasm_target_unavailable`, `selection`, `tuning`, `x86_features` |
| `keel-c` (`--features profile`) | `wasm_target_unavailable`, `gnu_types`, `gnu_vector_broadcast` |
| `keel-toolchain` | `foundations` |
| `keel` | binary unit tests, `wasm_target_unavailable`, `system_link_args` |

```sh
cargo test -p keel-ir --test targets --test data_layout --test sections
cargo test -p keel-codegen --test wasm_target_unavailable
cargo test -p keel --test wasm_target_unavailable --test system_link_args
cargo check --workspace --all-targets --features keel-c/profile
```

## System-link compatibility boundary (revised contract)

`keel link --system ...` is an explicit, opaque pass-through to Clang. Clang owns
target selection, repeated options, option operands, response files, configuration
files and environment interpretation. Clang may therefore select Wasm on this
route; that does not enable Keel's Wasm frontend, backend or native linker. Keel
does not expand or validate response or config files here (nested, quoted,
malformed, missing and cyclic response files are all forwarded), and it has no
Clang option table.

**Where to put `--system`.** Put it immediately after `link` for the full Clang
argument surface. It may also follow a prefix of Keel native arguments: Keel skips
the operands of its own value-taking options while looking for it. For example,
`link --target x86_64-unknown-linux-gnu --system ...` selects Clang, whereas
`link -o --system input.o` names a native output file called `--system`. A
standalone `--` or any option outside Keel's native syntax ends the search, so a
later `--system` does not switch routes. In particular, put `--system` **before**
Clang-only options such as `-Xlinker`, `-Xclang` and `--config=...`. Response-file
contents cannot select the route.

**What is forwarded.** Only the selected `--system` token is removed. Every other
argument keeps its order and bytes, including later `--system` tokens and anything
after `--`, except the existing literal translation of `--sysroot` and
`-syslibroot` to `-isysroot`. That legacy translation applies to direct argv tokens
(even after `--`), not to joined spellings or response-file contents. The
environment is inherited.

**Native route.** Without `--system`, Keel rejects any genuine
`wasm32-unknown-unknown` in the four target spellings (`--target VALUE`,
`-target VALUE`, `--target=VALUE`, `-target=VALUE`) before native input/output
processing or tool discovery. A later native target does not cancel an earlier Wasm
request. Operands of known native options and filenames after `--` are not treated
as targets.

The scope follows the LLVM 21.1.0 driver: the
[driver entry point](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/tools/driver/driver.cpp)
expands response files and handles argument-rewriting environment variables;
[Driver.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/lib/Driver/Driver.cpp)
loads configuration files and computes the target;
[Options.td](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/include/clang/Driver/Options.td)
defines version-specific operand consumption; and
[CommandLine.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Support/CommandLine.cpp)
implements response-file tokenization and nesting. Scanning argv for a target
substring therefore cannot determine the external driver's effective target, which
is why the route is opaque rather than filtered.

`cargo test -p keel --test system_link_args` uses an isolated fake Clang that
records its arguments. It covers all four target spellings, repeated targets,
native-prefix selection with value-taking options, target-looking output names,
literal `--system` operands, `--`, Clang-only options, and nested, quoted,
malformed, missing and cyclic response files. These are forwarding checks, not
claims that Clang accepts every fixture. Native Wasm cases assert that no driver
ran and the destination is unchanged. The `elf_link` and `macho_link` tests keep
native controls, including outputs and `--`-protected inputs named like target
options.
