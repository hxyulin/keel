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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each subcommand's target for `flags`, resolved by the parser it ships with.
    fn subcommand_targets(flags: &[&str]) -> Vec<(&'static str, Result<Target, String>)> {
        let args = |extra: &[&str]| -> Vec<String> {
            flags.iter().chain(extra).map(|s| s.to_string()).collect()
        };
        let options = |allowed| crate::parse_options(&args(&["input"]), allowed);
        vec![
            (
                "compile",
                options(crate::COMPILE_FLAGS).and_then(|o| o.target()),
            ),
            ("cc", options(crate::CC_FLAGS).and_then(|o| o.target())),
            ("opt", options(crate::OPT_FLAGS).and_then(|o| o.target())),
            (
                "asm-metrics",
                options(crate::ASM_METRICS_FLAGS).and_then(|o| resolve_target(o.target)),
            ),
            (
                "asm",
                crate::asm::Options::parse(&args(&["in.s", "-o", "out.o"])).map(|o| o.target),
            ),
            (
                "link",
                crate::link::split_target(&args(&["in.o", "-o", "out"])).map(|(t, _)| t),
            ),
            (
                "archive",
                crate::archive::parse(&args(&["-o", "out.a", "in.o"])).map(|(t, ..)| t),
            ),
        ]
    }

    #[test]
    fn every_subcommand_defaults_to_the_host_and_accepts_the_same_spellings() {
        for (command, target) in subcommand_targets(&[]) {
            match Target::host() {
                Some(host) => assert_eq!(target, Ok(host), "{command}"),
                None => assert!(target.is_err(), "{command}"),
            }
        }
        let spellings = Target::NATIVE
            .map(|target| (target.triple(), target))
            .into_iter()
            .chain(Target::ALIASES);
        for (spelling, expected) in spellings {
            for (command, target) in subcommand_targets(&["--target", spelling]) {
                assert_eq!(target, Ok(expected), "{command} --target {spelling}");
            }
        }
        for (command, target) in subcommand_targets(&["--target", "x86_64"]) {
            let error = target.unwrap_err();
            assert!(
                error.contains("unknown target `x86_64`"),
                "{command}: {error}"
            );
        }
    }
}
