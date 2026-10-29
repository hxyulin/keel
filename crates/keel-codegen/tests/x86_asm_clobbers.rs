use keel_codegen::{
    CodegenPipeline,
    regalloc::{Pools, RegisterAllocator},
};
use keel_codegen::{SelectionTier, regalloc::AllocationTier};

const SOURCE: &str = r#"
func @plain(i64) -> i64 {
entry(v0:i64):
 asm "nop", "~{memory},~{cc}"()
 v1 = iconst.i64 7
 v2 = iadd v0, v1
 return v2
}
func @clobber(i64, i64) -> i64 {
entry(v0:i64, v1:i64):
 asm "xorq %%rcx, %%rcx; xorq %%rbx, %%rbx; xorq %%r12, %%r12", "~{rcx},~{rbx},~{r12},~{cc}"()
 v2 = iadd v0, v1
 return v2
}
func @tied_live(i64, i64) -> i64 {
entry(v0:i64, v1:i64):
 v2 = asm "addq %q2, %q0", "=r,0,r,~{cc}"(v0, v1) -> i64
 v3 = iadd v0, v1
 v4 = iadd v2, v3
 return v4
}
func @memory(ptr, i64) -> i64 {
entry(v0:ptr, v1:i64):
 asm "addq %q1, %0", "mreadwrite,r,~{cc}"(v0, v1)
 v2 = load.i64 v0
 v3 = iadd v1, v2
 return v3
}
func @equal(i64, i64) -> i64 {
entry(v0:i64, v1:i64):
 v2 = asm "cmpq %q1, %q2", "=@ccz,r,r,~{cc}"(v0, v1) -> i8
 v3 = uext.i64 v2
 v4 = iadd v0, v1
 v5 = iadd v3, v4
 return v5
}
func @constant_live(i64) -> i64 {
entry(v0:i64):
 v1 = iconst.i64 1234605616436508552
 v2 = asm "addq %q2, %q0", "=r,0,r,~{cc}"(v0, v1) -> i64
 v3 = iadd v1, v2
 return v3
}
"#;

#[test]
fn selective_preservation_keeps_transport_parity_and_exports_runtime_cases() {
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (bytes, text) = compile(selector, &allocator, spilled);
                let plain = text
                    .split_once("\"plain\":\n")
                    .unwrap()
                    .1
                    .split_once(".Lplain_end:")
                    .unwrap()
                    .0;
                assert!(plain.contains("nop"), "{plain}");
                assert!(
                    !plain.contains("push r12") && !plain.contains("push rbx"),
                    "operand-free asm must not force callee saves: {plain}"
                );
                if !spilled {
                    assert!(
                        !plain.contains("[rbp -"),
                        "operand-free asm must not allocate staging: {plain}"
                    );
                }
                let clobber = text
                    .split_once("\"clobber\":\n")
                    .unwrap()
                    .1
                    .split_once(".Lclobber_end:")
                    .unwrap()
                    .0;
                assert!(
                    !clobber.contains("push r13")
                        && !clobber.contains("push r14")
                        && !clobber.contains("push r15"),
                    "undeclared callee saves: {clobber}"
                );
                if let Some(dir) = std::env::var_os("KEEL_ASM_CLOBBER_EXPORT_DIR") {
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(
                        std::path::Path::new(&dir)
                            .join(format!("{selector:?}-{allocator:?}-{spilled}.o")),
                        bytes,
                    )
                    .unwrap();
                }
            }
        }
    }
}

struct RematConstants;
impl RegisterAllocator for RematConstants {
    fn name(&self) -> &'static str {
        "asm-clobber-remat"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_ir::EntityRef;
        let mut allocation = AllocationTier::Fast.allocate(input);
        for (_, inst) in input.func.layout_insts() {
            if matches!(input.func.inst_data(inst), keel_ir::InstData::Iconst { .. }) {
                for &value in input.func.inst_results(inst) {
                    allocation.locs[0][value.index()] =
                        Some(keel_codegen::regalloc::Loc::Remat(value));
                }
            }
        }
        allocation
    }
}

struct CalleeHomes;
impl RegisterAllocator for CalleeHomes {
    fn name(&self) -> &'static str {
        "asm-clobber-callee-homes"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        let pools = Pools {
            caller_saved: &[],
            callee_saved: &[3, 12],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        AllocationTier::Fast.allocate(&keel_codegen::regalloc::AllocationInput {
            pools: &pools,
            ..*input
        })
    }
}

#[test]
fn staging_preserves_rematerialized_inputs_and_live_clobbered_homes() {
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [&RematConstants as &dyn RegisterAllocator, &CalleeHomes] {
            let (bytes, _) = compile(selector, allocator, false);
            if let Some(dir) = std::env::var_os("KEEL_ASM_CLOBBER_EXPORT_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("{selector:?}-{}.o", allocator.name())),
                    bytes,
                )
                .unwrap();
            }
        }
    }
}

fn compile(
    selector: SelectionTier,
    allocator: &dyn RegisterAllocator,
    spilled: bool,
) -> (Vec<u8>, String) {
    compile_source(SOURCE, selector, allocator, spilled, Default::default())
}

fn compile_source(
    source: &str,
    selector: SelectionTier,
    allocator: &dyn RegisterAllocator,
    spilled: bool,
    features: keel_asm_ir::x86::Features,
) -> (Vec<u8>, String) {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    if selector == SelectionTier::Full {
        keel_opt::optimize(&mut module).unwrap();
    }
    let selector = keel_codegen::x86_features::FeatureSelector {
        inner: &selector,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator,
    };
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let pools = if spilled {
        &empty
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    let bytes = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        bytes,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
        &module,
        pools,
        &pipeline,
        &mut |p| chunks.append(p),
    )
    .unwrap();
    assert_eq!(
        bytes,
        keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap()
    );
    (bytes, text)
}

fn simd_source() -> String {
    let mut source = String::new();
    for (name, ty, instruction) in [
        ("simd_f32", "f32", "addss %1,%0"),
        ("simd_f64", "f64", "addsd %1,%0"),
        ("simd_64", "i32x2", "paddd %1,%0"),
        ("simd_128", "i32x4", "paddd %1,%0"),
        ("simd_256", "i32x8", "vpaddd %1,%0,%0"),
        ("simd_barrier", "i32x4", "/* opaque %0 */"),
    ] {
        source += &format!(
            "func @{name}(ptr,ptr,ptr) {{\nentry(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{ty} align 1 v0\n v4 = asm \"{instruction}\", \"=w,0,~{{xmm4}},~{{memory}},~{{cc}}\"(v3) -> {ty}\n store align 1 v4,v1\n store align 1 v3,v2\n return\n}}\n"
        );
    }
    source += "func @simd_clobber(ptr,ptr,ptr) {\nentry(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i32x8 align 1 v0\n asm \"vpxor %%ymm4,%%ymm4,%%ymm4\", \"~{ymm4},~{memory}\"()\n store align 1 v3,v1\n store align 1 v3,v2\n return\n}\n";
    source += "func @simd_mixed(ptr,ptr,ptr) {\nentry(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.f32 align 1 v0\n v4 = load.f64 align 1 v0\n v5 = load.i32x8 align 1 v0\n v6 = load.i64 align 1 v0\n v7,v8,v9,v10 = asm \"/*mixed %0 %1 %2 %3*/\", \"=w,=w,=w,=r,0,1,2,3,~{memory}\"(v3,v4,v5,v6) -> (f32,f64,i32x8,i64)\n store align 1 v7,v1\n store align 1 v3,v2\n v11 = iconst.i64 8\n v12 = ptradd v1,v11\n v13 = ptradd v2,v11\n store align 1 v8,v12\n store align 1 v4,v13\n v14 = iconst.i64 16\n v15 = ptradd v1,v14\n v16 = ptradd v2,v14\n store align 1 v9,v15\n store align 1 v5,v16\n v17 = iconst.i64 48\n v18 = ptradd v1,v17\n v19 = ptradd v2,v17\n store align 1 v10,v18\n store align 1 v6,v19\n return\n}\n";
    source
}

struct SimdClobberHome;
impl RegisterAllocator for SimdClobberHome {
    fn name(&self) -> &'static str {
        "simd-clobbered-home"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        let pools = Pools {
            float_caller_saved: if input.pools.float_caller_saved.is_empty() {
                &[]
            } else {
                &[4]
            },
            ..*input.pools
        };
        AllocationTier::Full.allocate(&keel_codegen::regalloc::AllocationInput {
            pools: &pools,
            ..*input
        })
    }
}

#[test]
fn simd_operands_preserve_live_values_spills_and_upper_halves() {
    use keel_codegen::x86_features::{CpuLevel, Features};
    for selector in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocator in [
            &AllocationTier::Fast as &dyn RegisterAllocator,
            &AllocationTier::Full,
            &SimdClobberHome,
        ] {
            for spilled in [false, true] {
                let (object, text) = compile_source(
                    &simd_source(),
                    selector,
                    allocator,
                    spilled,
                    Features::for_level(CpuLevel::V3),
                );
                assert!(text.contains("opaque %xmm"), "{text}");
                assert!(text.contains("vpaddd %ymm"), "{text}");
                if let Some(dir) = std::env::var_os("KEEL_ASM_SIMD_EXPORT_DIR") {
                    let dir = std::path::Path::new(&dir);
                    std::fs::create_dir_all(dir).unwrap();
                    std::fs::write(
                        dir.join(format!("{selector:?}-{}-{spilled}.o", allocator.name())),
                        object,
                    )
                    .unwrap();
                    std::fs::write(
                        dir.join("runner.c"),
                        include_str!("x86_asm_clobbers/simd_runner.c"),
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[test]
fn simd_register_constraints_reject_overlap_mismatched_ties_and_bad_views() {
    let module = |constraints: &str, template: &str, output: &str| {
        keel_ir::parse(&format!("func @f(i32x4) -> {output} {{\nentry(v0:i32x4):\n v1 = asm \"{template}\", \"{constraints}\"(v0) -> {output}\n return v1\n}}" )).unwrap()
    };
    for (constraints, template, message) in [
        ("=&{xmm0},{xmm0}", "", "overlaps"),
        ("={xmm0},0,~{ymm0}", "", "overlaps"),
        ("={xmm16},0", "", "reserved"),
        ("=w,0", "%q0", "SIMD register modifier"),
        ("=w,0", "%t0", "256-bit operand"),
    ] {
        let m = module(constraints, template, "i32x4");
        keel_ir::verify_module(&m).unwrap();
        let error = keel_codegen::x86_64::compile(&m).unwrap_err();
        assert!(
            error.to_string().contains(message),
            "{constraints}: {error}"
        );
    }
    for constraints in ["={r16},0", "=r,0,~{r16}"] {
        let m = keel_ir::parse(&format!("func @g(i64) -> i64 {{\nentry(v0:i64):\n v1 = asm \"\", \"{constraints}\"(v0) -> i64\n return v1\n}}")).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(
            keel_codegen::x86_64::compile(&m)
                .unwrap_err()
                .to_string()
                .contains("reserved")
        );
    }
    let m = module("=w,0", "", "f32x4");
    assert!(keel_ir::verify_module(&m).is_err());
    let m = module("=&{xmm0},0", "", "i32x4");
    keel_ir::verify_module(&m).unwrap();
    keel_codegen::x86_64::compile(&m).unwrap();
    let m = keel_ir::parse(&simd_source()).unwrap();
    assert!(
        keel_codegen::x86_64::compile(&m).is_err(),
        "256-bit operands require AVX"
    );
}

#[test]
fn unique_operand_encodes_the_module_function_index() {
    // Only the later functions have inline assembly, at the same instruction
    // index, so their `%=` values must differ by module position alone.
    let source = r##"
func @none(i64) -> i64 {
entry(v0:i64):
 return v0
}
func @first() {
entry:
 asm "# id %=", ""()
 return
}
func @second() {
entry:
 asm "# id %=", ""()
 return
}
"##;
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = keel_codegen::compile(&module, keel_codegen::Target::X86_64LinuxGnu).unwrap();
    for index in [1u64, 2] {
        assert!(text.contains(&format!("# id {}\n", index << 32)), "{text}");
    }
}
