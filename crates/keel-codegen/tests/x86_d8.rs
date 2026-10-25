//! Numerical D8 vectors use the XMM bank, never the GNU physical MMX tag.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::VectorType;
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;

// f64x1 remains in every pointer-based numerical corpus. Its C value ABI
// is an explicit rejection roster, not an interchangeable sixth C shape.
const C_ABI_SHAPES: [VectorType; 5] = [
    VectorType::I8x8,
    VectorType::I16x4,
    VectorType::I32x2,
    VectorType::I64x1,
    VectorType::F32x2,
];
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    caller_saved: &[6],
    callee_saved: &[3],
    float_caller_saved: &[1],
    float_callee_saved: &[],
};

fn fixtures() -> Vec<(&'static str, String, String)> {
    let semantics =
        common::fixture_for(&VectorType::ALL_64).replace("-> f64x1 {", "-> f64x1 cc(fast) local {");
    // A one-lane pairwise operation combines a[0] and b[0]. The older Q
    // oracle's n/2 indexing assumes an even number of lanes.
    let reference = common::harness_for(&VectorType::ALL_64)
        .replace("(i<0?a[2*i]:b[2*(i-0)])", "a[0]")
        .replace("(i<0?a[2*i+1]:b[2*(i-0)+1])", "b[0]");
    let (mut abi, mut c) = common::abi_fixture_for(false, &C_ABI_SHAPES);
    // Common ABI harness also exercises scalar selects from the semantic module.
    for t in C_ABI_SHAPES {
        abi += &format!(
            "func @scalar_select_{t}(i64,{t},{t})->{t} {{\nentry(v0:i64,v1:{t},v2:{t}):\n v3 = select v0,v1,v2\n return v3\n}}\n"
        );
        abi += &format!(
            "func @indirect_tail_{t}(ptr,{t})->{t} {{\nentry(v0:ptr,v1:{t}):\n return_call_indirect v0(v1) : ({t})->{t}\n}}\n"
        );
        c = c.replace(
            &format!("extern V_{t} indirect_{t}"),
            &format!(
                "extern V_{t} indirect_tail_{t}(V_{t}(*)(V_{t}),V_{t});\nextern V_{t} indirect_{t}"
            ),
        );
        c = c.replace(&format!("r=indirect_{t}(external_{t},a);"), &format!("r=indirect_tail_{t}(external_{t},a);if(memcmp(&r,&e,8))return 20;\nr=indirect_{t}(external_{t},a);"));
    }
    extend_ninth_calls(&mut abi, &mut c);
    vec![
        ("semantics", semantics, reference),
        ("abi", abi, c),
        (
            "boundaries",
            boundary_ir(true),
            include_str!("x86_d8/boundaries.c").into(),
        ),
    ]
}

fn extend_ninth_calls(ir: &mut String, c: &mut String) {
    let mut declarations = String::new();
    let mut checks = String::new();
    for t in C_ABI_SHAPES {
        let types = std::iter::repeat_n(t.to_string(), 9)
            .collect::<Vec<_>>()
            .join(",");
        let params = (0..9)
            .map(|i| format!("v{i}:{t}"))
            .collect::<Vec<_>>()
            .join(",");
        let args = (0..9)
            .rev()
            .map(|i| format!("v{i}"))
            .collect::<Vec<_>>()
            .join(",");
        *ir += &format!(
            "decl @c_ninth_{t}({types})->{t}\nfunc @ninth_{t}({types})->{t} {{\nentry({params}):\n return v8\n}}\n"
        );
        for tail in [false, true] {
            let name = if tail { "tail_ninth" } else { "call_ninth" };
            let body = if tail {
                format!(" return_call @c_ninth_{t}({args})")
            } else {
                format!(" v9 = call @c_ninth_{t}({args})\n return v9")
            };
            *ir += &format!("func @{name}_{t}({types})->{t} {{\nentry({params}):\n{body}\n}}\n");
            let body = if tail {
                format!(" return_call_indirect v9({args}) : ({types})->{t}")
            } else {
                format!(" v10 = call_indirect v9({args}) : ({types})->{t}\n return v10")
            };
            *ir += &format!(
                "func @{name}_ind_{t}({types},ptr)->{t} {{\nentry({params},v9:ptr):\n{body}\n}}\n"
            );
        }
        let ctypes = std::iter::repeat_n(format!("V_{t}"), 9)
            .collect::<Vec<_>>()
            .join(",");
        let cparams = (0..9)
            .map(|i| format!("V_{t} a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        declarations += &format!("V_{t} c_ninth_{t}({cparams}){{");
        for i in 0..8 {
            declarations += &format!("(void)a{i};");
        }
        declarations += "return a8;}\n";
        for name in ["ninth", "call_ninth", "tail_ninth"] {
            declarations += &format!("extern V_{t} {name}_{t}({ctypes});\n");
        }
        for name in ["call_ninth_ind", "tail_ninth_ind"] {
            declarations += &format!("extern V_{t} {name}_{t}({ctypes},V_{t}(*)({ctypes}));\n");
        }
        checks += &format!(
            "{{V_{t} a[9],r;for(unsigned j=0;j<9;j++)for(unsigned i=0;i<8;i++)((unsigned char*)&a[j])[i]=(unsigned char)(17*j+i);\n"
        );
        let cargs = (0..9)
            .map(|i| format!("a[{i}]"))
            .collect::<Vec<_>>()
            .join(",");
        checks += &format!("r=ninth_{t}({cargs});if(memcmp(&r,&a[8],8))return 21;\n");
        for name in ["call_ninth", "tail_ninth"] {
            checks += &format!("r={name}_{t}({cargs});if(memcmp(&r,&a[0],8))return 22;\n");
        }
        for name in ["call_ninth_ind", "tail_ninth_ind"] {
            checks +=
                &format!("r={name}_{t}({cargs},c_ninth_{t});if(memcmp(&r,&a[0],8))return 23;\n");
        }
        checks += "}\n";
    }
    *c = c.replace("int main(void){", &(declarations + "int main(void){"));
    *c = c.replace("return 0;}", &(checks + "return 0;}"));
}

fn boundary_ir(wide: bool) -> String {
    let mut s = String::new();
    for t in VectorType::ALL_64 {
        s += &format!(
            "func @copy_{t}(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n v2 = load.{t} align 1 v0\n store align 1 v2,v1\n return\n}}\n"
        );
        let op = if t.lane_type().is_float() {
            "vfadd"
        } else {
            "viadd"
        };
        s += &format!(
            "func @add_{t}(ptr,ptr,ptr) {{\nentry(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.{t} align 1 v0\n v4 = load.{t} align 1 v1\n v5 = {op} v3,v4\n store align 1 v5,v2\n return\n}}\n"
        );
        // Register and stack paths for true wrappers, nested wrappers and
        // empty siblings at D width. The existing Y ambiguity stays rejected.
        for (n, l) in [
            format!("{{{t}}}"),
            format!("{{{{[1 x {t}]}}}}"),
            format!("{{[0 x i8],{t}}}"),
        ]
        .iter()
        .enumerate()
        .filter(|_| t != VectorType::F64x1)
        {
            s += &format!(
                "decl @c_wrap_{t}_{n}(ptr sret {l},ptr byval {l})\nfunc @wrap_{t}_{n}(ptr sret {l},ptr byval {l}) {{\nentry(v0:ptr,v1:ptr):\n v2 = load.{t} align 1 v1\n store align 1 v2,v0\n return\n}}\nfunc @call_wrap_{t}_{n}(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n call @c_wrap_{t}_{n}(v1,v0)\n return\n}}\n"
            );
        }
    }
    // Owned private calls and scalar/pointer C adapters retain numerical
    // f64x1 without exposing its unsupported C vector-value ABI.
    s += "func @private_f64x1(f64x1)->f64x1 cc(fast) local {\nentry(v0:f64x1):\n v1 = vfadd v0,v0\n return v1\n}\nfunc @private_roundtrip_f64x1(ptr,ptr) {\nentry(v0:ptr,v1:ptr):\n v2 = load.f64x1 align 1 v0\n v3 = call @private_f64x1(v2)\n store align 1 v3,v1\n return\n}\nfunc @scalar_roundtrip_f64x1(f64)->f64 {\nentry(v0:f64):\n v1 = bitcast.f64x1 v0\n v2 = call @private_f64x1(v1)\n v3 = bitcast.f64 v2\n return v3\n}\n";
    // Both directions across D/Q/Y width boundaries must scalarize. All
    // destination lanes, including narrow padding lanes, are initialized.
    for (from, to) in [
        ("i8x8", "i16x8"),
        ("i8x8", "i32x8"),
        ("i16x8", "i8x8"),
        ("i16x16", "i8x8"),
        ("i16x4", "i8x8"),
        ("i32x2", "i16x4"),
        ("i64x1", "i32x2"),
    ] {
        if !wide && (from == "i16x16" || to == "i32x8") {
            continue;
        }
        let widen = from.starts_with("i8") && !to.starts_with("i8");
        let ops: &[&str] = if widen {
            &["swiden_low", "uwiden_low"]
        } else {
            &["narrow_low"]
        };
        for op in ops {
            s += &format!(
                "func @{op}_{from}_{to}(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n v2 = load.{from} align 1 v0\n v3 = {op}.{to} v2\n store align 1 v3,v1\n return\n}}\n"
            );
        }
    }
    for t in VectorType::ALL_64
        .into_iter()
        .filter(|t| t.lane_type().is_int())
    {
        for op in keel_ir::VectorReduceOp::ALL {
            s += &format!(
                "func @{op}_{t}(ptr)->{} {{\nentry(v0:ptr):\n v1 = load.{t} align 1 v0\n v2 = {op} v1\n return v2\n}}\n",
                if t.lane_type().bytes().unwrap() < 4 {
                    format!("{} uext", t.lane_type())
                } else {
                    t.lane_type().to_string()
                }
            );
        }
    }
    for t in [VectorType::F32x2, VectorType::F64x1] {
        s += &format!(
            "func @fma_{t}(ptr,ptr,ptr,ptr) {{\nentry(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n v4 = load.{t} align 1 v0\n v5 = load.{t} align 1 v1\n v6 = load.{t} align 1 v2\n v7 = fma v4,v5,v6\n store align 1 v7,v3\n return\n}}\n"
        );
    }
    s
}

fn compile(
    source: &str,
    s: SelectionTier,
    a: AllocationTier,
    pools: &Pools,
    features: Features,
) -> (String, Vec<u8>, Vec<u8>) {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
        keel_ir::verify_module(&m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let typed = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    let parsed = keel_asm::x86::assemble_object_with_features(&text, features).unwrap();
    assert_eq!(typed, parsed);
    if features == Features::NONE {
        assert!(!text.contains("ymm"));
        for line in text.lines() {
            assert!(
                !line.trim_start().starts_with('v'),
                "VEX in strict SSE2: {line}"
            );
        }
    }
    (text, typed, parsed)
}

#[test]
fn d_matrix_export_and_clang_encoding() {
    use object::{Object, ObjectSection};
    let export = std::env::var_os("KEEL_X86_D8_EXPORT_DIR").map(PathBuf::from);
    let dir = common::temp("x86-d8");
    for (kind, source, c) in fixtures() {
        fs::write(dir.join("reference.c"), &c).unwrap();
        let target = if cfg!(target_os = "macos") {
            "x86_64-apple-macos11"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        common::command(
            Command::new("clang")
                .args([
                    "-target",
                    target,
                    "-std=gnu11",
                    "-DKEEL_D8_AVX2",
                    "-fsyntax-only",
                ])
                .arg(dir.join("reference.c")),
        );
        if let Some(d) = &export {
            fs::create_dir_all(d).unwrap();
            fs::write(d.join(format!("{kind}.ir")), &source).unwrap();
            fs::write(d.join(format!("{kind}.c")), &c).unwrap();
        }
        for (feature, features) in [
            ("none", Features::NONE),
            ("default", Features::default()),
            ("avx2", Features::for_level(CpuLevel::V3)),
        ] {
            for s in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                for a in [AllocationTier::Fast, AllocationTier::Full] {
                    for (pool, pools) in [
                        ("normal", &keel_codegen::x86_64::REGISTERS),
                        ("tiny", &TINY),
                        ("empty", &EMPTY),
                    ] {
                        let narrow;
                        let source = if kind == "boundaries" && !features.avx {
                            narrow = boundary_ir(false);
                            &narrow
                        } else {
                            &source
                        };
                        let (text, typed, parsed) = compile(source, s, a, pools, features);
                        let name = format!("{kind}-{feature}-{s:?}-{a:?}-{pool}");
                        if let Some(d) = &export {
                            fs::write(d.join(format!("{name}.s")), &text).unwrap();
                            fs::write(d.join(format!("{name}.typed.o")), &typed).unwrap();
                            fs::write(d.join(format!("{name}.text.o")), parsed).unwrap();
                        }
                        // Independent assembler oracle for every emitted function.
                        fs::write(dir.join("test.s"), &text).unwrap();
                        common::command(
                            Command::new("clang")
                                .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
                                .arg(dir.join("test.s"))
                                .arg("-o")
                                .arg(dir.join("clang.o")),
                        );
                        let clang_bytes = fs::read(dir.join("clang.o")).unwrap();
                        let clang = object::File::parse(&*clang_bytes).unwrap();
                        let keel = object::File::parse(&*typed).unwrap();
                        assert_eq!(
                            clang.section_by_name(".text").unwrap().data().unwrap(),
                            keel.section_by_name(".text").unwrap().data().unwrap(),
                            "{name}"
                        );
                    }
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn d_external_memory_is_exactly_eight_bytes() {
    for t in VectorType::ALL_64 {
        let source = format!(
            "decl @touch()\nfunc @copy(ptr,ptr) {{\nentry(v0:ptr,v1:ptr):\n v2 = load.{t} align 1 v0\n call @touch()\n store align 1 v2,v1\n return\n}}\n"
        );
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
            let (text, _, _) = compile(
                &source,
                SelectionTier::Baseline,
                AllocationTier::Full,
                pools,
                Features::NONE,
            );
            assert!(text.contains("movq\txmm"), "{text}");
            assert!(!text.contains("xmmword ptr"), "{text}");
            assert!(!text.contains("ymm"));
        }
    }
}

#[test]
fn natural_small_d_aggregate_boundaries_accept_except_untagged_f64x1() {
    for t in VectorType::ALL_64 {
        for layout in [format!("{{{t},i64}}"), format!("{{{t},{t}}}")] {
            // Exercise a defined boundary and its call, rather than an unused declaration.
            let source = format!(
                "decl @mixed(ptr sret {layout},ptr byval {layout})\n\
                 func @forward(ptr sret {layout},ptr byval {layout}) {{\n\
                 entry(v0:ptr,v1:ptr):\n return_call @mixed(v0,v1)\n}}\n"
            );
            let m = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&m).unwrap();
            let text = keel_codegen::x86_64::compile(&m);
            let typed = keel_codegen::compile_program(&m, keel_codegen::Target::X86_64LinuxGnu);
            if t == VectorType::F64x1 {
                for error in [text.unwrap_err(), typed.unwrap_err()] {
                    assert!(
                        error.message.contains("f64x1 SysV C ABI is unsupported:"),
                        "{error:?}"
                    );
                }
            } else {
                let text = text.unwrap();
                let typed = typed.unwrap();
                assert!(text.contains("jmp\t"), "{text}");
                assert_eq!(
                    keel_asm::x86::assemble_object(&text).unwrap(),
                    keel_asm::x86::assemble_program_object(&typed).unwrap()
                );
            }
        }
    }
}
