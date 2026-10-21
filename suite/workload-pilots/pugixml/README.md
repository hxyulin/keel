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

Run on Apple AArch64 with LLVM 23 and a privately built Keel binary. All Cargo,
compiler and native execution must be inside the shared lock; use `-j2` for
Cargo and a private target directory. The validator invokes commands serially,
uses no timing loops, and must not be wrapped recursively.

```sh
work=$(mktemp -d /private/tmp/keel-pugixml.XXXXXX)
python3 suite/fast_opt_locked.py "$work/lock.jsonl" \
  cargo build --offline --locked -j2 -p keel --target-dir "$work/target"
python3 suite/fast_opt_locked.py "$work/lock.jsonl" \
  python3 suite/workload-pilots/pugixml/validate.py \
    --libllvm ~/.rustup/toolchains/1.99.0-aarch64-apple-darwin/lib/libLLVM.dylib \
    --keel "$work/target/debug/keel" --out "$work/run"
```

`--archive` can name a predownloaded copy of the pinned official archive.
`--clangxx` selects the compiler. `--out` must be new. The script never builds
Keel. It emits `report.json` with exact argv/cwd, status, source/tool/IR hashes,
versions, diagnostics and stdout/stderr paths, plus a readable `report.html`.
The lock log separately records execution metadata; its wall time is not a
performance result. Artifact directories should live outside disposable
worktrees and be retained before cleanup.

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

Initial local result (2026-10-21, base `0f6fdd7fffbcbe31dde6499276f780634d7e6a05`):
Apple Clang 21.0.0 (`clang-2100.3.34.2`), LLVM 23.1.1, emitted target
`arm64-apple-macosx27.0.0`. Both references passed, each printing 65 lines:

```text
round=0 nodes=32 ids=496 selected=11 prices=176
...
round=63 nodes=32 ids=65008 selected=11 prices=22352
documents=64 nodes=2048 total=2817024
```

Full stdout SHA-256:
`d368fb19a3aac820cd6757bbe79afa0e7ec70d177a2f9d985b7a624beeb5226f`.
The pugixml module failed signature translation with:

```text
llvm-import: supported types are scalar i8/i16/i32/i64/f32/f64 and address-space-zero pointers; i1 is internal only
```

The first `i1`-returning definition in the unchanged module is
`_ZN4pugi15xml_tree_walker5beginERNS_8xml_nodeE` (`xml_tree_walker::begin`).
`reduced_bool.cpp` is a separately authored, small boolean-returning source
reproducer, not a patched upstream file. Its untouched O1 IR produces the same
rejection; its native reference exits 0. That followup runs only for this
specific type diagnostic and does not bypass the full-module blocker.
Driver import, Keel verification, object emission, linking and None/Fast/Full
execution were not reached. Additional application gaps are unknown.

Initial artifacts are retained outside the worktree at
`/private/tmp/keel-pugixml-pilot-79b6b693/`; `run-3/report.json` and
`run-3/report.html` describe the final run. `release.json` and `tag.json` retain
the official API evidence, `lock.jsonl` records the private build and runs,
and `evidence.tar.gz` preserves the final run plus provenance. The private
`target/` also remains there for the integrating agent.

Follow-up (2026-10-22): explicit zero-extended booleans, bounded two-word
aggregate transport and nonvolatile memory intrinsics now pass the earlier
signature gates. Both untouched source and LLVM references still match the
independent oracle. The full module next rejects a private `fastcc` helper
(`strcpy_insitu`); no Keel tier executes yet. Supporting that convention needs
a closed direct-call proof and consistent rewriting of definitions and calls,
not an assumption that fastcc is the platform C ABI. Evidence is retained at
`target/llvm-memory-20261022/pugixml/report.json`.

The subsequent private-fastcc implementation clears that convention gate by
translating proven contained definitions and calls to Keel `cc(fast)`. The next
untouched-module blocker is the two-word aggregate return of `xml_node::begin()`;
aggregate parameters are supported, but aggregate results are not yet imported.
Both native references still pass. No Keel application tier executes.
See `docs/fast-call-convention-20261022.html` and the final pilot evidence at
`target/fastcc-20261022/pugixml-final/report.json`.
