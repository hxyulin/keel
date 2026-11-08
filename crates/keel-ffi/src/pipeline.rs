//! Complete textual IR input and native pipeline selection.
use super::{
    COMPILE, INVALID, KeelBlock, KeelBuffer, KeelCodegenOptions, KeelFunction, KeelModule,
    KeelStatus, KeelSymbol, KeelValue, Result, VERIFY, input, invalid, output, owner, run,
};
use keel_ir::{EntityRef, Module, SourceFile, SourceLoc};

pub(super) fn target(raw: u32) -> Result<keel_ir::Target> {
    match raw {
        1 => Ok(keel_ir::Target::Aarch64AppleDarwin),
        2 => Ok(keel_ir::Target::Aarch64LinuxGnu),
        3 => Ok(keel_ir::Target::X86_64LinuxGnu),
        _ => Err(invalid("unknown native target")),
    }
}

fn utf8(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| invalid("input is not UTF-8"))
}

impl KeelModule {
    pub(super) fn sync_callee_attributes(&mut self, name: &str) {
        let attrs = self.ir.function_attrs(name).cloned().unwrap_or_default();
        for function in &mut self.ir.functions {
            let references = function
                .callees()
                .filter(|(_, decl)| decl.name == name)
                .map(|(reference, _)| reference)
                .collect::<Vec<_>>();
            for reference in references {
                function.set_callee_attrs(reference, attrs.clone());
            }
        }
    }

    pub(super) fn invalidate_ids(&mut self) {
        self.owner = owner();
        self.symbols.clear();
        self.signatures.clear();
        self.layouts.clear();
    }
}

fn emission(
    raw: KeelCodegenOptions,
) -> Result<(
    keel_codegen::SelectionTier,
    keel_codegen::regalloc::AllocationTier,
    keel_codegen::EmissionOptions,
)> {
    use keel_codegen::{SelectionTier as S, regalloc::AllocationTier as A};
    let selection = match raw.selection {
        0 => S::Covers,
        1 => S::Baseline,
        2 => S::Trees,
        3 => S::Full,
        _ => return Err(invalid("unknown selection tier")),
    };
    let allocation = match raw.allocation {
        0 => A::Full,
        1 => A::Fast,
        _ => return Err(invalid("unknown allocation tier")),
    };
    let mut emission = keel_codegen::EmissionOptions::default();
    emission.goal = match raw.goal {
        0 => keel_codegen::CodegenGoal::Speed,
        1 => keel_codegen::CodegenGoal::Size,
        _ => return Err(invalid("unknown codegen goal")),
    };
    emission.unwind_tables = match raw.unwind {
        0 => keel_codegen::UnwindTables::All,
        1 => keel_codegen::UnwindTables::RequiredOnly,
        _ => return Err(invalid("unknown unwind policy")),
    };
    emission.x86_code_model = match raw.x86_code_model {
        0 => None,
        1 => Some(keel_codegen::x86_64::CodeModel::PositionIndependent),
        2 => Some(keel_codegen::x86_64::CodeModel::Small),
        3 => Some(keel_codegen::x86_64::CodeModel::Kernel),
        _ => return Err(invalid("unknown x86 code model")),
    };
    if raw.flags & !7 != 0 {
        return Err(invalid("unknown emission flags"));
    }
    emission.function_sections = raw.flags & 1 != 0;
    emission.data_sections = raw.flags & 2 != 0;
    emission.separate_slots = raw.flags & 4 != 0;
    Ok((selection, allocation, emission))
}

pub(super) fn compile(
    module: &Module,
    target: keel_ir::Target,
    options: KeelCodegenOptions,
    text: bool,
) -> Result<Vec<u8>> {
    let (selection, allocation, options) = emission(options)?;
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &selection,
        allocator: &allocation,
    };
    let request = keel_codegen::Request::new(target, pipeline).emission(options);
    if text {
        return request
            .text(module)
            .map(String::into_bytes)
            .map_err(|e| (COMPILE, e.to_string()));
    }
    let program = request
        .program(module)
        .map_err(|e| (COMPILE, e.to_string()))?;
    let features = keel_codegen::x86_features::module_features(module, Default::default())
        .map_err(|e| (COMPILE, e.to_string()))?;
    let mut assembler =
        keel_asm::ObjectAssembler::with_features(target, Default::default(), features)
            .map_err(|e| (COMPILE, e.to_string()))?;
    assembler.push(program);
    assembler.finish().map_err(|e| (COMPILE, e.to_string()))
}

fn verify(ir: &Module) -> Result<()> {
    keel_ir::verify_module(ir).map_err(|errors| {
        (
            VERIFY,
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })
}

api! {
    /// Transactionally replaces a module from complete Keel IR, including all parser-supported instructions and metadata.
    fn keel_module_load_ir(module: *mut KeelModule, text: *const u8, length: usize) {
        // SAFETY: Public pointer contract covers the borrowed text.
        unsafe { run(module, |m| {
            let ir = keel_ir::parse(utf8(input(text, length)?)?).map_err(|e| (INVALID, e.to_string()))?;
            verify(&ir)?;
            m.ir = ir; m.invalidate_ids(); Ok(())
        }) }
    }
}
api! {
    /// Transactionally merges parsed IR using the core module merger's contracts.
    fn keel_module_merge_ir(module: *mut KeelModule, text: *const u8, length: usize) {
        // SAFETY: Public pointer contract covers the borrowed text.
        unsafe { run(module, |m| {
            m.verify()?;
            let ir = keel_ir::parse(utf8(input(text, length)?)?).map_err(|e| (INVALID, e.to_string()))?;
            verify(&ir)?;
            // Preserve the existing module if duplicate names or debug info make merging fail.
            let merged = keel_ir::merge_modules(vec![m.ir.clone(), ir]).map_err(|e| (INVALID, e))?;
            verify(&merged)?;
            m.ir = merged; m.invalidate_ids(); Ok(())
        }) }
    }
}
api! {
    /// Finds a function definition by its exact module symbol name.
    fn keel_module_find_function(module: *mut KeelModule, name: *const u8, length: usize, out: *mut KeelFunction) {
        // SAFETY: Public pointer contract covers name and output.
        unsafe { output(module, out, |m| {
            let name = utf8(input(name, length)?)?;
            let index = m.ir.functions.iter().position(|f| f.name == name).ok_or_else(|| invalid("function definition not found"))?;
            Ok(KeelFunction { owner: m.owner, index: index as u32 })
        }) }
    }
}
api! {
    /// Finds a function/data definition or declaration, including after optimization.
    fn keel_module_find_symbol(module: *mut KeelModule, name: *const u8, length: usize, out: *mut KeelSymbol) {
        // SAFETY: Public pointer contract covers name and output.
        unsafe { output(module, out, |m| {
            let name = utf8(input(name, length)?)?;
            let sig = m.ir.signature(name).cloned();
            if sig.is_none() && !m.ir.data.iter().any(|d| d.name == name) { return Err(invalid("symbol not found")); }
            m.intern_symbol(name, sig)
        }) }
    }
}
api! {
    /// Returns a function block by layout ordinal, rebuilding valid IDs after rewriting.
    fn keel_function_block_at(module: *mut KeelModule, function: KeelFunction, ordinal: usize, out: *mut KeelBlock) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let f = &m.ir.functions[m.function_index(function)?];
            let b = f.blocks().nth(ordinal).ok_or_else(|| invalid("block ordinal out of range"))?;
            Ok(KeelBlock { owner: m.owner, function: function.index, index: b.index() as u32 })
        }) }
    }
}
api! {
    /// Returns a block parameter by ordinal.
    fn keel_block_parameter_at(module: *mut KeelModule, block: KeelBlock, ordinal: usize, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.block(block)?;
            let value = *m.ir.functions[block.function as usize].block_params(b).get(ordinal).ok_or_else(|| invalid("parameter ordinal out of range"))?;
            Ok(m.value_id(block.function, value))
        }) }
    }
}
api! {
    /// Adds a source path for instruction/function locations and returns its module-owned ID.
    fn keel_source_file_add(module: *mut KeelModule, path: *const u8, length: usize, out: *mut super::KeelSourceFile) {
        // SAFETY: Public pointer contract covers path and output.
        unsafe { output(module, out, |m| {
            let path = utf8(input(path, length)?)?;
            if path.is_empty() || path.contains('\0') { return Err(invalid("source path must be nonempty and contain no NUL")); }
            let index = u32::try_from(m.ir.files.len()).map_err(|_| invalid("too many source files"))?;
            m.ir.files.push(path.into());
            Ok(super::KeelSourceFile { owner: m.owner, index })
        }) }
    }
}
api! {
    /// Sets a source location on one instruction selected by its block ordinal.
    fn keel_instruction_set_source(module: *mut KeelModule, block: KeelBlock, ordinal: usize, file: super::KeelSourceFile, line: u32, column: u32) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            let b = m.block(block)?;
            if file.owner != m.owner || file.index as usize >= m.ir.files.len() { return Err(invalid("foreign, stale or invalid source file ID")); }
            let f = &mut m.ir.functions[block.function as usize];
            let inst = *f.block_insts(b).get(ordinal).ok_or_else(|| invalid("instruction ordinal out of range"))?;
            f.set_inst_loc(inst, Some(SourceLoc { file: SourceFile::new(file.index as usize), line, col: column, scope: None }));
            Ok(())
        }) }
    }
}
api! {
    /// Configures function-local x86 ISA permissions using the core feature-list syntax.
    fn keel_function_set_target_features(module: *mut KeelModule, function: KeelFunction, features: *const u8, length: usize) {
        // SAFETY: Public pointer contract covers feature text.
        unsafe { run(module, |m| {
            let index = m.function_index(function)?;
            let features = utf8(input(features, length)?)?;
            let features = keel_ir::X86TargetFeatures::new([features.into()]).map_err(|e| (INVALID, e))?;
            m.ir.functions[index].attrs.target_features = features;
            let name = m.ir.functions[index].name.clone();
            m.sync_callee_attributes(&name); Ok(())
        }) }
    }
}
api! {
    /// Optimizes using a built-in tier. IDs become stale once passes start.
    fn keel_module_optimize_with_tier(module: *mut KeelModule, tier: u32) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| {
            use keel_opt::OptimizationTier as T;
            let tier = match tier { 0 => T::None, 1 => T::Fast, 2 => T::Basic, 3 => T::Balanced, 4 => T::Size, 5 => T::Full, _ => return Err(invalid("unknown optimization tier")) };
            m.verify()?; m.invalidate_ids();
            keel_opt::optimize_with_tier(&mut m.ir, tier).map_err(|e| (COMPILE,e.to_string()))?; m.verify()
        }) }
    }
}
api! {
    /// Optimizes using comma-separated core pass names, validated before rewriting.
    fn keel_module_run_passes(module: *mut KeelModule, passes: *const u8, length: usize) {
        // SAFETY: Public pointer contract covers pass-list text.
        unsafe { run(module, |m| {
            let text = utf8(input(passes, length)?)?;
            let passes = if text.is_empty() { Vec::new() } else { text.split(',').map(str::trim).collect::<Vec<_>>() };
            if passes.iter().any(|p| keel_opt::find_pass(p).is_none()) { return Err(invalid("unknown optimizer pass")); }
            m.verify()?; m.invalidate_ids();
            keel_opt::run_passes(&mut m.ir, &passes).map_err(|e| (COMPILE,e.to_string()))?; m.verify()
        }) }
    }
}
api! {
    /// Emits native assembly using explicit selection, allocation and emission policies.
    fn keel_module_emit_assembly(module: *mut KeelModule, raw_target: u32, options: KeelCodegenOptions, out: *mut *mut KeelBuffer) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            m.verify()?;
            Ok(Box::into_raw(Box::new(KeelBuffer(compile(&m.ir, target(raw_target)?, options, true)?))))
        }) }
    }
}
api! {
    /// Emits a native object using explicit selection, allocation and emission policies.
    fn keel_module_emit_object_with_options(module: *mut KeelModule, raw_target: u32, options: KeelCodegenOptions, out: *mut *mut KeelBuffer) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            m.verify()?;
            Ok(Box::into_raw(Box::new(KeelBuffer(compile(&m.ir, target(raw_target)?, options, false)?))))
        }) }
    }
}
