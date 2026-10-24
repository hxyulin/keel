# pugixml 1.15 application pilot

This is a correctness and importer-blocker pilot for upstream XML parsing,
traversal and XPath, not a benchmark success or native C++ frontend claim.
The pinned library is compiled as its own complete translation unit with
`clang++ -std=c++17 -O1 -S -emit-llvm`. The common LLVM input is **already
optimized by Clang**. Exceptions, RTTI, standard library use, unused exported
functions, attributes and directives retain their defaults. No upstream source
or LLVM IR is edited, no helpers are localized, and no linker dead stripping
is requested. The driver is a separate C++ translation unit.

`manifest.json` pins the official release archive (395,045 bytes), its SHA-256,
the v1.15 tag commit and all three required source hashes. The release was
announced January 10, 2025. GitHub's release API has no asset digest for this
release: the recorded checksum is computed from the downloaded official asset,
not an upstream-signed checksum. At initial validation, all three source files
were independently compared byte for byte against the official v1.15 raw tag.
The script rechecks the archive and source pins before any compilation.
The original archive, exact source files and license are retained per run.

Run on Apple AArch64 with LLVM 23 and a built Keel binary. The validator runs
commands serially and uses no timing loops.

```sh
cargo build -p keel
python3 suite/workload-pilots/pugixml/validate.py \
  --libllvm "$(rustc --print sysroot)/lib/libLLVM.dylib" \
  --keel target/debug/keel --out target/pugixml-1
```

`--archive` can name a predownloaded copy of the pinned official archive.
`--clangxx` selects the compiler. `--out` must be new. The script never builds
Keel. It emits `report.json` with exact argv/cwd, status, source/tool/IR hashes,
versions, diagnostics and stdout/stderr paths, plus a readable `report.html`.

The driver checks 64 generated documents with 32 nodes each. Its assertions
cover UTF-8/entity decoding, traversal, attributes, XPath node selection,
XPath numeric sum and malformed XML rejection. An independent Python oracle
checks every stdout line. Both a direct-source reference and a reference
compiled from the exact untouched `.ll` inputs must match it. Only then does
module-wide import begin. On acceptance of both modules, None/Fast/Full objects
replace both reference objects, link with Clang++, and must match full stdout.
A rejected import stops the workload immediately, retains the destination
sentinel and raw diagnostic, and returns **1**, with JSON status `blocked`.
Other errors return 1 with status `failed`; existing output directories are
refused. No rejection is counted as benchmark success. No Keel tier is run
with a native fallback library.

## Current status

Both references pass, each printing 65 lines:

```text
round=0 nodes=32 ids=496 selected=11 prices=176
...
round=63 nodes=32 ids=65008 selected=11 prices=22352
documents=64 nodes=2048 total=2817024
```

The untouched pugixml module is still rejected, so no Keel tier executes yet.
Earlier blockers (`i1` ABI returns such as `xml_tree_walker::begin`, two-word
aggregate parameters, memory intrinsics and the private `fastcc` helper
`strcpy_insitu`) are now handled. The current first blocker is the two-word
aggregate **return** of `xml_node::begin()`: aggregate parameters are
supported, but aggregate results are not yet imported. Fixing it may reveal
further gaps.

`reduced_bool.cpp` is a separately authored, small boolean-returning reproducer
for the original `i1` diagnostic, not a patched upstream file. It runs only
when that specific diagnostic appears and does not bypass the full-module
blocker.
