//! Per-invocation code layout, address and unwind metadata policies.
use crate::{CodegenError, Target, aarch64, x86_64};
use keel_ir::Module;

/// Whether ordinary functions receive compiler-generated call-frame information.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnwindTables {
    /// Preserve the existing call-frame information for every function.
    #[default]
    All,
    /// Emit only tables required by a function's exception-handling personality.
    /// Explicit CFI in user assembly is unaffected.
    RequiredOnly,
}

/// Preference for target-specific code layout and instruction sequences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodegenGoal {
    /// Preserve the existing performance-oriented layout and frame decisions.
    #[default]
    Speed,
    /// Prefer smaller code, including removal of optional x86 alignment padding.
    Size,
}

/// Emission policies independent of optimization, selection and allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmissionOptions {
    /// Target code-size policy, independent of the IR pass pipeline.
    pub goal: CodegenGoal,
    /// Compiler-generated unwind metadata. Defaults to all functions.
    pub unwind_tables: UnwindTables,
    /// Explicit x86-64 address model; other targets reject an explicit value.
    pub x86_code_model: Option<x86_64::CodeModel>,
    /// On ELF, put each function in its own section, `.text.<name>`, so a
    /// linker's section garbage collection removes functions one by one.
    /// Mach-O linkers split sections at symbols already, so Mach-O ignores
    /// this.
    pub function_sections: bool,
    /// On ELF, put each data object in its own section, `.data.<name>` and
    /// so on. Thread-local objects keep sharing `.tdata` and `.tbss`.
    pub data_sections: bool,
    /// Give every IR stack slot its own frame bytes, even when lifetime
    /// markers show that two slots are never live at once. For bisecting a
    /// producer's markers; sharing is on by default.
    pub separate_slots: bool,
}

/// The ELF section for function `name`: its `section` attribute, the retain
/// section, or `.text`, `.text.unlikely` or `.text.hot`, followed by
/// `.<name>` when `own` asks for a section per function.
pub(crate) fn elf_text_section(attrs: &keel_ir::FunctionAttrs, name: &str, own: bool) -> String {
    if let Some(section) = attrs.section {
        return section.as_str().to_owned();
    }
    if attrs.retain {
        return format!(".text.keel.retain.{name}");
    }
    let base = if attrs.cold {
        ".text.unlikely"
    } else if attrs.hot {
        ".text.hot"
    } else {
        ".text"
    };
    if own {
        format!("{base}.{name}")
    } else {
        base.into()
    }
}

pub(crate) enum Output {
    Text(String),
    Program(keel_asm_ir::Program),
}

pub(crate) fn compile(
    module: &Module,
    request: &crate::Request<'_>,
    structured: bool,
    sink: Option<&mut dyn FnMut(keel_asm_ir::Program)>,
) -> Result<Output, CodegenError> {
    let crate::Request {
        target,
        ref pipeline,
        emission: options,
        pools,
    } = *request;
    crate::require_native_target(target).map_err(|message| CodegenError {
        func: "<target>".into(),
        message: message.into(),
    })?;
    if options.x86_code_model.is_some() && target != Target::X86_64LinuxGnu {
        return Err(CodegenError {
            func: "<target>".into(),
            message: "x86 code model requires x86_64-unknown-linux-gnu".into(),
        });
    }
    match target {
        Target::X86_64LinuxGnu => {
            match x86_64::compile_output(
                module,
                pools.unwrap_or(&x86_64::REGISTERS),
                pipeline,
                options,
                structured,
                sink,
            )? {
                x86_64::Output::Text(text, _) => Ok(Output::Text(text.into_string())),
                x86_64::Output::Structured(program) => Ok(Output::Program(program)),
            }
        }
        Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => {
            match aarch64::compile_output(
                module,
                pools.unwrap_or(&aarch64::REGISTERS),
                pipeline,
                structured,
                sink,
                target == Target::Aarch64LinuxGnu,
                options,
            )? {
                aarch64::Output::Text(text) | aarch64::Output::ElfText(text) => {
                    Ok(Output::Text(text.into_string()))
                }
                aarch64::Output::Structured(program) => Ok(Output::Program(program)),
            }
        }
        Target::Wasm32UnknownUnknown => unreachable!("validated native target"),
    }
}

/// Retain target capabilities while overriding optional selection per function.
pub(crate) struct FunctionSelector<'a>(pub &'a dyn crate::InstructionSelector);
impl FunctionSelector<'_> {
    fn options(function: &keel_ir::Function) -> Option<crate::SelectionOptions> {
        use keel_ir::FunctionOptLevel;
        let policy = keel_ir::FunctionPolicy::resolve(&function.attrs).ok()?;
        Some(match policy.level? {
            FunctionOptLevel::None => crate::SelectionTier::Covers.options(),
            FunctionOptLevel::Fast => crate::SelectionTier::Trees.options(),
            _ => crate::SelectionOptions::ALL,
        })
    }
}
impl crate::InstructionSelector for FunctionSelector<'_> {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn x86_features(&self) -> keel_asm_ir::x86::Features {
        self.0.x86_features()
    }
    fn validate_target(&self, target: Target) -> Result<(), &'static str> {
        self.0.validate_target(target)
    }
    #[cfg(feature = "experimental-dispatch")]
    fn experimental_aarch64_dispatch_tails(&self) -> bool {
        self.0.experimental_aarch64_dispatch_tails()
    }
    fn select(
        &self,
        function: &keel_ir::Function,
        target: crate::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> crate::select::Selection {
        match Self::options(function) {
            Some(options) => crate::OptionsSelector(options).select(function, target, folds),
            None => self.0.select(function, target, folds),
        }
    }
    fn select_with_uses(
        &self,
        function: &keel_ir::Function,
        target: crate::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        uses: &[u8],
    ) -> crate::select::Selection {
        match Self::options(function) {
            Some(options) => {
                crate::OptionsSelector(options).select_with_uses(function, target, folds, uses)
            }
            None => self.0.select_with_uses(function, target, folds, uses),
        }
    }
}

pub(crate) fn function_goal(attrs: &keel_ir::FunctionAttrs, default: CodegenGoal) -> CodegenGoal {
    use keel_ir::FunctionOptLevel;
    match keel_ir::FunctionPolicy::resolve(attrs)
        .ok()
        .and_then(|p| p.level)
    {
        Some(FunctionOptLevel::Size) => CodegenGoal::Size,
        Some(_) => CodegenGoal::Speed,
        None if attrs.cold => CodegenGoal::Size,
        None => default,
    }
}
