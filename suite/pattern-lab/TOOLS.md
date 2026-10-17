# Optional LLVM adapters

`tools.py` is a standalone Python 3 stdlib CLI. The harness invokes it as a
subprocess; there is no import dependency or compiler cost-model integration.
Run from any directory, with explicit input/output paths:

```sh
python3 suite/pattern-lab/tools.py discover --output /existing/reports/discovery.json
python3 suite/pattern-lab/tools.py mca --assembly suite/pattern-lab/mca/example.s \
  --cpu cortex-a76 --triple aarch64-unknown-linux-gnu --iterations 100 \
  --output /existing/reports/model.json
```

Each output must be a **new JSON file in an existing directory**. Exclusive
creation rejects existing files and symlinks; there is no overwrite flag. Keep
generated reports under ignored `suite/reports/` or `target/`. Invalid CLI
arguments exit 2 without creating a report. A filesystem input error creates a
report with `status: error` and exits 1. Output creation/write errors exit 2.

Discovery searches the unversioned tool name on PATH, followed by only these
explicit directories: `/opt/homebrew/opt/llvm/bin`, `/usr/local/opt/llvm/bin`,
`/usr/lib/llvm-19/bin`, `/usr/lib/llvm-18/bin`, `/usr/lib/llvm-17/bin`.
There is no directory scan, installation, or package-manager invocation.
`--llvm-mca /absolute/path` overrides discovery; a missing explicit path never
falls back. Discovery also accepts `--llvm-exegesis /absolute/path`.

Discovery runs only `--version` and `--help`, with a five second timeout each,
stdin disconnected, shell disabled, and captured stdout/stderr. It does not
execute an instruction benchmark. Tool records contain resolved executable
paths, SHA-256, exact probe argv/output/return codes and `available`, `missing`,
or `version_failure` status. Failed help probes remain visible. Registered
targets are extracted only when printed in version/help text; otherwise target
capability is `unknown`. A target listing does not establish model fidelity.
Native host OS/release/machine/processor are recorded independently.

MCA requires an explicit CPU name. `--triple` and `--arch` pass through to LLVM;
when omitted LLVM uses its own defaults, recorded as null in the model fields.
The adapter never guesses an MacHost/Apple CPU model. All selections are
user-declared and **unverified**, including an accepted `apple-m5` name.
Acceptance does not establish Apple M5 accuracy, PMU availability, hardware
performance, or measured profit. There is no calibration or latency summation.

MCA uses `-mcpu=`, `-iterations=`, optional `-mtriple=`/`-march=`, and the resolved
assembly filename as separate argv entries. Model/target names are restricted
to 128 simple name characters, iteration count to 1..100000 (default 100),
assembly to a regular file up to 4 MiB, and `--timeout` to 0.05..60 seconds
(default 30). A symlink to a regular input resolves to its target; directories,
devices, and FIFOs are rejected before being read. Inputs and executables are
hashed before execution; callers must keep them unchanged during execution.

Reports retain the source hash, binary hash, declared CPU/triple/arch,
uncertainty, exact argv, raw stdout/stderr, return code, timeout, and errors.
Missing tools or failed version probes yield `unsupported`, an empty metrics
object, and exit 1. Nonzero exit, timeout, any stderr diagnostic, recognized
stdout diagnostic, or an absent recognizable cycle summary yields
`analysis_failed`, no metrics, and exit 1. This deliberately rejects even benign
stderr rather than silently accepting an ignored CPU or skipped instruction.
No fallback model is selected. A successful run yields `analyzed` and exit 0;
this is an analysis status, not a correctness PASS. Only a single unambiguous
`Total Cycles:` summary is parsed, with `modeled cycles` units. Multiple regions
retain raw output with empty metrics rather than combining costs. These model
results exclude hardware/cache/OS effects not represented by LLVM's model.

The first increment provides exegesis **discovery only**. There is no native
exegesis execution subcommand, automatic calibration, PMU probe, or PMU result.
Native execution would require a future explicitly opted-in, bounded interface.

Flag references: [official llvm-mca documentation](https://llvm.org/docs/CommandGuide/llvm-mca.html)
and [official llvm-exegesis documentation](https://llvm.org/docs/CommandGuide/llvm-exegesis.html).
Installed versions vary; the exact local version/help probes remain in reports.

Tests use scripted Python executables and CLI subprocesses, without LLVM,
Clang, Cargo, native kernels, or performance measurements:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest discover \
  -s suite/pattern-lab/tests -p test_tools.py -v
```

The default fixture directory is ignored `target/pattern-lab-tools-tests` in
the checkout. Set `PATTERN_LAB_TEST_ROOT` to another ignored report directory;
development validation uses the shared root's
`suite/reports/pattern-lab-development-20261017/adapters`. Fixtures are temporary
and removed after each test. Tests establish adapter behavior, not a real LLVM
analysis or model/hardware accuracy.
