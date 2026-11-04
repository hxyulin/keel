//! Recognized compilation targets. Recognition does not imply an available
//! frontend, backend, assembler or linker: each consumer rejects the targets it
//! cannot serve. Native frontends use the target for ABI types and names.

/// A recognized machine/environment, including gated compilation targets.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Target {
    /// arm64 macOS: Apple's variant of AAPCS64, Mach-O.
    Aarch64AppleDarwin,
    /// x86-64 Linux: the System V psABI, ELF.
    X86_64LinuxGnu,
    /// AArch64 Linux: base AAPCS64, ELF.
    Aarch64LinuxGnu,
    /// Freestanding wasm32, compiled by `keel-wasm` rather than the native backends.
    Wasm32UnknownUnknown,
}

impl Target {
    /// All recognized targets, including targets with unavailable public pipelines.
    pub const ALL: [Target; 4] = [
        Target::Aarch64AppleDarwin,
        Target::X86_64LinuxGnu,
        Target::Aarch64LinuxGnu,
        Target::Wasm32UnknownUnknown,
    ];

    /// Targets supported by the native code generators and native C frontend.
    pub const NATIVE: [Target; 3] = [
        Target::Aarch64AppleDarwin,
        Target::X86_64LinuxGnu,
        Target::Aarch64LinuxGnu,
    ];

    /// Accepted alternative spellings, as GNU toolchains name the Linux targets.
    pub const ALIASES: [(&'static str, Target); 2] = [
        ("x86_64-linux-gnu", Target::X86_64LinuxGnu),
        ("aarch64-linux-gnu", Target::Aarch64LinuxGnu),
    ];

    /// The machine this program runs on, if keel supports it.
    pub fn host() -> Option<Target> {
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            Some(Target::Aarch64AppleDarwin)
        } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            Some(Target::X86_64LinuxGnu)
        } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
            Some(Target::Aarch64LinuxGnu)
        } else {
            None
        }
    }

    /// The target triple, as clang and rustc spell it.
    pub fn triple(self) -> &'static str {
        match self {
            Target::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
            Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
            Target::Wasm32UnknownUnknown => "wasm32-unknown-unknown",
        }
    }

    /// Parse a canonical triple or one of [`Self::ALIASES`].
    pub fn from_triple(triple: &str) -> Option<Target> {
        Target::ALIASES
            .into_iter()
            .find_map(|(alias, target)| (alias == triple).then_some(target))
            .or_else(|| Target::ALL.into_iter().find(|t| t.triple() == triple))
    }
}

/// The host, or Apple arm64 where keel does not run natively.
impl Default for Target {
    fn default() -> Self {
        Target::host().unwrap_or(Target::Aarch64AppleDarwin)
    }
}
