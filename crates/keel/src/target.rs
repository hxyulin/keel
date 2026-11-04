//! `--target` parsing and the default target, shared by every subcommand.
use keel_codegen::Target;

/// Parse a `--target` value: a canonical triple or one of [`Target::ALIASES`].
pub(crate) fn parse_target(triple: &str) -> Result<Target, String> {
    Target::from_triple(triple).ok_or_else(|| {
        let known: Vec<_> = Target::ALL.iter().map(|t| t.triple()).collect();
        format!("unknown target `{triple}`; known: {}", known.join(", "))
    })
}

/// The explicit target, or else the machine keel runs on.
pub(crate) fn resolve_target(explicit: Option<Target>) -> Result<Target, String> {
    explicit
        .or_else(Target::host)
        .ok_or_else(|| "keel does not run on this machine's target; pass `--target`".to_string())
}
