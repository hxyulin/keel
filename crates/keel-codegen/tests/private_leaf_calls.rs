#[path = "private_leaf_calls/private_call_chains.rs"]
mod private_call_chains;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[path = "common/native_oracle.rs"]
mod native_oracle;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
const SOURCE: &str = include_str!("private_leaf_calls/pressure.keel");
const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

struct ScratchEdit;
impl keel_codegen::regalloc::RegisterAllocator for ScratchEdit {
    fn name(&self) -> &'static str {
        "leaf-transfer-clobber"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        let mut alloc = AllocationTier::Full.allocate(input);
        if input.func.name == "leaf" {
            let (_, ret) = input
                .func
                .layout_insts()
                .find(|(_, i)| matches!(input.func.inst_data(*i), keel_ir::InstData::Return { .. }))
                .unwrap();
            let keel_ir::InstData::Return { args } = input.func.inst_data(ret) else {
                unreachable!()
            };
            let src = alloc.loc(input.func.resolve(args[0])).unwrap();
            alloc
                .edits
                .instruction_mut(ret)
                .before
                .push((keel_codegen::regalloc::Loc::Reg(9), src));
        }
        alloc
    }
}

fn fixture() -> String {
    let caller = SOURCE.split("func @caller").nth(1).unwrap();
    let pressure = caller
        .replace("(ptr) -> i64", "(ptr) -> i64 cc(fast) local")
        .replace(" v37 = call @leaf(v3)", " v37 = iconst.i64 0");
    let two = caller.replace(
        " v37 = call @leaf(v3)",
        " v50 = call @leaf(v3)\n v51 = call @pressure_leaf(v0)\n v37 = iadd v50, v51",
    );
    let mixed = caller.replace(
        " v37 = call @leaf(v3)",
        " v50 = call @leaf(v3)\n v51 = call @external_noise(v3)\n v37 = iadd v50, v51",
    );
    let tls = caller.replace("call @leaf(v3)", "call @tls_leaf(v3)");
    format!(
        "decl @external_noise(i64) -> i64\ndata @tls = thread_local align 8 {{ i64 5 }}\n{SOURCE}\nfunc @pressure_leaf{pressure}\nfunc @two_leaves{two}\nfunc @mixed{mixed}\nfunc @tls_caller{tls}\nfunc @tls_leaf(i64) -> i64 cc(fast) local {{\nblock0(v0:i64):\n v1 = symbol_addr @tls\n v2 = load.i64 v1\n v3 = iadd v0, v2\n return v3\n}}\n"
    )
}

#[test]
fn proven_leaf_removes_caller_spills_with_both_allocators() {
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let m = keel_ir::parse(SOURCE).unwrap();
            let old = keel_ir::parse(&SOURCE.replace("cc(fast)", "cc(c)")).unwrap();
            let emit = |m: &keel_ir::Module| {
                keel_codegen::aarch64::compile_with_pipeline(
                    m,
                    &keel_codegen::aarch64::REGISTERS,
                    &pipeline,
                )
                .unwrap()
            };
            let a = emit(&m);
            let b = emit(&old);
            let body = |s: String| {
                s.split("_caller:\n")
                    .nth(1)
                    .unwrap()
                    .split("Lcaller_end:")
                    .next()
                    .unwrap()
                    .to_owned()
            };
            let (a, b) = (body(a), body(b));
            assert!(!a.contains("[sp, #0]"), "{a}");
            assert!(!a.contains("[sp, #8]"), "{a}");
            assert!(b.contains("[sp, #0]"));
            assert!(a.matches("stp\t").count() < b.matches("stp\t").count());
        }
    }
}

#[test]
fn forward_references_custom_pools_tls_and_different_leaf_clobbers_agree_on_object_routes() {
    let m = keel_ir::parse(&fixture()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            for pools in [&keel_codegen::aarch64::REGISTERS, &TINY, &EMPTY] {
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                assert_eq!(
                    keel_asm::assemble_object(&text).unwrap(),
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                );
                assert!(text.find("_caller:\n").unwrap() < text.find("_pressure_leaf:\n").unwrap());
            }
        }
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_pressure_known_unknown_and_tls_callees_preserve_values() {
    let m = keel_ir::parse(&fixture()).unwrap();
    let oracle = native_oracle::PreparedOracle::new(
        "private-leaf-calls",
        include_str!("private_leaf_calls/oracle.c"),
        &["-O2"],
    );
    // The text and typed routes must agree, and an object that already ran
    // gives the same executable, so each distinct object runs once.
    let mut ran = std::collections::HashSet::new();
    let mut configurations = 0;
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [
            &AllocationTier::Fast as &dyn keel_codegen::regalloc::RegisterAllocator,
            &AllocationTier::Full,
            &ScratchEdit,
        ] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator,
            };
            for pools in [&keel_codegen::aarch64::REGISTERS, &TINY, &EMPTY] {
                if allocator.name() == "leaf-transfer-clobber"
                    && pools.caller_saved != keel_codegen::aarch64::REGISTERS.caller_saved
                {
                    continue;
                }
                let context = format!("{selector:?} {} {:?}", allocator.name(), pools.caller_saved);
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let object = keel_asm::assemble_object(&text).unwrap();
                assert_eq!(
                    object,
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                    "{context}"
                );
                configurations += 1;
                if !ran.contains(&object) {
                    oracle.run(&object, &context);
                    ran.insert(object);
                }
            }
        }
    }
    assert_eq!(configurations, 14);
}
