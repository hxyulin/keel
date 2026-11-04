//! The compile-and-compare step that codegen tests share.
//!
//! [`Case`] compiles one module through the text route and the typed route,
//! assembles both, and asserts that the two objects are byte-identical.
//! Options add the checks some tests need on top: no opaque text in the typed
//! program, chunked compilation producing the same object, and every x86
//! instruction staying within the enabled features.
//!
//! [`matrix`] iterates the selection, allocation and pool combinations that
//! tests otherwise spell out as nested loops.
use keel_asm_ir::Program;
use keel_codegen::regalloc::{AllocationTier, Pools, RegisterAllocator};
use keel_codegen::x86_features::{FeatureSelector, Features};
use keel_codegen::{CodegenError, CodegenPipeline, InstructionSelector, SelectionTier};
use keel_ir::Module;

/// Pools with no registers, so every value spills.
pub const EMPTY: Pools<'static> = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

pub const SELECTIONS: [SelectionTier; 3] = [
    SelectionTier::Baseline,
    SelectionTier::Trees,
    SelectionTier::Full,
];

pub const ALLOCATIONS: [AllocationTier; 2] = [AllocationTier::Fast, AllocationTier::Full];

/// Every combination of `selections`, [`ALLOCATIONS`] and `pools`, with the
/// selection outermost and the pools innermost.
pub fn matrix<'p>(
    selections: &'p [SelectionTier],
    pools: &'p [&'p Pools<'p>],
) -> impl Iterator<Item = (SelectionTier, AllocationTier, &'p Pools<'p>)> + 'p {
    selections.iter().flat_map(move |&selection| {
        ALLOCATIONS.iter().flat_map(move |&allocation| {
            pools
                .iter()
                .map(move |&pools| (selection, allocation, pools))
        })
    })
}

/// Parses and verifies a module.
pub fn parse(source: &str) -> Module {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

/// One module compiled for one target and pipeline configuration.
pub struct Case<'a> {
    module: &'a Module,
    arch: Arch,
    selector: &'a dyn InstructionSelector,
    allocator: &'a dyn RegisterAllocator,
    pools: Option<&'a Pools<'a>>,
    features: Option<Features>,
    no_text_fallback: bool,
    chunks: bool,
    within_features: bool,
}

/// The outputs of a [`Case`]. The object came from the typed program, and the
/// text route assembled to the same bytes.
pub struct Compiled {
    pub text: String,
    pub program: Program,
    pub object: Vec<u8>,
}

impl<'a> Case<'a> {
    pub fn new(arch: Arch, module: &'a Module) -> Self {
        Self {
            module,
            arch,
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Full,
            pools: None,
            features: None,
            no_text_fallback: false,
            chunks: false,
            within_features: false,
        }
    }

    pub fn x86(module: &'a Module) -> Self {
        Self::new(Arch::X86_64, module)
    }

    pub fn aarch64(module: &'a Module) -> Self {
        Self::new(Arch::Aarch64, module)
    }

    /// The instruction selector. The default is [`SelectionTier::Baseline`].
    pub fn selector(mut self, selector: &'a dyn InstructionSelector) -> Self {
        self.selector = selector;
        self
    }

    /// The register allocator. The default is [`AllocationTier::Full`].
    pub fn allocator(mut self, allocator: &'a dyn RegisterAllocator) -> Self {
        self.allocator = allocator;
        self
    }

    /// The register pools. The default is the target's `REGISTERS`.
    pub fn pools(mut self, pools: &'a Pools<'a>) -> Self {
        self.pools = Some(pools);
        self
    }

    /// x86 only: wraps the selector in a [`FeatureSelector`] and assembles
    /// both routes with `features`.
    pub fn features(mut self, features: Features) -> Self {
        assert_eq!(self.arch, Arch::X86_64, "features are x86-only");
        self.features = Some(features);
        self
    }

    /// Asserts that the typed program holds no opaque text.
    pub fn no_text_fallback(mut self) -> Self {
        self.no_text_fallback = true;
        self
    }

    /// Also compiles through the chunked entry point and asserts that the
    /// collected chunks assemble to the same object.
    pub fn chunks(mut self) -> Self {
        self.chunks = true;
        self
    }

    /// x86 only: asserts that every instruction of the typed program (and of
    /// the chunks, when enabled) requires only the enabled features.
    pub fn within_features(mut self) -> Self {
        assert_eq!(self.arch, Arch::X86_64, "features are x86-only");
        self.within_features = true;
        self
    }

    /// Compiles, panicking on a codegen error.
    pub fn compile(self) -> Compiled {
        self.try_compile().unwrap()
    }

    /// Compiles, returning the first codegen error. Assembly failures and
    /// object mismatches still panic.
    pub fn try_compile(self) -> Result<Compiled, CodegenError> {
        let feature_selector;
        let selector: &dyn InstructionSelector = match self.features {
            Some(features) => {
                feature_selector = FeatureSelector {
                    inner: self.selector,
                    features,
                };
                &feature_selector
            }
            None => self.selector,
        };
        let pipeline = CodegenPipeline {
            selector,
            allocator: self.allocator,
        };
        let module = self.module;
        let features = self.features.unwrap_or_default();
        let (text, program, chunks) = match self.arch {
            Arch::X86_64 => {
                use keel_codegen::x86_64::*;
                let pools = self.pools.unwrap_or(&REGISTERS);
                let text = compile_with_pipeline(module, pools, &pipeline)?;
                let program = compile_program_with_pipeline(module, pools, &pipeline)?;
                let chunks = if self.chunks {
                    let mut chunks = Program::default();
                    compile_program_chunks_with_pipeline(module, pools, &pipeline, &mut |p| {
                        chunks.items.extend(p.items)
                    })?;
                    Some(chunks)
                } else {
                    None
                };
                (text, program, chunks)
            }
            Arch::Aarch64 => {
                use keel_codegen::aarch64::*;
                let pools = self.pools.unwrap_or(&REGISTERS);
                let text = compile_with_pipeline(module, pools, &pipeline)?;
                let program = compile_program_with_pipeline(module, pools, &pipeline)?;
                let chunks = if self.chunks {
                    let mut chunks = Program::default();
                    compile_program_chunks_with_pipeline(module, pools, &pipeline, &mut |p| {
                        chunks.items.extend(p.items)
                    })?;
                    Some(chunks)
                } else {
                    None
                };
                (text, program, chunks)
            }
        };
        let programs = std::iter::once(&program).chain(&chunks);
        for program in programs.clone() {
            if self.no_text_fallback {
                assert_eq!(program.stats().text_bytes, 0, "opaque text in\n{text}");
            }
            if self.within_features {
                for item in &program.items {
                    if let keel_asm_ir::Item::X86Instruction(i) = item {
                        assert!(features.contains(i.required_features()), "{i}");
                    }
                }
            }
        }
        let assemble_program = |program: &Program| match self.arch {
            Arch::X86_64 => keel_asm::x86::assemble_program_object_with_features(program, features),
            Arch::Aarch64 => keel_asm::assemble_program_object(program, Default::default()),
        };
        let object = assemble_program(&program).unwrap_or_else(|e| panic!("{e}\n{text}"));
        let parsed = match self.arch {
            Arch::X86_64 => keel_asm::x86::assemble_object_with_features(&text, features),
            Arch::Aarch64 => keel_asm::assemble_object(&text),
        }
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert!(object == parsed, "typed and text objects differ\n{text}");
        if let Some(chunks) = &chunks {
            let chunked = assemble_program(chunks).unwrap_or_else(|e| panic!("{e}\n{text}"));
            assert!(object == chunked, "chunked object differs\n{text}");
        }
        Ok(Compiled {
            text,
            program,
            object,
        })
    }
}
