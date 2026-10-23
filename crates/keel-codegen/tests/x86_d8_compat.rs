//! Retained Q/Y object witness. Run this same file against the prerequisite
//! backend and the D8 backend; compare the complete object bytes, not mnemonics.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, path::PathBuf};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;
#[test]
fn retained_qy_object_witness() {
    let shapes: Vec<_> = keel_ir::VectorType::ALL_128
        .into_iter()
        .chain(keel_ir::VectorType::ALL_256)
        .collect();
    let mut source = common::fixture_for(&shapes);
    for shape in &shapes {
        source += &format!(
            "decl @external_{shape}({shape})->{shape}\nfunc @transport_{shape}(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n v2 = load.{shape} align 1 v0\n v3 = call @external_{shape}(v2)\n store align 1 v2,v1\n store align 1 v3,v0\n return\n}}\n"
        );
        source += &format!(
            "func @var_{shape}(i64,...)->{shape} {{\nss0 = stack_slot 24, align 8\nentry(v0:i64):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n v2 = va_arg.{shape} v1\n return v2\n}}\n"
        );
    }
    let export = std::env::var_os("KEEL_X86_D8_QY_WITNESS_DIR").map(PathBuf::from);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        let mut m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        if s == SelectionTier::Full {
            keel_opt::optimize(&mut m).unwrap();
        }
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (n, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("empty", &empty),
            ] {
                let features = Features::for_level(CpuLevel::V3);
                let selector = FeatureSelector {
                    inner: &s,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &a,
                };
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let typed =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap();
                if let Some(d) = &export {
                    fs::create_dir_all(d).unwrap();
                    fs::write(d.join(format!("{s:?}-{a:?}-{n}.o")), typed).unwrap();
                }
            }
        }
    }
}
