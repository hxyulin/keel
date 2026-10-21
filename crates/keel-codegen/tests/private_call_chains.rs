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
                .push((keel_codegen::regalloc::Loc::Reg(15), src));
        }
        alloc
    }
}

fn wrapper(name: &str, callee: &str) -> String {
    format!(
        "func @{name}(i64) -> i64 cc(fast) local {{\nblock0(v0:i64):\n v1 = call @{callee}(v0)\n return v1\n}}\n"
    )
}

fn fixture() -> String {
    let caller = SOURCE.split("func @caller").nth(1).unwrap();
    let mut source = format!("decl @external_noise(i64) -> i64\n{SOURCE}\n");
    // Caller before definitions, nested callees before and after their callers.
    for (name, callee) in [
        ("chain_caller", "outer"),
        ("unknown_caller", "unknown_outer"),
        ("recursive_caller", "recursive"),
        ("shuffle_caller", "shuffle_outer"),
    ] {
        source.push_str(&format!(
            "func @{name}{}\n",
            caller.replace("call @leaf(v3)", &format!("call @{callee}(v3)"))
        ));
    }
    source.push_str(&wrapper("outer", "middle"));
    source.push_str(&wrapper("unknown_outer", "unknown_middle"));
    source.push_str(&wrapper("middle", "leaf"));
    source.push_str(&wrapper("unknown_middle", "external_noise"));
    source.push_str("func @recursive(i64) -> i64 cc(fast) local {\nblock0(v0:i64):\n v1 = iconst.i64 0\n v2 = icmp.eq v0, v1\n brif v2, block1, block2\nblock1:\n return v1\nblock2:\n v3 = call @recursive(v1)\n return v3\n}\n");
    source.push_str("func @shuffle_outer(i64) -> i64 cc(fast) local {\nblock0(v0:i64):\n v1 = call @shuffle(v0,v0,v0,v0,v0,v0,v0,v0)\n return v1\n}\nfunc @shuffle(i64,i64,i64,i64,i64,i64,i64,i64) -> i64 cc(fast) local {\nblock0(v0:i64,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64,v7:i64):\n v8 = call @combine(v7,v6,v5,v4,v3,v2,v1,v0)\n return v8\n}\nfunc @combine(i64,i64,i64,i64,i64,i64,i64,i64) -> i64 cc(fast) local {\nblock0(v0:i64,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64,v7:i64):\n v8 = iadd v0,v1\n v9 = iadd v8,v2\n v10 = iadd v9,v3\n v11 = iadd v10,v4\n v12 = iadd v11,v5\n v13 = iadd v12,v6\n v14 = iadd v13,v7\n return v14\n}\n");
    source
}

#[test]
fn proven_chain_reduces_pressure_caller_stores_with_both_allocators() {
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let source = fixture();
            let m = keel_ir::parse(&source).unwrap();
            let old = keel_ir::parse(&source.replace("cc(fast)", "cc(c)")).unwrap();
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
            if let Some(dir) = std::env::var_os("KEEL_CALL_CHAIN_EVIDENCE_DIR") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir).unwrap();
                let stem = format!("{selector:?}-{allocator:?}");
                std::fs::write(dir.join(format!("{stem}-chain.s")), &a).unwrap();
                std::fs::write(dir.join(format!("{stem}-abi-control.s")), &b).unwrap();
            }
            let body = |s: String| {
                s.split("_chain_caller:\n")
                    .nth(1)
                    .unwrap()
                    .split("Lchain_caller_end:")
                    .next()
                    .unwrap()
                    .to_owned()
            };
            let (a, b) = (body(a), body(b));
            let stores = |s: &str| s.matches("stp\t").count() + s.matches("str\t").count();
            assert!(stores(&a) < stores(&b), "optimized:\n{a}\nABI:\n{b}");
        }
    }
}

#[test]
fn forward_references_custom_pools_and_chains_agree_on_both_target_object_routes() {
    let m = keel_ir::parse(&fixture()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let x86_text = keel_codegen::x86_64::compile_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            let x86_program = keel_codegen::x86_64::compile_program_with_pipeline(
                &m,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            assert_eq!(
                keel_asm::x86::assemble_object(&x86_text).unwrap(),
                keel_asm::x86::assemble_program_object(&x86_program).unwrap()
            );
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
                assert!(text.find("_caller:\n").unwrap() < text.find("_outer:\n").unwrap());
            }
        }
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_chain_pressure_unknown_clobbers_recursion_shuffles_and_transfers() {
    use std::{fs, process::Command};
    let m = keel_ir::parse(&fixture()).unwrap();
    let dir = std::env::var_os("KEEL_CALL_CHAIN_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-private-call-chains-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("fixture.keel"), fixture()).unwrap();
    fs::write(
        dir.join("oracle.c"),
        include_str!("private_leaf_calls/chain_oracle.c"),
    )
    .unwrap();
    let mut configurations = 0;
    let mut reversed = m.clone();
    reversed.functions.reverse();
    for m in [&m, &reversed] {
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
                    let text =
                        keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                    let program =
                        keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                            .unwrap();
                    for object in [
                        keel_asm::assemble_object(&text).unwrap(),
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                    ] {
                        configurations += 1;
                        fs::write(dir.join(format!("configuration-{configurations}.s")), &text)
                            .unwrap();
                        fs::write(
                            dir.join(format!("configuration-{configurations}.o")),
                            &object,
                        )
                        .unwrap();
                        fs::write(dir.join("calls.o"), object).unwrap();
                        let build = Command::new("clang")
                            .arg("-O2")
                            .arg(dir.join("oracle.c"))
                            .arg(dir.join("calls.o"))
                            .arg("-o")
                            .arg(dir.join("oracle"))
                            .output()
                            .unwrap();
                        assert!(
                            build.status.success(),
                            "{}",
                            String::from_utf8_lossy(&build.stderr)
                        );
                        let run = Command::new(dir.join("oracle")).output().unwrap();
                        assert!(
                            run.status.success(),
                            "{}",
                            String::from_utf8_lossy(&run.stderr)
                        );
                    }
                }
            }
        }
    }
    assert_eq!(configurations, 56);
}
