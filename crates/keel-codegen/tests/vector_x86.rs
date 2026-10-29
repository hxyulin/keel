use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
#[path = "common/vector.rs"]
pub mod vector_common;
use vector_common::{command, fixture, harness, temp};
fn abi_fixture() -> (String, String) {
    vector_common::abi_fixture(false)
}

const TINY: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3],
    float_caller_saved: &[0, 1],
    float_callee_saved: &[],
};
const SPILLED: Pools = Pools {
    float_caller_saved: &[],
    ..TINY
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn host_supports_level(level: CpuLevel) -> bool {
    use std::arch::x86_64::{__cpuid, __cpuid_count};
    // AVX detection below includes OSXSAVE/XCR0, not merely CPUID.AVX.
    let (basic, extended, leaf7) =
        unsafe { (__cpuid(1), __cpuid(0x80000001), __cpuid_count(7, 0)) };
    let v2_bits = (1 << 0) | (1 << 9) | (1 << 13) | (1 << 19) | (1 << 20) | (1 << 23);
    let v2 = basic.ecx & v2_bits == v2_bits && extended.ecx & 1 != 0;
    match level {
        CpuLevel::V1 => true,
        CpuLevel::V2 => v2,
        CpuLevel::V3 => {
            let basic_bits = (1 << 12) | (1 << 22) | (1 << 26) | (1 << 27) | (1 << 28) | (1 << 29);
            let leaf7_bits = (1 << 3) | (1 << 5) | (1 << 8);
            v2 && basic.ecx & basic_bits == basic_bits
                && leaf7.ebx & leaf7_bits == leaf7_bits
                && extended.ecx & (1 << 5) != 0
                && std::is_x86_feature_detected!("avx")
        }
    }
}
fn compile(
    source: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
    tiny: bool,
    features: Features,
) -> (String, keel_asm_ir::Program) {
    let pools = if tiny {
        &TINY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    compile_with_pools(source, selection, allocation, pools, features)
}
fn compile_with_pools(
    source: &str,
    selection: SelectionTier,
    allocation: AllocationTier,
    pools: &Pools,
    features: Features,
) -> (String, keel_asm_ir::Program) {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if matches!(selection, SelectionTier::Full) {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &selection,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &allocation,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let typed = keel_asm::x86::assemble_program_object_with_features(&program, features)
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
    let parsed = keel_asm::x86::assemble_object_with_features(&text, features)
        .unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(typed, parsed);
    (text, program)
}
#[test]
fn all_vector_operations_emit_zero_text_and_identical_typed_objects() {
    let source = fixture();
    // The V3 row also carries Q-width call transport and va_arg under VEX.
    let mut v3 = source.clone();
    for shape in keel_ir::VectorType::ALL_128 {
        v3 += &format!(
            "decl @external_{shape}({shape})->{shape}\nfunc @transport_{shape}(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n v2 = load.{shape} align 1 v0\n v3 = call @external_{shape}(v2)\n store align 1 v2,v1\n store align 1 v3,v0\n return\n}}\n"
        );
        v3 += &format!(
            "func @var_{shape}(i64,...)->{shape} {{\nss0 = stack_slot 24, align 8\nentry(v0:i64):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n v2 = va_arg.{shape} v1\n return v2\n}}\n"
        );
    }
    let v1 = Features::default();
    for (s, a, t, features, source) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Full,
            false,
            v1,
            &source,
        ),
        (SelectionTier::Full, AllocationTier::Full, true, v1, &source),
        (SelectionTier::Full, AllocationTier::Fast, true, v1, &source),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            true,
            Features::for_level(CpuLevel::V3),
            &v3,
        ),
    ] {
        let (text, _) = compile(source, s, a, t, features);
        for opcode in [
            "paddb", "paddw", "paddd", "paddq", "psubb", "psubq", "pmullw", "addps", "addpd",
            "sqrtps", "sqrtpd", "pcmpeqd", "pcmpgtd", "pshufd", "andnps",
        ] {
            assert!(text.contains(opcode), "missing {opcode}");
        }
    }
}

#[test]
fn strict_v1_vector_corpus_emits_without_post_sse2_requirements() {
    let source = fixture();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for tiny in [false, true] {
                compile(
                    &source,
                    selection,
                    allocation,
                    tiny,
                    Features::for_level(CpuLevel::V1),
                );
            }
        }
    }
}

#[test]
fn vector_runtime_reference_harness_compiles() {
    let dir = temp("syntax");
    let c = dir.join("test.c");
    fs::write(&c, harness()).unwrap();
    command(
        Command::new("clang")
            .args(["-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg(&c),
    );
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn vector_operations_execute_unaligned_edges_masks_and_spills() {
    let dir = temp("runtime");
    let c = dir.join("test.c");
    fs::write(&c, harness()).unwrap();
    let source = fixture();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for tiny in [false, true] {
                let mut features = vec![Features::for_level(CpuLevel::V1), Features::default()];
                if host_supports_level(CpuLevel::V2) {
                    features.push(Features::for_level(CpuLevel::V2));
                }
                if std::is_x86_feature_detected!("avx") {
                    features.push(Features {
                        xsave: true,
                        osxsave: true,
                        avx: true,
                        ..Features::default()
                    });
                    if host_supports_level(CpuLevel::V3) {
                        features.push(Features::all_known());
                    }
                }
                for features in features {
                    println!(
                        "vector runtime: selection={selection:?} allocation={allocation:?} tiny={tiny} features={features:?}"
                    );
                    let (_, program) = compile(&source, selection, allocation, tiny, features);
                    let object = dir.join("test.o");
                    fs::write(
                        &object,
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap(),
                    )
                    .unwrap();
                    let exe = dir.join("test");
                    command(
                        Command::new("clang")
                            .args(["-O2", "-ffp-contract=off"])
                            .arg(&c)
                            .arg(&object)
                            .arg("-lm")
                            .arg("-o")
                            .arg(&exe),
                    );
                    command(&mut Command::new(exe));
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn vector_abi_fixture_emits_and_reference_harness_cross_compiles() {
    let (source, harness) = abi_fixture();
    let (_, program) = compile(
        &(fixture() + &source),
        SelectionTier::Full,
        AllocationTier::Full,
        true,
        Features::default(),
    );
    assert_eq!(program.stats().text_bytes, 0);
    let dir = temp("abi-syntax");
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-fsyntax-only",
            ])
            .arg(&c),
    );
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn vector_sysv_arguments_results_calls_tail_calls_varargs_cycles_and_pressure_execute() {
    let (extra, harness) = abi_fixture();
    let source = fixture() + &extra;
    let dir = temp("abi-runtime");
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            let feature_sets = if std::is_x86_feature_detected!("avx") {
                vec![
                    Features::default(),
                    Features {
                        xsave: true,
                        osxsave: true,
                        avx: true,
                        ..Features::default()
                    },
                ]
            } else {
                vec![Features::default()]
            };
            for features in feature_sets {
                println!(
                    "vector ABI runtime: allocation={allocation:?} tiny={tiny} features={features:?}"
                );
                let (_, program) =
                    compile(&source, SelectionTier::Full, allocation, tiny, features);
                let object = dir.join("test.o");
                fs::write(
                    &object,
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap(),
                )
                .unwrap();
                let exe = dir.join("test");
                command(
                    Command::new("clang")
                        .args(["-O2", "-ffp-contract=off"])
                        .arg(&c)
                        .arg(&object)
                        .arg("-o")
                        .arg(&exe),
                );
                command(&mut Command::new(exe));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn packed_avx_selection_preserves_sources_and_removes_two_address_copies() {
    use keel_asm_ir::{
        Item,
        x86::{Opcode, Operand},
    };
    let features = Features {
        xsave: true,
        osxsave: true,
        avx: true,
        ..Features::default()
    };
    let mut source = String::new();
    for shape in ["f32x4", "f64x2"] {
        for op in ["add", "sub", "mul", "div"] {
            // Return register aliases rhs, exercising the SSE preservation path.
            source += &format!(
                "func @{op}_{shape}({shape}, {shape}) -> {shape} {{\nblock0(v0: {shape}, v1: {shape}):\n v2 = vf{op} v1, v0\n return v2\n}}\n"
            );
        }
    }
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        let (baseline, bp) = compile(
            &source,
            SelectionTier::Baseline,
            allocation,
            false,
            Features::default(),
        );
        let (enabled, ep) = compile(
            &source,
            SelectionTier::Baseline,
            allocation,
            false,
            features,
        );
        let instructions = |p: &keel_asm_ir::Program| {
            p.items
                .iter()
                .filter_map(|item| match item {
                    Item::X86Instruction(i) => Some(i.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let bi = instructions(&bp);
        let ei = instructions(&ep);
        let avx_ops = [
            Opcode::Vaddps,
            Opcode::Vsubps,
            Opcode::Vmulps,
            Opcode::Vdivps,
            Opcode::Vaddpd,
            Opcode::Vsubpd,
            Opcode::Vmulpd,
            Opcode::Vdivpd,
        ];
        for op in avx_ops {
            let ops: Vec<_> = ei.iter().filter(|i| i.opcode == op).collect();
            assert_eq!(ops.len(), 1, "{enabled}");
            assert_eq!(ops[0].operands.len(), 3);
            // The ABI puts lhs v1 in xmm1 and rhs v0 in xmm0.
            for (operand, number) in [(1, 1), (2, 0)] {
                assert!(
                    matches!(&ops[0].operands[operand], Operand::Register(r) if r.number == number),
                    "{enabled}"
                );
            }
        }
        assert!(
            !bi.iter().any(|i| avx_ops.contains(&i.opcode)),
            "{baseline}"
        );
        assert!(
            ei.iter().filter(|i| i.opcode == Opcode::Movaps).count()
                < bi.iter().filter(|i| i.opcode == Opcode::Movaps).count(),
            "baseline:\n{baseline}AVX:\n{enabled}"
        );
        let bytes = |p: &keel_asm_ir::Program, f| {
            use object::{Object, ObjectSection};
            let obj = keel_asm::x86::assemble_program_object_with_features(p, f).unwrap();
            object::File::parse(obj.as_slice())
                .unwrap()
                .section_by_name(".text")
                .unwrap()
                .data()
                .unwrap()
                .len()
        };
        let baseline_bytes = bytes(&bp, Features::default());
        let avx_bytes = bytes(&ep, features);
        println!(
            "packed shape {allocation:?}: baseline {baseline_bytes} bytes/{} movaps; AVX {avx_bytes} bytes/{} movaps",
            bi.iter().filter(|i| i.opcode == Opcode::Movaps).count(),
            ei.iter().filter(|i| i.opcode == Opcode::Movaps).count()
        );
        assert!(avx_bytes < baseline_bytes);
    }
}

// Keep all eighteen inputs live over an opaque call, then exercise each packed
// operation with the reloaded values. Two-register pools also force scratch use.
fn packed_pressure_fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n#include <math.h>\n");
    let mut main = String::from("int main(void) {\n");
    for (shape, ty, values) in [
        (
            "f32x4",
            "float",
            "0.0f,-0.0f,1.0f,-1.0f,1.5f,-2.5f,INFINITY,-INFINITY,NAN,0x1p-126f,0x1p-149f,0x1.fffffep127f,0.5f,3.25f",
        ),
        (
            "f64x2",
            "double",
            "0.0,-0.0,1.0,-1.0,1.5,-2.5,INFINITY,-INFINITY,NAN,0x1p-1022,0x1p-1074,0x1.fffffffffffffp1023,0.5,3.25",
        ),
    ] {
        ir += &format!("decl @pressure_external_{shape}({shape}) -> {shape}\n");
        c += &format!(
            "typedef {ty} V_{shape} __attribute__((vector_size(16)));\n__attribute__((noinline)) V_{shape} pressure_external_{shape}(V_{shape} x) {{return x+x;}}\n"
        );
        let lanes = if ty == "float" { 4 } else { 2 };
        for (op, symbol) in [("add", "+"), ("sub", "-"), ("mul", "*"), ("div", "/")] {
            let name = format!("packed_pressure_{op}_{shape}");
            ir += &format!("func @{name}(ptr, ptr) {{\nblock0(v0: ptr, v1: ptr):\n");
            for j in 0..18 {
                ir += &format!(
                    " v{} = iconst.i64 {}\n v{} = ptradd v0, v{}\n v{} = load.{shape} align 1 v{}\n",
                    2 + 3 * j,
                    16 * j,
                    3 + 3 * j,
                    2 + 3 * j,
                    4 + 3 * j,
                    3 + 3 * j
                );
            }
            ir += &format!(" v60 = call @pressure_external_{shape}(v4)\n");
            for j in 0..18 {
                ir += &format!(" v{} = vf{op} v{}, v{}\n", 61 + j, 60 + j, 4 + 3 * j);
            }
            ir += " store align 1 v78, v1\n return\n}\n";
            c += &format!("extern void {name}(const void*, void*);\n");
            main += &format!(
                "{{ {ty} values[]={{ {values} }}, in[18][{lanes}], out[{lanes}];\nfor(unsigned k=0;k<28;k++) {{for(unsigned j=0;j<18;j++) for(unsigned lane=0;lane<{lanes};lane++) {{ {ty} finite[]={{1.0,-1.0,1.5,-2.5,0.5,3.25}}; in[j][lane]=finite[(k+j+lane)%6]; if(k>=14 && j==k%18) in[j][lane]=values[(k-14+lane)%14]; }}\n{name}(in,out);for(unsigned lane=0;lane<{lanes};lane++) {{ {ty} expected=in[0][lane]+in[0][lane];for(unsigned j=0;j<18;j++) expected=expected {symbol} in[j][lane];\nif(!((isnan(out[lane])&&isnan(expected))||(out[lane]==expected&&(out[lane]!=0||!!signbit(out[lane])==!!signbit(expected)))))return 1; }} }} }}\n"
            );
        }
    }
    main += "return 0; }\n";
    (ir, c + &main)
}

#[test]
fn packed_pressure_fixture_emits_with_both_allocators() {
    let (source, harness) = packed_pressure_fixture();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for avx in [false, true] {
            let (text, _) = compile(
                &source,
                SelectionTier::Baseline,
                allocation,
                true,
                Features {
                    avx,
                    xsave: avx,
                    osxsave: avx,
                    ..Features::default()
                },
            );
            assert!(text.contains(if avx { "vdivpd" } else { "divpd" }));
        }
    }
    let dir = temp("packed-pressure-syntax");
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    command(
        Command::new("clang")
            .args(["-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg(c),
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn packed_arithmetic_edges_execute_across_calls_and_forced_spills() {
    if !std::is_x86_feature_detected!("avx") {
        return;
    }
    let (source, harness) = packed_pressure_fixture();
    let dir = temp("packed-pressure-runtime");
    let c = dir.join("test.c");
    fs::write(&c, harness).unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            for avx in [false, true] {
                println!("packed pressure: allocation={allocation:?} tiny={tiny} avx={avx}");
                let features = Features {
                    avx,
                    xsave: avx,
                    osxsave: avx,
                    ..Features::default()
                };
                let (_, program) =
                    compile(&source, SelectionTier::Baseline, allocation, tiny, features);
                let object = dir.join("test.o");
                fs::write(
                    &object,
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap(),
                )
                .unwrap();
                let exe = dir.join("test");
                command(
                    Command::new("clang")
                        .args(["-O2", "-ffp-contract=off"])
                        .arg(&c)
                        .arg(object)
                        .arg("-o")
                        .arg(&exe),
                );
                command(&mut Command::new(exe));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[path = "common/x86_packed.rs"]
mod x86_packed;

#[test]
fn packed_integer_compare_oracle_emits_both_object_routes() {
    let (source, harness) = x86_packed::fixture();
    let evidence = std::env::var_os("KEEL_X86_PACKED_EVIDENCE").map(std::path::PathBuf::from);
    if let Some(dir) = &evidence {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("oracle.c"), &harness).unwrap();
        fs::write(dir.join("oracle.keel"), &source).unwrap();
    }
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for tiny in [false, true] {
            let (text, program) = compile(
                &source,
                SelectionTier::Baseline,
                allocation,
                tiny,
                Features::default(),
            );
            if let Some(dir) = &evidence {
                let stem = format!("{allocation:?}-tiny-{tiny}");
                fs::write(dir.join(format!("{stem}.s")), &text).unwrap();
                fs::write(
                    dir.join(format!("{stem}.o")),
                    keel_asm::x86::assemble_program_object_with_features(
                        &program,
                        Features::default(),
                    )
                    .unwrap(),
                )
                .unwrap();
            }
        }
    }
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        let (text, program) = compile_with_pools(
            &source,
            SelectionTier::Baseline,
            allocation,
            &SPILLED,
            Features::default(),
        );
        assert!(text.contains("xmmword ptr [rbp"));
        if let Some(dir) = &evidence {
            let stem = format!("{allocation:?}-spilled");
            fs::write(dir.join(format!("{stem}.s")), text).unwrap();
            fs::write(
                dir.join(format!("{stem}.o")),
                keel_asm::x86::assemble_program_object_with_features(&program, Features::default())
                    .unwrap(),
            )
            .unwrap();
        }
    }
    let dir = temp("packed-compare-syntax");
    let c = dir.join("oracle.c");
    fs::write(&c, harness).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-fsyntax-only",
            ])
            .arg(&c),
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn packed_integer_compare_boundaries_aliases_live_inputs_and_spills_execute() {
    let (source, harness) = x86_packed::fixture();
    let dir = temp("packed-compare-runtime");
    let c = dir.join("oracle.c");
    fs::write(&c, harness).unwrap();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool_name, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("tiny", &TINY),
                ("spilled", &SPILLED),
            ] {
                println!("packed compare: {selection:?} {allocation:?} pool={pool_name}");
                let (_, program) =
                    compile_with_pools(&source, selection, allocation, pools, Features::default());
                let object = dir.join("oracle.o");
                fs::write(
                    &object,
                    keel_asm::x86::assemble_program_object_with_features(
                        &program,
                        Features::default(),
                    )
                    .unwrap(),
                )
                .unwrap();
                let exe = dir.join("oracle");
                command(
                    Command::new("clang")
                        .args(["-O2", "-msse2", "-mno-avx"])
                        .arg(&c)
                        .arg(&object)
                        .arg("-o")
                        .arg(&exe),
                );
                command(&mut Command::new(exe));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn packed_compare_and_splat_recipe_encodings_match_clang_sse2() {
    use object::{Object, ObjectSection};
    let (source, _) = x86_packed::fixture();
    let (text, _) = compile(
        &source,
        SelectionTier::Baseline,
        AllocationTier::Full,
        true,
        Features::default(),
    );
    let mut assembly = String::from(".intel_syntax noprefix\n.text\n");
    let mut comparison = false;
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            comparison = line.contains(" = vicmp.") || line.contains(" = vsplat.");
        } else if comparison && !line.trim().is_empty() {
            assembly += line;
            assembly += "\n";
        }
    }
    assert!(assembly.contains("psrlq"));
    assert!(assembly.contains("pcmpgtd"));
    let dir = temp("packed-encoding");
    let input = dir.join("recipes.s");
    let output = dir.join("clang.o");
    fs::write(&input, &assembly).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-msse2",
                "-mno-avx",
                "-c",
            ])
            .arg(&input)
            .arg("-o")
            .arg(&output),
    );
    let ours =
        keel_asm::x86::assemble_object_with_features(&assembly, Features::default()).unwrap();
    let clang = fs::read(&output).unwrap();
    let text_bytes = |bytes: &[u8]| {
        object::File::parse(bytes)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    assert_eq!(text_bytes(&ours), text_bytes(&clang));
    if let Some(evidence) = std::env::var_os("KEEL_X86_PACKED_EVIDENCE") {
        let evidence = std::path::PathBuf::from(evidence);
        fs::create_dir_all(&evidence).unwrap();
        fs::write(evidence.join("recipes.s"), assembly).unwrap();
        fs::write(evidence.join("recipes-keel.o"), ours).unwrap();
        fs::write(evidence.join("recipes-clang.o"), clang).unwrap();
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn packed_compare_coalesces_result_without_staging_register_rhs() {
    for shape in ["i8x16", "i16x8", "i32x4", "i64x2"] {
        for cc in ["eq", "ne", "sgt", "sle"] {
            let source = format!(
                "func @cmp({shape}, {shape}) -> {shape} {{\nblock0(v0: {shape}, v1: {shape}):\n v2 = vicmp.{cc} v0, v1\n return v2\n}}\n"
            );
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                let (text, _) = compile(
                    &source,
                    SelectionTier::Baseline,
                    allocation,
                    false,
                    Features::default(),
                );
                let copies = text
                    .lines()
                    .filter(|l| l.trim_start().starts_with("movaps"))
                    .count();
                // The signed i64 recipe needs a mutable copy of rhs and a
                // temporary copy of lhs; all other compares need no copies.
                assert_eq!(
                    copies,
                    if shape == "i64x2" && matches!(cc, "sgt" | "sle") {
                        2
                    } else {
                        0
                    },
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn packed_splats_use_sse2_without_stack_staging() {
    for (shape, lane) in [
        ("i8x16", "i8"),
        ("i16x8", "i16"),
        ("i32x4", "i32"),
        ("i64x2", "i64"),
        ("f32x4", "f32"),
        ("f64x2", "f64"),
    ] {
        let source = format!(
            "func @splat({lane}) -> {shape} {{\nblock0(v0: {lane}):\n v1 = vsplat.{shape} v0\n return v1\n}}\n"
        );
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let (text, _) = compile(
                &source,
                SelectionTier::Baseline,
                allocation,
                true,
                Features::default(),
            );
            assert!(!text.contains("[rbp") && !text.contains("[rsp"), "{text}");
            assert!(
                !text.contains("vbroadcast") && !text.contains("vpbroadcast"),
                "{text}"
            );
            assert!(
                text.contains(if shape == "i64x2" {
                    "punpcklqdq"
                } else {
                    "pshufd"
                }),
                "{text}"
            );
        }
    }
}

#[test]
fn interleaved_dwords_and_v1_multiply_use_packed_recipes_with_live_inputs() {
    let source = "func @shuffle(ptr,ptr,ptr) {
block0(v0:ptr,v1:ptr,v2:ptr):
 v3 = load.i32x4 align 1 v0
 v4 = load.i32x4 align 1 v1
 v5 = vshuffle v3,v4,[3,4,1,6]
 store align 1 v5,v2
 v6 = iconst.i64 16
 v7 = ptradd v2,v6
 store align 1 v3,v7
 v8 = iconst.i64 32
 v9 = ptradd v2,v8
 store align 1 v4,v9
 return
}
func @multiply(ptr,ptr,ptr) {
block0(v0:ptr,v1:ptr,v2:ptr):
 v3 = load.i32x4 align 1 v0
 v4 = load.i32x4 align 1 v1
 v5 = vimul v3,v4
 store align 1 v5,v2
 v6 = iconst.i64 16
 v7 = ptradd v2,v6
 store align 1 v3,v7
 v8 = iconst.i64 32
 v9 = ptradd v2,v8
 store align 1 v4,v9
 return
}";
    for pools in [&keel_codegen::x86_64::REGISTERS, &SPILLED] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let (text, _) = compile_with_pools(
                source,
                SelectionTier::Baseline,
                allocation,
                pools,
                Features::for_level(CpuLevel::V1),
            );
            assert_eq!(text.matches("pmuludq\t").count(), 2, "{text}");
            assert_eq!(text.matches("punpckldq\t").count(), 2, "{text}");
            assert!(!text.contains("imul\t"), "{text}");
            assert!(!text.contains("pmulld\t"), "{text}");
            assert!(!text.contains("stack_addr"), "lane legalization: {text}");
        }
    }
}
