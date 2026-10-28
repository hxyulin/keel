# Freestanding wasm32 output from the CLI

The `keel` driver can emit a complete core WebAssembly 1.0 (MVP) module from one
freestanding C translation unit, or from raw Keel IR that was produced for the
wasm32 storage layout. There is no linker step: the output is a finished `.wasm`
file with one owned memory and the functions you export explicitly.

## Quick start

Build the driver, then compile:

```sh
cargo build --release -p keel

# C source (or already-preprocessed .i)
target/release/keel cc --target wasm32-unknown-unknown --emit-wasm \
  --export checksum --export local_array input.c -o output.wasm

# Raw wasm32 Keel IR
target/release/keel compile --target wasm32-unknown-unknown --emit-wasm \
  --export entry input.keel -o output.wasm

# Opt-in fixed optimization profile
target/release/keel cc --target wasm32-unknown-unknown --emit-wasm --wasm-opt basic \
  --export checksum input.c -o output.wasm
```

Both `--target wasm32-unknown-unknown` and `--emit-wasm` are required. The output
suffix does not select the format. Wasm flags without the Wasm target or without
`--emit-wasm` are errors, and selecting the Wasm target without `--emit-wasm` is
rejected before any native processing.

## Options

| Option | Meaning |
| --- | --- |
| `--target wasm32-unknown-unknown` | Select the Wasm target (exact triple only) |
| `--emit-wasm` | Emit a complete core `.wasm` module |
| `-o path` | Output file. Required; cannot be `-` (stdout) |
| `--export name` | Export the function `name` under exactly that name. Repeatable |
| `--wasm-opt basic` | Run the fixed basic optimization profile (see below) |
| `--wasm-memory-pages N` | Fixed memory size in 64 KiB pages (default 1, max 65536) |
| `--wasm-data-floor N` | First byte eligible for static data (default 1024, nonzero) |
| `--wasm-stack-bottom N` | Lowest stack byte; static data must end at or below it (default 32768) |
| `--wasm-stack-top N` | Initial stack pointer, within memory (default 65536) |

For C input, `-I dir`, `-isystem dir`, `-D name[=value]` and `-U name` keep their
usual ordered semantics.

Option syntax is strict: `-o`, `--export`, `--wasm-opt` and the numeric flags take
a separate argument (`-opath` and `--flag=value` are rejected). Numeric values are
unsigned decimal u32 only: signs, whitespace, hexadecimal, fractions and overflow
are rejected. Repeating `-o` or a memory flag uses the last value. Repeating
`--wasm-opt basic` is harmless. Extra inputs and a standalone `--` are rejected.

### Exports

`--export name` exports that exact function name, with no renaming or underscore
decoration. There are no implicit function exports, and an empty export list is
accepted. Linear memory is always exported as `memory`. Missing, duplicate or empty
export names, and names that collide with `memory`, are errors. C `export_name`,
`import_module` and `import_name` attributes are rejected rather than silently
ignored.

### Rejected native options

Native optimization, pass and report flags are rejected on the Wasm route:
`-O` (native `-O` means the native Full tier plus instruction-selection presets, so
it is not reused for Wasm), `-O0`, `-p`, `--opt-tier` (including `none`),
`--verify-each`, `--no-verify-each` and `--opt-report`. CPU tuning and features,
register allocation and instruction-selection controls, `-g`, `--emit-object`,
`--emit-ir`, `-E`/`-dM`, an explicit sysroot and Darwin deployment controls are
also rejected before any input is read. A later option cannot cancel an earlier
forbidden one. Import and link flags are not implemented and are rejected. This
route never invokes a linker, including `link --system`.

Native commands and target-free `opt` are unaffected.

## Inputs

**C.** Raw C uses the [freestanding C profile](wasm-c-freestanding.md): embedded
portable headers, the ILP32 wasm32 layout, and a whole-translation-unit check that
rejects unsupported features before dead code is pruned. Files ending in `.i` are
treated as already preprocessed. Header directories you pass are your
responsibility; there is no implicit host include path, SDK lookup, libc, native
frontend fallback or external compiler. `SDKROOT` and Darwin deployment
environment variables are ignored on this route.

**Raw IR.** IR has no embedded target. `compile --target wasm32-unknown-unknown
--emit-wasm` asserts that you supplied unoptimized IR with wasm32 storage layout;
the driver cannot detect native-layout IR. Target-tagged `KEEL-IR-UNIT` artifacts
(including native ones) are rejected.

Both routes call `keel_wasm::emit_wasm32_runtime_module`, which verifies the module
with the WASM32 layout, checks the whole module against the supported subset, and
validates the generated MVP bytes before any output file is opened.

## Supported subset

The accepted C and IR subset is defined by the
[C profile](wasm-c-freestanding.md) and the
[`keel-wasm` crate](../crates/keel-wasm/README.md). In summary:

- Supported: integer computation up to 64 bits, ordinary control flow, direct calls
  with plain i32/i64 arguments and results, bounded local frames and recursion,
  and defined static data including data-pointer initializers.
- Not supported: floating point, SIMD, atomics, function pointers, imports,
  unresolved externs, WASI/libc, native objects or assembly, relocatable Wasm
  objects and object linking. Narrow, pointer and aggregate values cannot cross a
  function boundary; there is no general C ABI.
- There is no start function and no implicit call to `main`.

## Memory layout

The module has one owned, unshared memory whose initial and maximum page counts
are equal (no growth), plus a checked software stack.

- `data floor <= stack bottom <= stack top`, and both stack bounds must be
  multiples of 16. The stack top must lie within the configured memory.
- Changing the page count does not move the other bounds automatically.
- Static data placement and recursive frame sizes must fit their regions.
- These bounds govern placement and stack checks only. They do not protect a null
  page or separate objects within linear memory.

Discard an instance after any trap: the saved stack pointer is restored only on
normal returns. See the [runtime contract](wasm-backend.md) and the
[`keel-wasm` README](../crates/keel-wasm/README.md) for exact access widths,
pointer normalization, data placement and stack behavior.

## Basic optimization profile

Without `--wasm-opt basic`, no optimizer runs at all. With it, the driver runs
exactly one fixed sequence of function passes:

```text
const-fold, simplify-cfg, phi-simplify, dce
```

There are no native tiers, canonical-form restoration, host tuning,
vectorization, module passes, custom pass selection or iteration to a fixed point.
The whole module is verified with `DataLayout::WASM32` after every pass, in both
debug and release builds.

Before optimization, the driver runs the backend's non-emitting admission check on
the original module (including unexported functions, signatures and dead code), so
unsupported input cannot become accepted by constant-branch pruning or DCE. The
final emitter then repeats its legality checks and validates the MVP bytes. This
adds a preflight and a module clone; no compile-time or runtime gain is claimed.

Faulting loads, volatile loads (including `volatile notrap`), divisions and other
pinned effects keep their order and exact access widths. DCE may remove an unused
nonvolatile `notrap` load, relying on the producer's promise that it cannot trap.
The optimizer never adds `notrap`; raw IR producers are responsible for that
promise. The design rationale is in the
basic optimization audit.

## Output publication

Frontend, verifier, unsupported-feature, export and memory errors leave any existing
destination untouched. Once validated bytes exist, the driver writes them to a
unique temporary file in the destination directory, syncs and closes it, then
renames it over the destination. On failure the temporary is removed and the old
destination remains. Inputs are unmapped first, so the output path may equal the
input path. A symlink or hard-link destination is replaced as a directory entry;
the file it pointed to is left unchanged.

This is atomic replacement, not a crash-durability guarantee: process termination
or filesystem failure can leave the temporary behind.

## Testing

The `wasm_output` integration test drives the real CLI on C, preprocessed C and raw
IR inputs and runs the results under Node (Node must be on `PATH`). It covers
checksums, local arrays, direct and recursive calls, static data pointers, explicit
exports, fixed memory options, the basic profile, option and export errors,
artifact rejection, in-place and failed outputs, and host isolation (no native
compiler, SDK or linker is discovered or spawned).

```sh
cargo test -p keel --bin keel \
  --test wasm_output --test wasm_target_unavailable --test system_link_args
cargo test -p keel-opt --test wasm32_basic
```

Add `--release` for release builds. The basic-profile fixtures live in
`crates/keel/tests/fixtures/wasm-basic.c` and `wasm-basic.keel`, exercised by
`cargo test -p keel --test wasm_output`.
