//! Bound ABI metadata before any expanded leaf walk or unchecked size use.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier, regalloc::AllocationTier, x86_features::FeatureSelector,
};
fn both(module: &keel_ir::Module, accepted: bool, features: Features) {
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Fast,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(
        module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    );
    let typed = keel_codegen::x86_64::compile_program_with_pipeline(
        module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    );
    assert_eq!(text.is_ok(), accepted, "{text:?}");
    assert_eq!(typed.is_ok(), accepted, "{typed:?}");
}
#[test]
fn huge_positive_and_overflow_layouts_reject_at_every_early_contract() {
    for producer in ["gnu", "clang"] {
        for (layout, ir_ok) in [
            ("[536870911 x f64x1]", true),
            ("[536870912 x f64x1]", false),
            ("[4294967295 x [4294967295 x f64x1]]", false),
            ("{[4294967295 x [4294967295 x f64x1]],i64}", false),
        ] {
            let tag = format!("x86_vector_abi({producer})");
            let decl = keel_ir::parse(&format!("decl @huge(ptr byval {layout}) {tag}")).unwrap();
            assert_eq!(keel_ir::verify_module(&decl).is_ok(), ir_ok);
            both(&decl, false, Features::NONE);
            let defined = keel_ir::parse(&format!(
                "func @huge(ptr byval {layout}) {tag} {{\nentry(v0:ptr):\n return\n}}"
            ))
            .unwrap();
            assert_eq!(keel_ir::verify_module(&defined).is_ok(), ir_ok);
            both(&defined, false, Features::NONE);
            for callee in [true, false] {
                let mut module = keel_ir::parse("func @empty(){\nentry:\n return\n}").unwrap();
                if callee {
                    module.functions[0].import_function("huge", decl.decls[0].sig.clone());
                } else {
                    module.functions[0].import_signature(decl.decls[0].sig.clone());
                }
                assert_eq!(keel_ir::verify_module(&module).is_ok(), ir_ok && !callee);
                both(&module, false, Features::NONE);
            }
            // Only the actual unnamed contract reveals the large type; both direct
            // and indirect paths must check it before planning outgoing stack space.
            for indirect in [false, true] {
                let call = if indirect {
                    format!("call_indirect v1(v0 : ptr byval {layout}) : (...) {tag}")
                } else {
                    format!("call @sink(v0 : ptr byval {layout})")
                };
                let module=keel_ir::parse(&format!("decl @sink(...) {tag}\nfunc @send(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n {call}\n return\n}}")).unwrap();
                assert_eq!(keel_ir::verify_module(&module).is_ok(), ir_ok);
                both(&module, false, Features::NONE);
            }
            let module=keel_ir::parse(&format!("func @reader(i64,...) {tag} {{\n ss0=stack_slot 24, align 8\n ss1=stack_slot 32, align 32\nentry(v0:i64):\n v1=stack_addr ss0\n intrinsic va_start(v1)\n v2=va_arg_aggregate {layout},v1,ss1 {tag}\n return\n}}")).unwrap();
            assert!(keel_ir::verify_module(&module).is_err());
            both(&module, false, Features::NONE);
        }
        let module=keel_ir::parse(&format!("decl @budget(ptr byval [50000000 x f64x1],ptr byval [50000000 x f64x1],ptr byval [50000000 x f64x1]) x86_vector_abi({producer})")).unwrap();
        keel_ir::verify_module(&module).unwrap();
        both(&module, false, Features::NONE);
    }
}
#[test]
fn bounded_memory_and_huge_zero_elements_do_not_expand_and_keep_structure() {
    for producer in ["gnu", "clang"] {
        for layout in [
            "[1048576 x f64x1]",
            "{[0 x f64x1],[1048576 x i8]}",
            "[4294967295 x {[0 x f64x1]}]",
        ] {
            let decl = keel_ir::parse(&format!(
                "decl @bounded(ptr byval {layout}) x86_vector_abi({producer})"
            ))
            .unwrap();
            keel_ir::verify_module(&decl).unwrap();
            both(&decl, true, Features::NONE);
            for callee in [true, false] {
                let mut module = keel_ir::parse("func @empty(){\nentry:\n return\n}").unwrap();
                if callee {
                    module.functions[0].import_function("bounded", decl.decls[0].sig.clone());
                    module.decls.push(decl.decls[0].clone());
                } else {
                    module.functions[0].import_signature(decl.decls[0].sig.clone());
                }
                keel_ir::verify_module(&module).unwrap();
                both(&module, true, Features::NONE);
            }
        }
        let layout = "{[4294967295 x {[0 x f64x1]}],i64x4}";
        let module = keel_ir::parse(&format!(
            "decl @split(ptr sret {layout},ptr byval {layout}) x86_vector_abi({producer})"
        ))
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let l = module.decls[0].sig.params[0].layout().unwrap();
        assert_eq!(l.checked_size(), Some(32));
        assert_eq!(l.align(), 32);
        assert_eq!(
            l.leaves(),
            vec![(0, keel_ir::Type::Vector(keel_ir::VectorType::I64x4))]
        );
        both(&module, producer == "clang", Features::NONE);
        both(&module, true, Features::for_level(CpuLevel::V3));
    }
}

#[test]
fn bounded_huge_defined_memory_result_does_not_expand_payload() {
    for producer in ["gnu", "clang"] {
        let module = keel_ir::parse(&format!(
            "func @large_sret(ptr sret [100000000 x f64x1]) x86_vector_abi({producer}) {{\nentry(v0:ptr):\n return\n}}"
        )).unwrap();
        keel_ir::verify_module(&module).unwrap();
        both(&module, true, Features::NONE);
    }
}
