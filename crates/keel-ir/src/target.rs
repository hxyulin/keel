//! The machines keel generates code for. The IR itself is the same for
//! every target; frontends need the target for the C types the ABI fixes
//! (`va_list`, symbol names), and code generation picks a backend by it.

/// A machine and operating system to generate code for.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Target {
    /// arm64 macOS: Apple's variant of AAPCS64, Mach-O.
    Aarch64AppleDarwin,
    /// x86-64 Linux: the System V psABI, ELF.
    X86_64LinuxGnu,
}

impl Target {
    pub const ALL: [Target; 2] = [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu];

    /// The machine this program runs on, if keel supports it.
    pub fn host() -> Option<Target> {
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            Some(Target::Aarch64AppleDarwin)
        } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            Some(Target::X86_64LinuxGnu)
        } else {
            None
        }
    }

    /// The target triple, as clang and rustc spell it.
    pub fn triple(self) -> &'static str {
        match self {
            Target::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
        }
    }

    pub fn from_triple(triple: &str) -> Option<Target> {
        Target::ALL.into_iter().find(|t| t.triple() == triple)
    }
}

/// The host, or Apple arm64 where keel does not run natively.
impl Default for Target {
    fn default() -> Self {
        Target::host().unwrap_or(Target::Aarch64AppleDarwin)
    }
}
