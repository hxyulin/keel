//! Immutable fixture preparation shared across allocator and pool combinations.
use keel_codegen::SelectionTier;
use keel_ir::Module;
use std::sync::OnceLock;

#[derive(Debug)]
pub struct PreparedModule {
    raw: Module,
    optimized: OnceLock<Module>,
}

impl PreparedModule {
    pub fn parse(source: &str) -> Self {
        let raw = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&raw).unwrap();
        Self {
            raw,
            optimized: OnceLock::new(),
        }
    }

    pub fn for_selector(&self, selector: SelectionTier) -> &Module {
        if selector == SelectionTier::Full {
            self.optimized.get_or_init(|| {
                let mut module = self.raw.clone();
                keel_opt::optimize(&mut module).unwrap();
                keel_ir::verify_module(&module).unwrap();
                module
            })
        } else {
            &self.raw
        }
    }
}
