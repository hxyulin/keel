# Darwin TLS alignment probe

This is a **manual diagnostic**, not a TLS alignment fix or a passing CI gate.
Run on native Apple arm64 Darwin with Apple Clang and an explicitly selected,
already built Keel binary. It does not build Keel or change the host:

```sh
python3 tools/tls-alignment/probe.py --keel /absolute/path/to/keel \
  --out /absolute/path/to/fresh-evidence-directory
# Exit 1 is expected when counterexamples are captured; inspect results.json.
```

This diagnostic exits **1** for any failed runtime check or link failure. It retains
all commands, outputs, sources, IR, assembly, objects, executables and hashes. It
requires a fresh destination and does not retry failed configurations. It is not
a passing CI test or a replacement for `aligned_objects_with_debug_info`.
Exit **0** means only that this run found no counterexample; it does not prove
over-aligned TLS is supported. Invalid arguments exit **2**; compilation failures,
unexpected runtime exits and timeouts stop the run with retained partial evidence.

Seven fixtures cover initialized and all-zero TLS32/64, mixed initialized/zero
TLS32/64/128 in two declaration orders, and an ordinary 4/8/16-byte control.
Ordinary char/short/long TLS coexist with the ints. Each fixture uses Clang,
Keel text assembled by Clang, and Keel structured objects; each runs with debug
information, with and without optimization. Clang's optimized control uses `-O2`;
Keel uses its `-O` pipeline. The separate unoptimized C driver observes actual
pointers and values in the main thread and two simultaneously live fresh
pthreads. It verifies repeated addresses, initialized values, mutation and
thread isolation, retaining failures rather than comparing only exit codes.
The ordinary TLS accesses in `tls_count` trigger real per-thread allocation
before the over-aligned pointers are measured.

The 2026-10-20 assessment on MACHOST, macOS 27.0.1 build 26A434, Apple Clang
21.0.0, current-main Keel `86c064365b46639265b154a37ca4d7f9d5cb9826`
(binary SHA-256 `20fafb1746e894daf590c8d7eb8b1235e340f4fd3c54203e451086c02b16e926`)
recorded **42 configurations, 36 alignment counterexamples, zero link failures**.
Each route had 12 counterexamples and two passing ordinary-alignment controls.
This reproduces a Darwin TLV allocation limitation affecting Clang as well as
both Keel routes; it is not evidence of a Keel-only defect. This is an
observation on that platform and layout, not a guarantee of failure under every
allocator or link layout. The smaller historical TLS32 fixture can pass fresh
links while its retained older binary fails.

The retained investigation found that requested alignment reaches Keel IR and
aligned Mach-O template symbols. Both compilers use the standard three-word
Darwin TLV descriptor and return
addresses through its getter. Installed libdyld's `instantiateVariable` calls
`malloc_type_malloc(size, type_id)` for initialized templates and
`malloc_type_calloc(size, 1, type_id)` for all-zero templates; its `_tlv_get_addr`
adds the descriptor offset to that allocation. These calls carry no requested
alignment. Increasing section alignment or changing the fixed descriptor offset
cannot ensure an over-aligned address for every allowed allocation base.

Apple's public source at commit
[fd8d0c4d52320ebf64db34f3cb280310d905c5ae](https://github.com/apple-oss-distributions/dyld/blob/fd8d0c4d52320ebf64db34f3cb280310d905c5ae/libdyld/ThreadLocalVariables.cpp)
(dyld-1378) illustrates the same allocation model. It is **not** the exact
installed dyld source: `/usr/lib/dyld` reports dyld-27062. The retained runtime
disassembly and stepped calls, with libdyld UUID
`62863BCA-DD60-3D32-A9DB-89099BA6F3D7`, support the conclusion for this host.

For each new run, `commands.json` records exact argv, working directories, exit
codes and stdout/stderr paths; `identity.json` hashes the selected Keel binary,
driver and probe. `results.json` retains observations and source/IR/object/binary
hashes for completed native configurations. Its historical
`alignment counterexamples` summary counts all runtime-check failures, so inspect the
per-object residues and values before attributing a failure to alignment.
Keep the selected binary unchanged for the entire run. Use a new output directory
for each invocation; there are no retries or relaxed expectations.

The assessment's local evidence is under
`target/branch-assessment-20261020/tls-current-main` in the assessment checkout;
that private directory is not shipped with this tool. The older runtime
investigation is under `suite/reports/tls-alignment-20261017` there.
No compiler, linker, ABI, DWARF or existing test expectation is changed here.
There is no allocator interposition, new TLS ABI or linker padding workaround.
