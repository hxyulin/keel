# C++ and Cargo workload investigation

The current `keel-suite` compiler enum accepts Clang C and Keel C. Its shared
preprocessing and source build model cannot represent a C++ or Cargo build.
Do not put these inputs into the existing C performance aggregate.

## First milestone: downstream toolchain workloads

Keep Clang++/rustc as the frontend, and measure Keel's assembler and linker
independently. This tests real compiler output without attributing LLVM's
machine-code optimizations to Keel. Changing an assembler normally should not
improve application runtime; runtime is primarily a correctness gate here.

Start with the existing C++ templates, virtual dispatch, static initialization,
atomics, vector loops, exceptions and TLS fixtures in
`compiler_asm_compat/fixtures/cpp`. The existing harness already records
unmodified assembly, section/symbol/relocation comparisons and native runtime
oracles. Header-light fixtures cover language features, but are not a substitute
for a substantial STL application.

`workload-pilots/cargo-smoke/Cargo.toml` wraps the existing std/generics fixture
as a standalone, dependency-free Cargo package. It exercises Vec, BTreeMap,
String, generics and trait objects. It is a plumbing pilot, not a representative
Cargo application. Build it offline with a separate output directory:

```sh
cargo rustc --offline --locked \
  --manifest-path suite/workload-pilots/cargo-smoke/Cargo.toml \
  --target-dir target/cargo-smoke --release -- \
  -C codegen-units=1 -C save-temps --emit=asm,obj,link --print link-args
```

For the application-object replay and runtime/structure checks on Apple ARM:

```sh
python3 suite/cargo_asm_probe.py --keel /path/to/release/keel \
  --out target/cargo-smoke-probe
```

Use a fresh output directory. This pilot deliberately uses the system linker
and keeps dependency archives unchanged. The Cargo, reference-reassembled and
Keel-reassembled executables agree; code and data match modulo benign metadata,
but `__eh_frame` bytes and relocations differ, so runtime agreement does not
establish unwind equivalence.

A C++/Rust exception probe at O0/O2, with and without DWARF5, checked
destructor/Drop order, catches and C++ rethrows (including forced DWARF fallback
with unchanged instruction bytes) through Keel's assembler and the Apple ARM
system linker; a negative control without unwind coverage aborts as expected.
This does not establish general metadata equivalence or Keel-linker coverage.

The C++ template smoke cases also agree at runtime at O0/O2 with and without
DWARF5, but debug objects are not structurally identical. Both the reference
object and Keel's object fail the O2 `dwarfdump` name-index check; both O0
debug objects verify. These remain unresolved metadata results.

For a Cargo adapter, preserve Cargo's JSON artifact messages and rustc's actual
link arguments. Keep allocator shims, system libraries, startup objects,
frameworks and Rust runtime archives in the same order. Reassemble the actual
saved CGU assembly, replace only that object, relink, and compare exit status,
stdout and stderr. A final-binary-only replacement leaves dependency rlibs
untouched: report that coverage explicitly. A linker driver wrapper can capture
the real final link for replay; it must distinguish executable, dylib and
build-script links and preserve response-file contents.

The next larger pilots should be a checked-in multi-crate workspace with a
path dependency, then a pinned application with vendored/locked dependencies,
build.rs and proc-macro coverage. For C++, add a multi-TU STL executable with
templates, vtables, exception unwinding and static constructors. Start with a
small deterministic workload before choosing a large external project.

## Measurement contract

- Separate frontend, assembly, final link and clean end-to-end build times.
- Reuse byte-identical assembly/object inputs for tool replacement comparisons;
  record hashes, tool versions, target, CPU features and exact commands.
- Keep debug/release, panic abort/unwind, LTO, CGU count, incremental and linker
  dead stripping explicit. Use no LTO first; embedded bitcode needs plugin work.
- Distinguish an empty-target clean build, a dependency-cached build, a no-op
  build and a controlled one-source-file rebuild. Do not average them together.
- Require executable output, constructors/destructors, panic/exception and
  debugger/unwind gates before timing accepted configurations. Preserve failures
  as unsupported coverage; do not silently strip directives or drop objects.
- Cover native AArch64/Mach-O and x86-64/ELF separately. Cross-assembly alone is
  not an execution or system-runtime linking result.

## Runtime optimization comparisons require a different milestone

`suite/llvm_import.py` now covers scalar CFGs, indirect calls, scalar floating
point, globals, bounded aggregate-return transport, exported ODR definitions,
Itanium RTTI and exception paths on Apple AArch64. All fourteen required C++
profiles pass in the integrated frontend harness, including virtual dispatch
and user-defined catches. It remains an LLVM importer, not a C++/Rust frontend
or a general C++ application compatibility claim.

The pinned [pugixml 1.15 pilot](workload-pilots/pugixml/README.md) exercises a
complete unmodified XML/XPath implementation and a separate driver. Both
Clang references pass; untouched Clang O1 IR is currently rejected at a
two-word aggregate return. Keel None/Fast/Full are not reached, and the
workload is not included in performance aggregates.
Fixing that blocker may reveal additional unsupported features. Keep imported
IR production, import cost and Keel compilation separate in future timings.

The native-C [yyjson](yyjson/README.md) and [LZ4](lz4/README.md) workloads
provide additional runtime coverage without requiring the importer.
