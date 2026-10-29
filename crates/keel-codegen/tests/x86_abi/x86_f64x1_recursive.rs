//! Recursive producer policy over the complete independent primary roster.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier, regalloc::AllocationTier, x86_features::FeatureSelector,
};

fn compile(source: &str, features: Features) -> Result<String, keel_codegen::CodegenError> {
    let m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let n = keel_ir::parse(&m.to_string()).unwrap();
    keel_ir::verify_module(&n).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Baseline,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Fast,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(
        &n,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    );
    let typed = keel_codegen::x86_64::compile_program_with_pipeline(
        &n,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    );
    assert_eq!(
        text.is_ok(),
        typed.is_ok(),
        "text and typed routes must agree"
    );
    let text = text?;
    let typed = typed?;
    assert_eq!(
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
        keel_asm::x86::assemble_program_object_with_features(&typed, features).unwrap()
    );
    Ok(text)
}
#[test]
fn all_primary_layouts_named_unnamed_returns_and_compatible_tails() {
    let mut count = 0;
    for row in include_str!("../f64x1_recursive/layouts.txt").lines() {
        let fields: Vec<_> = row.split('|').collect();
        let layout = fields[1];
        let size = fields[2];
        let align = fields[3];
        let home = size.parse::<u32>().unwrap().max(1);
        for producer in ["gnu", "clang"] {
            let tag = format!("x86_vector_abi({producer})");
            let sig = format!("(ptr sret {layout},ptr byval {layout})");
            let source = format!(
                "decl @c_echo{sig} {tag}\nfunc @echo{sig} {tag} {{\nentry(v0:ptr,v1:ptr):\n v2=iconst.i64 {size}\n intrinsic memcpy(v0,v1,v2)\n return\n}}\nfunc @reader(ptr sret {layout},ptr byval {layout},...) {tag} {{\n ss0=stack_slot 24, align 8\n ss1=stack_slot {home}, align {align}\nentry(v0:ptr,v1:ptr):\n v2=stack_addr ss0\n intrinsic va_start(v2)\n v3=va_arg_aggregate {layout},v2,ss1 {tag}\n v4=iconst.i64 {size}\n intrinsic memcpy(v0,v3,v4)\n return\n}}\nfunc @tail{sig} {tag} {{\nentry(v0:ptr,v1:ptr):\n return_call @c_echo(v0,v1)\n}}\nfunc @tail_indirect(ptr sret {layout},ptr byval {layout},ptr) {tag} {{\nentry(v0:ptr,v1:ptr,v2:ptr):\n return_call_indirect v2(v0,v1) : {sig} {tag}\n}}\n"
            );
            compile(&source, Features::for_level(CpuLevel::V3))
                .unwrap_or_else(|e| panic!("layout{} {producer}: {e:?}", fields[0]));
            count += 1;
            let mode = fields[if producer == "gnu" { 5 } else { 6 }];
            let vector32 = size == "32" && mode == "vector";
            if !vector32 {
                compile(&source, Features::NONE)
                    .unwrap_or_else(|e| panic!("baseline layout{} {producer}: {e:?}", fields[0]));
            } else {
                assert!(compile(&source, Features::NONE).is_err());
            }
            // Retain the target's exact named prefix. The unnamed aggregate
            // travels solely through its CallArgs contract, also for tails.
            for indirect in [false, true] {
                let target = if indirect {
                    format!(
                        "return_call_indirect v3(v0,v1,v2 : ptr byval {layout}) : (ptr sret {layout},i64,...) {tag}"
                    )
                } else {
                    format!("return_call @va(v0,v1,v2 : ptr byval {layout})")
                };
                let tails = format!(
                    "decl @va(ptr sret {layout},i64,...) {tag}\nfunc @tail_va(ptr sret {layout},i64,ptr byval {layout},ptr) {tag} {{\nentry(v0:ptr,v1:i64,v2:ptr,v3:ptr):\n {target}\n}}\n"
                );
                let result = compile(&tails, Features::for_level(CpuLevel::V3));
                assert_eq!(
                    result.is_ok(),
                    !vector32,
                    "layout{} {producer}: {result:?}",
                    fields[0]
                );
            }
        }
    }
    assert_eq!(count, 220);
}
#[test]
fn scalar_tail_forwards_protocol_and_aggregate_prefix_stays_exact() {
    for producer in ["gnu", "clang"] {
        let tag = format!("x86_vector_abi({producer})");
        let text=compile(&format!("decl @target(f64x1)->f64x1 {tag}\nfunc @tail(f64x1)->f64x1 {tag} {{\nentry(v0:f64x1):\n return_call @target(v0)\n}}"),Features::NONE).unwrap();
        assert!(text.contains("jmp\t"));
        assert!(!text.contains("call\t"));
        let source = format!(
            "decl @sink(i64,...) {tag}\nfunc @send(ptr) {{\nentry(v0:ptr):\n v1=iconst.i64 7\n call @sink(v1 : i64,v0 : ptr byval {{[2 x f64x1],i8}})\n return\n}}"
        );
        let m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let f = &m.functions[0];
        let (_, decl) = f.callees().next().unwrap();
        assert_eq!(decl.sig.params.len(), 1);
        assert!(decl.sig.variadic);
        compile(&source, Features::NONE).unwrap();
    }
}

#[test]
fn contextual_memory_width_checks_every_contract_without_restricting_byte_copies() {
    let memory = "{f64x1,i64x4}";
    let register_y = "{[0 x f64x1],i64x4}";
    for producer in ["gnu", "clang"] {
        let tag = format!("x86_vector_abi({producer})");
        for variadic in [false, true] {
            let suffix = if variadic { ",..." } else { "" };
            let sig = format!("(ptr sret {memory},ptr byval {memory}{suffix})");
            let src = format!(
                "decl @target{sig} {tag}\nfunc @direct{sig} {tag} {{\nentry(v0:ptr,v1:ptr):\n call @target(v0,v1)\n return\n}}\nfunc @indirect(ptr sret {memory},ptr byval {memory},ptr) {tag} {{\nentry(v0:ptr,v1:ptr,v2:ptr):\n call_indirect v2(v0,v1) : {sig} {tag}\n return\n}}\n"
            );
            compile(&src, Features::NONE).unwrap();
            let m = keel_ir::parse(&src).unwrap();
            assert!(keel_codegen::aarch64::compile(&m).is_err());
        }
        for src in [
            format!("decl @wide(i64x4) {tag}"),
            format!("decl @wide()->i64x4 {tag}"),
            format!(
                "func @wide(ptr)->i64x4 {tag} {{\nentry(v0:ptr):\n v1=load.i64x4 v0\n return v1\n}}"
            ),
            // A producer tag never proves a scalar-layout byval contract.
            format!("decl @unproven(ptr byval f64x1) {tag}"),
            format!("decl @unproven(ptr byval {{[0 x i64x4],i64x4}}) {tag}"),
        ] {
            assert!(compile(&src, Features::NONE).is_err(), "{src}");
        }
        if producer == "gnu" {
            for purpose in ["byval", "sret"] {
                assert!(
                    compile(
                        &format!("decl @y(ptr {purpose} {register_y}) {tag}"),
                        Features::NONE
                    )
                    .is_err()
                );
            }
        } else {
            compile(
                &format!("decl @y(ptr sret {register_y},ptr byval {register_y}) {tag}"),
                Features::NONE,
            )
            .unwrap();
        }
        // The callee's metadata governs unnamed operands, independently of
        // the enclosing function's producer policy.
        for indirect in [false, true] {
            let call = if indirect {
                format!("call_indirect v1(v0 : ptr byval {memory}) : (...)")
            } else {
                format!("call @plain(v0 : ptr byval {memory})")
            };
            let src = format!(
                "decl @plain(...)\nfunc @caller(ptr,ptr) {tag} {{\nentry(v0:ptr,v1:ptr):\n {call}\n return\n}}"
            );
            assert!(compile(&src, Features::NONE).is_err());
        }
    }
    // An unnamed GNU SSEUP/Y layout is a memory operand even though its
    // named form requires YMM. Neither its reader nor caller creates wide SSA.
    for producer in ["gnu", "clang"] {
        let tag = format!("x86_vector_abi({producer})");
        let src = format!(
            "decl @sink(i64,...) {tag}\nfunc @send(ptr) {{\nentry(v0:ptr):\n v1=iconst.i64 7\n call @sink(v1,v0 : ptr byval {register_y})\n return\n}}\nfunc @reader(i64,...) {tag} {{\n ss0=stack_slot 24, align 8\n ss1=stack_slot 32, align 32\nentry(v0:i64):\n v1=stack_addr ss0\n intrinsic va_start(v1)\n v2=va_arg_aggregate {register_y},v1,ss1 {tag}\n return\n}}\n"
        );
        compile(&src, Features::NONE).unwrap();
    }
    // Unused imported declarations and indirect signature tables are still
    // executable contracts and must pass the exact same validation.
    for producer in ["gnu", "clang"] {
        for imported in [false, true] {
            for (layout, rejected) in [("i64x4", true), ("ptr byval {f64x1,i64x4}", false)] {
                let mut module = keel_ir::parse("func @empty() {\nentry:\n return\n}").unwrap();
                let decl = keel_ir::parse(&format!(
                    "decl @contract({layout}) x86_vector_abi({producer})"
                ))
                .unwrap();
                if imported {
                    module.functions[0].import_function("contract", decl.decls[0].sig.clone());
                } else {
                    module.functions[0].import_signature(decl.decls[0].sig.clone());
                }
                assert_eq!(keel_codegen::x86_64::compile(&module).is_err(), rejected);
                assert!(keel_codegen::aarch64::compile(&module).is_err());
            }
        }
    }
    // Default validation on other backends still inspects aggregate leaves.
    let plain = keel_ir::parse("decl @wide(ptr byval {i64x4,i64})").unwrap();
    assert!(keel_codegen::aarch64::compile(&plain).is_err());
}

#[test]
fn cross_producer_tails_require_complete_result_protocol_compatibility() {
    for (caller, callee) in [("gnu", "clang"), ("clang", "gnu")] {
        for indirect in [false, true] {
            for (layout, compatible) in [
                ("{[2 x f64x1],i64x4}", true),
                ("{[0 x f64x1],i64x4}", false),
            ] {
                let sig = format!("(ptr sret {layout},ptr byval {layout})");
                let target = if indirect {
                    format!("return_call_indirect v2(v0,v1) : {sig} x86_vector_abi({callee})")
                } else {
                    "return_call @target(v0,v1)".into()
                };
                let src = format!(
                    "decl @target{sig} x86_vector_abi({callee})\nfunc @tail(ptr sret {layout},ptr byval {layout},ptr) x86_vector_abi({caller}) {{\nentry(v0:ptr,v1:ptr,v2:ptr):\n {target}\n}}"
                );
                assert_eq!(
                    compile(&src, Features::for_level(CpuLevel::V3)).is_ok(),
                    compatible,
                    "{src}"
                );
            }
            let target = if indirect {
                format!("return_call_indirect v1(v0) : (f64x1)->f64x1 x86_vector_abi({callee})")
            } else {
                "return_call @target(v0)".into()
            };
            let src = format!(
                "decl @target(f64x1)->f64x1 x86_vector_abi({callee})\nfunc @tail(f64x1,ptr)->f64x1 x86_vector_abi({caller}) {{\nentry(v0:f64x1,v1:ptr):\n {target}\n}}"
            );
            assert!(compile(&src, Features::NONE).is_err());
        }
    }
}

#[test]
fn inlining_keeps_exact_foreign_prefix_layout_and_producer() {
    let layout = "{[0 x f64x1],i64x4}";
    let source = format!(
        "decl @foreign(ptr sret {layout},i64,...) x86_vector_abi(clang)\nfunc @helper(ptr,ptr) x86_vector_abi(gnu) local {{\nentry(v0:ptr,v1:ptr):\n v2=iconst.i64 17\n call @foreign(v0,v2,v1 : ptr byval {layout})\n return\n}}\nfunc @outer(ptr,ptr) x86_vector_abi(gnu) {{\nentry(v0:ptr,v1:ptr):\n call @helper(v0,v1)\n return\n}}"
    );
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let outer = module.functions.iter().find(|f| f.name == "outer").unwrap();
    let mut found = false;
    for (_, inst) in outer.layout_insts() {
        if let keel_ir::InstData::Call { func, args } = outer.inst_data(inst) {
            let callee = outer.callee(*func);
            assert_eq!(callee.name, "foreign", "local adapter must inline");
            assert_eq!(callee.sig.params.len(), 2);
            assert!(callee.sig.variadic);
            assert_eq!(
                callee.sig.x86_vector_abi,
                Some(keel_ir::X86VectorAbi::Clang)
            );
            assert_eq!(args.len(), 3);
            assert_eq!(args.abi(2).unwrap().layout(), callee.sig.params[0].layout());
            found = true;
        }
    }
    assert!(found);
    compile(&module.to_string(), Features::NONE).unwrap();
}
