# Commits

- Use Conventional Commits: `type(scope): description`, with an optional scope.
- Choose the type that describes the change, such as `feat`, `fix`, `perf`,
  `refactor`, `test`, `docs`, `build`, `ci`, or `chore`.
- Write a short, imperative description. Mark breaking changes with `!` and
  explain them in a `BREAKING CHANGE:` footer.

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
  existing infrastructure cannot exercise. Do not add suites to track progress.
- Keep fixtures, source pins, policies, generator inputs and reviewed data needed
  to reproduce public benchmark claims. These are maintained inputs, not run
  artifacts. Remove temporary scripts after incorporating their regression.
