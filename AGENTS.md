# Commits

- Use Conventional Commits: `type(scope): description`, with an optional scope.
- Choose the type that describes the change, such as `feat`, `fix`, `perf`,
  `refactor`, `test`, `docs`, `build`, `ci`, or `chore`.
- Write a short, imperative description. Mark breaking changes with `!` and
  explain them in a `BREAKING CHANGE:` footer.

# Validation scope

- During development, run the changed test targets and the relevant component's
  unit/integration tests. Prefer `tools/test.py <component> --quick`, `--unit`
  or `--test <target>` over a workspace-wide command.
- Include focused consumer tests when shared IR, ABI, instruction metadata,
  linker contracts or shared fixtures change. If automatic change selection
  expands a small change to the entire workspace, choose explicit relevant
  components and explain the coverage instead of blindly running everything.
- Reserve full workspace verification for merge/fast-forward boundaries,
  broad shared-contract changes that cannot be checked narrowly, or an explicit
  request for qualification or timing. Run it once at that boundary; do not
  repeat it after unrelated edits or a successful targeted check.
- Report the scope actually run, failures and skips. A component pass does not
  establish that the entire workspace is green.

# Documentation and suites

- Maintain current references, not a diary of implementation progress. Update the
  owning guide or crate README instead of adding a document for each feature,
  checkpoint, investigation, validation run or handoff.
- Put API contracts beside the API when practical. Keep shared architecture,
  user-facing behavior and limitations in the maintained references indexed by
  `docs/README.md`. Put target-specific contracts under `docs/targets/<target>/`
  and shared material in the owning compiler, frontend or linker section. Update
  the section navigation instead of adding flat feature pages. Consolidate
  overlapping material rather than duplicating it.
- Do not commit progress ledgers, per-run audit JSON, logs, object dumps, timing
  samples or report renderers. Store temporary experiments and their outputs in
  ignored `target/` or `suite/reports/`; use Git history for historical evidence.
- Prefer focused regressions in existing test targets and workloads in existing
  suite runners. Add a standalone harness only for a reusable test boundary the
  existing infrastructure cannot exercise. Place it under the owning `suite/c`,
  `suite/ir`, `suite/asm`, `suite/linker` or `suite/integration` group. Use
  `cargo suite check <group>` for runner tests. Do not add suites to track progress.
- Keep fixtures, source pins, policies, generator inputs and reviewed data needed
  to reproduce public benchmark claims. These are maintained inputs, not run
  artifacts. Remove temporary scripts after incorporating their regression.

- After a feature stabilizes, keep regressions for distinct failure modes rather
  than every development experiment. Prefer boundary cases over dense sweeps
  when interior values follow the same lowering. Keep independent oracles,
  negative controls and genuinely different ABI/allocation paths.
- If two output routes are asserted byte-identical, execute one image rather
  than linking and running both. Reuse immutable fixture preparation across
  matrix configurations. Do not reduce tests just to lower a test count.
