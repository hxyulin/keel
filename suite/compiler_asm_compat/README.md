# Real compiler assembly through `keel asm`

Checks whether Keel assembles real `rustc` and `clang++` assembly output the same way the compilers'
own integrated assembler does. Nothing in the `.s` is edited before the A/B.

- `fixtures/cpp/*.cpp`: header-light C++20 (only `extern "C"` libc/pthread declarations), so the same
  sources cross-compile to `x86_64-unknown-linux-gnu` without a sysroot. Covers templates/constexpr/
  lambdas, classes/inheritance/virtual dispatch/RTTI, static init/guards/atexit, `__atomic` builtins,
  vectorizable loops, RAII + exceptions, and `thread_local` with dynamic init across threads.
- `fixtures/rust/*.rs`: `extern "C"` exports (scalars/loops/slices/enums), std Vec/String/BTreeMap/
  generics/trait objects, `panic=unwind` with `catch_unwind` and Drop, and `thread_local!`/threads/atomics.
- `run.py`: the matrix is {fixture} x {O0,O2} x {no debug, `-g` DWARF 5} x {AArch64 Mach-O, x86-64 ELF
  AT&T, x86-64 ELF Intel}. Each case runs in its own child process.
- `objinspect.py`: Mach-O/ELF structure normalization (sections, relocations, symbols).
- `test_harness.py`: command selection across all 132 cases and focused evidence-classification tests;
  runs without compilers or native programs. The harness supports Python 3.9+.

Acceptance, object comparison, debug metadata checks, and runtime checks are separate evidence.
Object verdicts distinguish byte identity, strict normalized structure equality, equality modulo
documented Mach-O differences, code/data-only equality, and structural differences. Strict structure
requires equal alignment, entry sizes, format/CPU/header fields, symbols, and normalized relocations.
Mach-O temporary section locals (`ltmp<N>` / `ltmp_Lkeel_metadata_section<N>`) and relocation attribute
bits are allowed only in the separately named benign tier. ELF `.llvm_addrsig` symbol indexes are
resolved to symbol records, preserving changes to the significant symbols; `.comment`, `.llvm_addrsig`,
and `.note.GNU-stack` are metadata. Debug, unwind, and other metadata differences remain visible and
prevent full structure equality. Meaningful encoding, alignment, symbol, or relocation differences
are never waived.

The AArch64 runtime suffix requires identical stdout/stderr/exit status and a reference that exits 0.
Rust also cross-checks the original rustc-built executable. Runtime equivalence does not imply object
equivalence. `-g5` cases independently report DWARF dump/verification and Mach-O dSYM results; failed,
different, or unchecked metadata qualifies the verdict even when runtime matches. Length-masked DWARF
comparison is diagnostic only. Normalized structure is not a claim of complete object equivalence.

The assembly hash is recorded before the reference, before Keel, and after all tools; a changed assembly
or fixture invalidates the verdict. Full command stdout/stderr is retained in JSON. A rejection reports
its real source line and feature category, including for AT&T. Optional diagnostic chains blank lines
only in a scratch copy that is never compared, linked, or executed as compatibility evidence.

```
python3 suite/compiler_asm_compat/run.py \
    --keel target/debug/keel --out target/compiler-asm-compat \
    --no-timing --chain-limit 0                 # add --only rust-panic or --report-only

PYTHONDONTWRITEBYTECODE=1 python3 suite/compiler_asm_compat/test_harness.py
```

Notes:
- Keel receives explicit `--x86-syntax att` / `--x86-syntax intel`; compiler-default AT&T and
  compiler-requested Intel (`clang++ -masm=intel` / rustc LLVM syntax option) remain separate cases.
- `rustc` 1.99's LLVM is newer than Apple clang 21's. Only a reference rejection of x86 Rust
  `.prefalign` as an unknown directive permits a fallback to rustc's own `--emit obj` from the same
  compilation. The report records the original Clang rejection and the fallback object hash. This
  reference is not assembled from the `.s`; its differences remain findings. Other rejections stay failures.
- Rust AArch64 binaries reuse rustc's printed link line and swap only the `cgu.0` object. The
  allocator-shim and `symbols.o` objects are rustc-built and shared by both arms.
- On macOS hosts x86 ELF is structure-only; this harness does not run Linux executables.
- `--report-only --recompare` reparses existing objects only after checking assembly, fixture, and
  object hashes. Missing historical hashes prevent recomparison; historical unchecked hashes/metadata
  are labeled explicitly. Recomparison does not rerun tools or establish runtime equivalence anew.
- Compiler version probes and cases run one at a time; `--lock-log PATH` sets the log of that
  serialization (default: `OUT/lock.jsonl`). `--one CASE.json` is the internal child entry point.
- Timing is optional legacy functionality; use `--no-timing` for compatibility-only runs. Increase
  `--chain-limit` explicitly when diagnostic stripping is useful; stripped output is never evidence.
