//! Give operand-free interrupt controls their existing IR memory contract.
use crate::{ModuleContext, OptimizationContext, Pass, VectorTarget};
use keel_ir::{AsmClobber, Function, InstData, Intrinsic, Module};

pub(crate) struct X86InterruptAsm;

impl Pass for X86InterruptAsm {
    fn name(&self) -> &'static str {
        "x86-interrupt-asm"
    }

    fn description(&self) -> &'static str {
        "recognize operand-free x86 CLI and STI compiler barriers"
    }

    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }

    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        if context.target != VectorTarget::X86Sse2 {
            return Some(false);
        }
        let mut candidates = Vec::new();
        // GNU macros share state across assembly fragments, including those
        // in functions outside this pass's rewrite scope. An instruction
        // mnemonic in a raw template can be the name of a macro.
        if module
            .assembly
            .iter()
            .any(|source| has_macro_source(source))
        {
            return Some(false);
        }
        for (index, f) in module.functions.iter().enumerate() {
            for (_, inst) in f.layout_insts() {
                let InstData::InlineAsm { asm, .. } = f.inst_data(inst) else {
                    continue;
                };
                if has_macro_source(&asm.template) {
                    return Some(false);
                }
                if !asm.inputs.is_empty()
                    || !asm.outputs.is_empty()
                    || asm.clobbers.iter().any(|c| *c != AsmClobber::Memory)
                {
                    continue;
                }
                let id = match asm.template.trim() {
                    "cli" => Intrinsic::X86Cli,
                    "sti" => Intrinsic::X86Sti,
                    _ => continue,
                };
                if context.enabled(f) {
                    candidates.push((index, inst, id));
                }
            }
        }
        if candidates.is_empty() {
            return Some(false);
        }
        context.changed.resize(module.functions.len(), false);
        for (index, inst, id) in candidates {
            *module.functions[index].inst_data_mut(inst) = InstData::Intrinsic {
                id,
                args: Default::default(),
            };
            context.changed[index] = true;
        }
        Some(true)
    }
}

fn has_macro_source(source: &str) -> bool {
    [b".macro".as_slice(), b".include".as_slice()]
        .into_iter()
        .any(|directive| {
            source
                .as_bytes()
                .windows(directive.len())
                .any(|window| window.eq_ignore_ascii_case(directive))
        })
}
