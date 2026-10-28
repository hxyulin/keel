//! Generic unnamed aggregate transport. Native execution is root-owned.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Purpose};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn small_source() -> &'static str {
    "decl @sink(i64,...)\nfunc @send(ptr,ptr){\nentry(v0:ptr,v1:ptr):\n v2=iconst.i64 7\n call @sink(v2 : i64,v0 : ptr byval {f64,i64},v1 : ptr byval {{i32x4}})\n return\n}\nfunc @read(i64,...)->ptr{\n ss0=stack_slot 24, align 8\n ss1=stack_slot 16, align 16\n ss2=stack_slot 16, align 16\nentry(v0:i64):\n v1=stack_addr ss0\n intrinsic va_start(v1)\n v2=va_arg_aggregate {f64,i64},v1,ss1\n v3=va_arg_aggregate {{i32x4}},v1,ss2\n v4=load.i64 v2\n store v4,v3\n return v2\n}\n"
}
#[test]
fn materialization_and_actual_named_prefix_roundtrip() {
    let m = keel_ir::parse(small_source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let text = m.to_string();
    let n = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&n).unwrap();
    assert_eq!(text, n.to_string());
    let f = &n.functions[0];
    let (_, decl) = f.callees().next().unwrap();
    assert_eq!(decl.sig.params.len(), 1);
    assert!(decl.sig.variadic);
    let call = f
        .layout_insts()
        .find_map(|(_, i)| f.inst_data(i).call_args())
        .unwrap();
    assert!(matches!(call.abi(1).unwrap().purpose, Purpose::ByVal(_)));
    for (_, i) in n.functions[1].layout_insts() {
        if let InstData::AggregateVaArg { .. } = n.functions[1].inst_data(i) {
            assert!(n.functions[1].inst_data(i).is_pinned());
            assert!(n.functions[1].inst_data(i).may_trap());
            assert_eq!(
                n.functions[1].inst_data(i).memory_effects(),
                keel_ir::MemoryEffects::ReadWrite
            );
        }
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                let selector = FeatureSelector {
                    inner: &s,
                    features: Features::NONE,
                };
                let p = CodegenPipeline {
                    selector: &selector,
                    allocator: &a,
                };
                let out = keel_codegen::x86_64::compile_with_pipeline(&n, pools, &p).unwrap();
                let typed =
                    keel_codegen::x86_64::compile_program_with_pipeline(&n, pools, &p).unwrap();
                let bytes =
                    keel_asm::x86::assemble_program_object_with_features(&typed, Features::NONE)
                        .unwrap();
                assert_eq!(
                    bytes,
                    keel_asm::x86::assemble_object_with_features(&out, Features::NONE).unwrap()
                );
            }
        }
    }
}
#[test]
fn i128_aggregate_ir_uses_two_gp_slots_and_never_fp_cursor() {
    for layout in ["{i128}", "[1 x i128]", "{{[1 x i128]}}"] {
        let source = format!(
            "func @read(i64,...)->ptr{{\n ss0=stack_slot 24, align 8\n ss1=stack_slot 16, align 16\nentry(v0:i64):\n v1=stack_addr ss0\n intrinsic va_start(v1)\n v2=va_arg_aggregate {layout},v1,ss1\n return v2\n}}\n"
        );
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(
            module.to_string(),
            keel_ir::parse(&module.to_string()).unwrap().to_string()
        );
        let assembly = keel_codegen::x86_64::compile(&module).unwrap();
        assert!(assembly.contains("cmp\teax, 32"), "{assembly}");
        assert!(assembly.contains("add\tdword ptr [r10], 16"), "{assembly}");
        assert!(!assembly.contains("dword ptr [r10 + 4]"), "{assembly}");
    }
}

#[test]
fn no_class_aggregate_ir_skips_padding_without_advancing_other_bank() {
    for (layout, changed, absent) in [
        (
            "{i64,[0 x i128]}",
            "add\tdword ptr [r10], 8",
            "dword ptr [r10 + 4]",
        ),
        (
            "{{f64,[0 x i128]}}",
            "add\tdword ptr [r10 + 4], 16",
            "dword ptr [r10]",
        ),
    ] {
        let source = format!(
            "func @read(ptr)->ptr{{\n ss0=stack_slot 16, align 16\nentry(v0:ptr):\n v1=va_arg_aggregate {layout},v0,ss0\n return v1\n}}\n"
        );
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let assembly = keel_codegen::x86_64::compile(&module).unwrap();
        assert!(assembly.contains(changed), "{assembly}");
        assert!(!assembly.contains(absent), "{assembly}");
    }
}

#[test]
fn supported_tail_jumps_and_unsupported_contracts_reject() {
    let m = keel_ir::parse(small_source()).unwrap();
    assert!(
        keel_codegen::aarch64::compile(&m)
            .unwrap_err()
            .to_string()
            .contains("unsupported")
    );
    let src = "decl @sink(i64,...)\nfunc @tail(ptr,i64,...){\nentry(v0:ptr,v1:i64):\n return_call @sink(v1,v0 : ptr byval {i64,f64})\n}\n";
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let assembly = keel_codegen::x86_64::compile(&m).unwrap();
    assert!(assembly.contains("\tjmp\t\"sink\"@PLT"), "{assembly}");
    assert!(
        !assembly
            .lines()
            .any(|line| line.trim_start().starts_with("call\t"))
    );
    keel_asm::x86::assemble_object_with_features(&assembly, Features::NONE).unwrap();
    let bad = small_source().replace("{f64,i64}", "{f64x1}");
    let m = keel_ir::parse(&bad).unwrap();
    assert!(
        keel_codegen::x86_64::compile(&m)
            .unwrap_err()
            .to_string()
            .contains("f64x1")
    );
}
#[test]
fn cloning_and_inlining_preserve_contracts_and_explicit_home_identity() {
    let source = small_source().replace("func @send(ptr,ptr)", "func @send(ptr,ptr) local")
        + "func @top(ptr,ptr) {\nentry(v0:ptr,v1:ptr):\n call @send(v0,v1)\n return\n}\n";
    let mut m = keel_ir::parse(&source).unwrap();
    let f = &m.functions[1];
    let homes: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, i)| match f.inst_data(i) {
            InstData::AggregateVaArg { home, .. } => {
                let InstData::AggregateVaArg { home: cloned, .. } = f.inst_data(i).clone() else {
                    unreachable!()
                };
                assert_eq!(*home, cloned);
                Some(*home)
            }
            _ => None,
        })
        .collect();
    assert_eq!(homes.len(), 2);
    assert_ne!(homes[0], homes[1]);
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let top = m.functions.iter().find(|f| f.name == "top").unwrap();
    let args = top
        .layout_insts()
        .find_map(|(_, i)| match top.inst_data(i) {
            InstData::Call { func, args } if top.callee(*func).name == "sink" => Some(args),
            _ => None,
        })
        .expect("wrapper inlined");
    assert!(matches!(args.abi(1).unwrap().purpose, Purpose::ByVal(_)));
    assert!(matches!(args.abi(2).unwrap().purpose, Purpose::ByVal(_)));
}

#[test]
fn y_layout_hidden_behind_ptr_requires_avx_before_encoding() {
    use keel_asm_ir::x86::CpuLevel;
    let source = small_source()
        .replace("i32x4", "i32x8")
        .replace("stack_slot 16, align 16", "stack_slot 32, align 32");
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Fast,
    };
    let error = keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
        .unwrap_err();
    assert!(error.to_string().contains("256-bit"));
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features: Features::for_level(CpuLevel::V3),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Fast,
    };
    keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline).unwrap();
}

#[test]
fn invoke_to_call_and_tail_text_keep_abi_descriptors() {
    let source = "decl @sink(i64,...) nounwind\ndecl @personality()\nfunc @invoke(ptr,i64) personality @personality {\nentry(v0:ptr,v1:i64):\n invoke @sink(v1 : i64,v0 : ptr byval {i64,f64}),normal,unwind\nnormal:\n return\nunwind:\n v2,v3=landingpad cleanup\n return\n}\nfunc @tail(ptr,i64,...) {\nentry(v0:ptr,v1:i64):\n return_call @sink(v1 : i64,v0 : ptr byval {i64,f64})\n}\nfunc @indirect_tail(ptr,ptr,i64,...) {\nentry(v0:ptr,v1:ptr,v2:i64):\n return_call_indirect v0(v2 : i64,v1 : ptr byval {i64,f64}) : (i64,...)\n}\n";
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = module.clone().to_string();
    assert_eq!(keel_ir::parse(&text).unwrap().to_string(), text);
    for f in &module.functions {
        let args = f
            .layout_insts()
            .find_map(|(_, i)| f.inst_data(i).call_args())
            .unwrap();
        assert!(matches!(args.abi(1).unwrap().purpose, Purpose::ByVal(_)));
    }
    keel_opt::run_passes(&mut module, &["call-contracts"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(!module.functions[0].to_string().contains("invoke @sink"));
    let args = module.functions[0]
        .layout_insts()
        .find_map(|(_, i)| module.functions[0].inst_data(i).call_args())
        .unwrap();
    assert!(matches!(args.abi(1).unwrap().purpose, Purpose::ByVal(_)));
}
