//! Bounded disp32 selection for exact ordinary i32x4 loads and closed m128 widening.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionOptions, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::EntityRef;
use std::{fs, path::PathBuf};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const OFFSETS: [i64; 13] = [
    0,
    1,
    -17,
    127,
    -128,
    128,
    -129,
    i32::MIN as i64,
    i32::MAX as i64,
    i32::MIN as i64 - 1,
    i32::MAX as i64 + 1,
    i64::MIN,
    i64::MAX,
];
fn source(offset: i64, wide: bool, mode: usize) -> String {
    let convert = if wide {
        " v5 = swiden_low.i64x4 v4\n"
    } else {
        ""
    };
    let value = if wide { "v5" } else { "v4" };
    let call = if mode == 2 { " call @clobber()\n" } else { "" };
    let before_load = if mode == 4 { " call @clobber()\n" } else { "" };
    let extra = if mode == 1 {
        " v6 = load.i32x4 align 1 v3\n store align 1 v6,v1\n"
    } else if mode == 3 {
        " store align 1 v4,v1\n"
    } else {
        ""
    };
    format!(
        "func @f(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = iconst.i64 {offset}\n v3 = ptradd v0,v2\n{before_load} v4 = load.i32x4 align 1 v3\n{convert}{call} store align 1 {value},v1\n{extra} return\n}}\n"
    )
}
fn dir() -> PathBuf {
    std::env::var_os("KEEL_ADDRESS_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "keel-constant-vector-address-{}",
                std::process::id()
            ))
        })
}
fn compile(
    module: &keel_ir::Module,
    s: SelectionTier,
    a: AllocationTier,
    spilled: bool,
    features: Features,
) -> (String, Vec<u8>) {
    let selector = FeatureSelector {
        inner: &s,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let pools = if spilled {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(module, pools, &pipeline).unwrap();
    let typed =
        keel_codegen::x86_64::compile_program_with_pipeline(module, pools, &pipeline).unwrap();
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
        module,
        pools,
        &pipeline,
        &mut |p| chunks.items.extend(p.items),
    )
    .unwrap();
    for p in [&typed, &chunks] {
        for item in &p.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(features.contains(i.required_features()), "{i}");
            }
        }
    }
    let object = keel_asm::x86::assemble_program_object_with_features(&typed, features).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap()
    );
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (text, object)
}
#[test]
fn selection_bounds_leaves_aliases_and_debug_boundaries() {
    use keel_codegen::select::{Selection, Target};
    for offset in OFFSETS {
        for mode in 0..2 {
            let module = keel_ir::parse(&source(offset, false, mode)).unwrap();
            let f = &module.functions[0];
            let insts = f.block_insts(f.blocks().next().unwrap());
            let selected = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
            let expected = i32::try_from(offset).is_ok() && mode == 0;
            assert_eq!(selected.covered(insts[1]), expected, "{offset}, {mode}");
            assert_eq!(
                selected.operands(f, insts[2]),
                vec![keel_ir::Value::new(if expected { 0 } else { 3 })]
            );
        }
    }
    for (address_line, load_line, expected) in [
        (Some(2), Some(2), true),
        (Some(2), Some(3), false),
        (Some(2), None, false),
        (None, Some(2), true),
    ] {
        let mut module = keel_ir::parse(&source(16, false, 0)).unwrap();
        let f = &mut module.functions[0];
        let insts = f.block_insts(f.blocks().next().unwrap()).to_vec();
        for (inst, line) in [(insts[1], address_line), (insts[2], load_line)] {
            f.set_inst_loc(
                inst,
                line.map(|line| keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line,
                    col: 1,
                    scope: None,
                }),
            );
        }
        let selected = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
        assert_eq!(selected.covered(insts[1]), expected);
    }
    let mut module = keel_ir::parse(&source(16, false, 0)).unwrap();
    let f = &mut module.functions[0];
    f.replace_all_uses(keel_ir::Value::new(0), keel_ir::Value::new(1));
    let insts = f.block_insts(f.blocks().next().unwrap());
    let selected = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
    assert_eq!(selected.operands(f, insts[2]), vec![keel_ir::Value::new(1)]);
    // Cross-block, dynamic-index, volatile and other 16-byte types remain outside this pilot.
    for src in [
        source(16, false, 0).replace("load.i32x4 align 1", "load.i32x4 volatile align 1"),
        source(16, false, 0).replace("i32x4", "i8x16"),
        source(16, false, 0).replace("i32x4", "i16x8"),
        source(16, false, 0).replace("i32x4", "i64x2"),
        source(16, false, 0).replace("i32x4", "f32x4"),
        source(16, false, 0).replace("i32x4", "f64x2"),
        source(16, false, 0).replace("i32x4", "i32x8"),
        source(16, false, 0).replace("i32x4", "i128"),
        source(16, false, 0).replace("v4 = load", "jump block1\nblock1:\n v4 = load"),
        source(16, false, 0)
            .replace("ptr,ptr)", "ptr,ptr,i64)")
            .replace("v1:ptr):", "v1:ptr,v20:i64):")
            .replace("ptradd v0,v2", "ptradd v0,v20"),
    ] {
        let module = keel_ir::parse(&src).unwrap();
        let f = &module.functions[0];
        let insts = f.block_insts(f.blocks().next().unwrap());
        assert!(
            !Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL)
                .covered(insts[1]),
            "{src}"
        );
    }
}

#[test]
fn resolved_address_aliases_count_shared_uses() {
    use keel_codegen::select::{Selection, Target};
    let src = source(16, false, 1).replace(
        "v6 = load.i32x4 align 1 v3",
        "v20 = ptradd v0,v2\n v6 = load.i32x4 align 1 v20",
    );
    let mut module = keel_ir::parse(&src).unwrap();
    let f = &mut module.functions[0];
    let insts = f.block_insts(f.blocks().next().unwrap()).to_vec();
    // The two raw load operands differ, but resolve to one shared address.
    f.replace_all_uses(f.first_result(insts[4]), f.first_result(insts[1]));
    let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
    assert!(!plan.covered(insts[1]));
    assert!(!plan.covered(insts[4]));
    for load in [insts[2], insts[5]] {
        assert_eq!(plan.operands(f, load), vec![f.first_result(insts[1])]);
    }
    // A sole resolved address alias still selects the surviving definition.
    let src = source(16, false, 0).replace(
        "v4 = load.i32x4 align 1 v3",
        "v20 = ptradd v0,v2\n v4 = load.i32x4 align 1 v20",
    );
    let mut module = keel_ir::parse(&src).unwrap();
    let f = &mut module.functions[0];
    let insts = f.block_insts(f.blocks().next().unwrap()).to_vec();
    f.replace_all_uses(f.first_result(insts[2]), f.first_result(insts[1]));
    let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
    assert!(plan.covered(insts[1]));
    assert_eq!(plan.operands(f, insts[3]), vec![keel_ir::Value::new(0)]);
}

#[test]
fn exact_loads_features_parity_and_native_exports() {
    fs::create_dir_all(dir()).unwrap();
    for (profile, features, wide) in [
        ("v1", Features::for_level(CpuLevel::V1), false),
        ("v2", Features::for_level(CpuLevel::V2), false),
        (
            "avx",
            Features::for_level(CpuLevel::V3)
                .with_features("-avx2")
                .unwrap(),
            true,
        ),
        ("v3", Features::for_level(CpuLevel::V3), true),
    ] {
        let mut src = String::from("decl @clobber()\n");
        let mut c = String::from(include_str!("x86_constant_vector_address/harness.c"));
        let mut cases = String::from("static const struct Case cases[]={\n");
        for (i, offset) in OFFSETS.into_iter().enumerate() {
            for mode in 0..5 {
                let name = format!("f_{i}_{mode}");
                src += &source(offset, wide, mode).replace("@f(", &format!("@{name}("));
                c += &format!("extern void {name}(const void*,void*);\n");
                cases += &format!(
                    "{{{name},UINT64_C({}),{},{mode}}},\n",
                    offset as u64,
                    usize::from(wide)
                );
            }
        }
        c += &(cases + "};\n");
        c += include_str!("x86_constant_vector_address/main.c");
        let module = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for spilled in [false, true] {
                    let (text, object) = compile(&module, s, a, spilled, features);
                    if wide && features.avx2 {
                        assert!(text.contains("vpmovsxdq"));
                    }
                    if !features.avx2 {
                        assert!(!text.contains("vpmovsxdq"));
                    }
                    let name = format!("{profile}-{s:?}-{a:?}-{spilled}");
                    fs::write(dir().join(format!("{name}.s")), text).unwrap();
                    fs::write(dir().join(format!("{name}.o")), object).unwrap();
                    fs::write(dir().join(format!("{name}.c")), &c).unwrap();
                }
            }
        }
    }
}

#[test]
fn selection_options_and_symbolic_base_keep_exact_accesses() {
    let src = source(-17, false, 0);
    let module = keel_ir::parse(&src).unwrap();
    let f = &module.functions[0];
    let insts = f.block_insts(f.blocks().next().unwrap());
    for trees in [false, true] {
        for folded_operands in [false, true] {
            for register_hints in [false, true] {
                let plan = keel_codegen::select::Selection::compute_with_options(
                    f,
                    keel_codegen::select::Target::X86,
                    SelectionOptions {
                        trees,
                        folded_operands,
                        register_hints,
                    },
                );
                assert_eq!(plan.covered(insts[1]), trees);
            }
        }
    }
    // Symbol materialization remains a leaf, rather than inventing a RIP-relative relocation.
    let src = source(16, false, 0)
        .replace("v2 = iconst", "v10 = symbol_addr @bytes\n v2 = iconst")
        .replace("ptradd v0,v2", "ptradd v10,v2");
    let src = format!("data @bytes = align 1 {{ i64 0, i64 1, i64 2, i64 3 }}\n{src}");
    let module = keel_ir::parse(&src).unwrap();
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for spilled in [false, true] {
            let (text, _) = compile(
                &module,
                SelectionTier::Full,
                AllocationTier::Full,
                spilled,
                Features::for_level(level),
            );
            assert!(text.contains("bytes"));
            assert!(text.contains("xmmword ptr ["), "{text}");
        }
    }
    // Stores of this type have not acquired address selection.
    let src = source(16, false, 0).replace("store align 1 v4,v1", "store align 1 v4,v3");
    let module = keel_ir::parse(&src).unwrap();
    let f = &module.functions[0];
    let insts = f.block_insts(f.blocks().next().unwrap());
    let plan = keel_codegen::select::Selection::compute_with_options(
        f,
        keel_codegen::select::Target::X86,
        SelectionOptions::ALL,
    );
    assert!(!plan.covered(insts[1]));
}

#[test]
fn export_unchanged_controls() {
    fs::create_dir_all(dir()).unwrap();
    for (name, src) in [
        ("arm", source(16, false, 0)),
        ("scalar", source(16, false, 0).replace("i32x4", "i64")),
        (
            "other_vector",
            source(16, false, 0).replace("i32x4", "i64x2"),
        ),
        (
            "volatile",
            source(16, false, 0).replace("load.i32x4 align 1", "load.i32x4 volatile align 1"),
        ),
    ] {
        let module = keel_ir::parse(&src).unwrap();
        for (tier, options) in [
            ("base", SelectionOptions::default()),
            (
                "trees",
                SelectionOptions {
                    trees: true,
                    ..SelectionOptions::default()
                },
            ),
            ("full", SelectionOptions::ALL),
        ] {
            let target = if name == "arm" {
                keel_codegen::Target::Aarch64AppleDarwin
            } else {
                keel_codegen::Target::X86_64LinuxGnu
            };
            let text = keel_codegen::compile_with_options(&module, target, options).unwrap();
            fs::write(dir().join(format!("{name}-{tier}.control.s")), text).unwrap();
        }
    }
}
