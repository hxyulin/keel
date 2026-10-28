//! Per-invocation address and unwind metadata policies.
use crate::{CodegenError, CodegenPipeline, Target, aarch64, x86_64};
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

/// Emission policies independent of optimization, selection and allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EmissionOptions {
    /// Compiler-generated unwind metadata. Defaults to all functions.
    pub unwind_tables: UnwindTables,
    /// Explicit x86-64 address model; other targets reject an explicit value.
    pub x86_code_model: Option<x86_64::CodeModel>,
}

pub(crate) enum Output {
    Text(String),
    Program(keel_asm_ir::Program),
}

pub(crate) fn compile(
    module: &Module,
    target: Target,
    pipeline: &CodegenPipeline<'_>,
    options: EmissionOptions,
    structured: bool,
    sink: Option<&mut dyn FnMut(keel_asm_ir::Program)>,
) -> Result<Output, CodegenError> {
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
                &x86_64::REGISTERS,
                pipeline,
                options,
                structured,
                sink,
            )? {
                x86_64::Output::Text(text) => Ok(Output::Text(text.into_string())),
                x86_64::Output::Structured(program) => Ok(Output::Program(program)),
            }
        }
        Target::Aarch64AppleDarwin | Target::Aarch64LinuxGnu => {
            match aarch64::compile_output(
                module,
                &aarch64::REGISTERS,
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
