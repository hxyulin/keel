//! Owned standard-format xstate; Linux execution belongs to the parent runner.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::EntityRef;
use keel_test_support::native_program;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const CUSTOM: Pools = Pools {
    caller_saved: &[6, 7, 8, 9],
    callee_saved: &[3, 12, 13, 14],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
fn types(wide: bool) -> Vec<keel_ir::Type> {
    use keel_ir::{Type, VectorType};
    [Type::F32, Type::F64]
        .into_iter()
        .chain(
            VectorType::ALL_128
                .into_iter()
                .chain(VectorType::ALL_256.into_iter().filter(|_| wide))
                .map(Type::Vector),
        )
        .collect()
}
fn source(wide: bool) -> String {
    let mut s = String::new();
    for op in ["save", "restore"] {
        let intrinsic = if op == "save" {
            "x86_xsave64"
        } else {
            "x86_xrstor64"
        };
        s += &format!(
            "func @{op}(ptr,i64) {{\nblock0(v0:ptr,v1:i64):\n intrinsic {intrinsic}(v0,v1)\n return\n}}\n"
        );
        for (i, t) in types(wide).iter().enumerate() {
            s += &format!(
                "func @{op}_live{i}(ptr,i64,ptr,ptr,i64) {{\nblock0(v0:ptr,v1:i64,v2:ptr,v3:ptr,v4:i64):\n v5 = load.{t} align 1 v2\n intrinsic {intrinsic}(v0,v1)\n store align 1 v5,v3\n v6 = iconst.i64 32\n v7 = ptradd v3,v6\n store align 1 v4,v7\n return\n}}\n"
            );
        }
    }
    s += "func @ordered(ptr,i64,i32)->i32 {\nblock0(v0:ptr,v1:i64,v2:i32):\n v3 = intrinsic x86_sse_swap_mxcsr(v2)\n intrinsic x86_xsave64(v0,v1)\n intrinsic x86_xrstor64(v0,v1)\n v4 = intrinsic x86_sse_read_mxcsr()\n return v4\n}\n";
    s += "func @raw_ordered(ptr,i64,ptr,ptr)->i32 {\nblock0(v0:ptr,v1:i64,v2:ptr,v3:ptr):\n v4 = load.f32 align 1 v2\n intrinsic x86_xrstor64(v0,v1)\n v5 = intrinsic x86_sse_addss(v4,v4)\n intrinsic x86_xsave64(v0,v1)\n v6 = intrinsic x86_sse_read_mxcsr()\n store align 1 v5,v3\n return v6\n}\n";
    s
}
// Exercise the public plug-in allocator interface and its clobber classifier.
// This adapter deliberately delegates dataflow to a built-in allocator; arbitrary
// third-party Allocation plans still must honor is_call and transfer liveness.
struct CheckedAllocator(AllocationTier);
impl keel_codegen::regalloc::RegisterAllocator for CheckedAllocator {
    fn name(&self) -> &'static str {
        "xstate-checked"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_ir::{InstData, Intrinsic};
        let mut restore = false;
        for (_, at) in input.func.layout_insts() {
            if let InstData::Intrinsic { id, .. } = input.func.inst_data(at) {
                if *id == Intrinsic::X86Xrstor64 {
                    assert!((input.is_call)(input.func.inst_data(at)));
                    restore = true;
                }
                if *id == Intrinsic::X86Xsave64 {
                    assert!(!(input.is_call)(input.func.inst_data(at)));
                }
            }
        }
        let allocation = keel_codegen::regalloc::RegisterAllocator::allocate(&self.0, input);
        if restore {
            for (_, at) in input.func.layout_insts() {
                if let InstData::Load { .. } = input.func.inst_data(at) {
                    let value = input.func.first_result(at);
                    assert!(
                        matches!(
                            allocation.locs[0][value.index()],
                            Some(keel_codegen::regalloc::Loc::Stack(_))
                        ),
                        "live SIMD value must spill across XRSTOR"
                    );
                }
            }
        }
        allocation
    }
}
fn compile(
    s: SelectionTier,
    a: AllocationTier,
    p: &Pools,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(&source(f.avx)).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &CheckedAllocator(a),
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, p, &pipeline)?;
    let program = keel_codegen::x86_64::compile_program_with_pipeline(&m, p, &pipeline)?;
    let obj = keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
    assert_eq!(
        obj,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    assert_eq!(
        text.lines()
            .filter(|l| l.split_whitespace().next() == Some("xsave64"))
            .count(),
        types(f.avx).len() + 3
    );
    // Capture needs no SIMD/x87 setup, calls, or user-image shadow copy.
    let only = text
        .split("\"save\":")
        .nth(1)
        .unwrap()
        .split(".size")
        .next()
        .unwrap();
    assert!(!only.contains("xmm") && !only.contains("ymm") && !only.contains("call\t"));
    assert_eq!(
        text.lines()
            .filter(|l| l.split_whitespace().next() == Some("xrstor64"))
            .count(),
        types(f.avx).len() + 3
    );
    Ok((obj, text))
}
#[test]
fn xstate_contract_and_packet() {
    let mut f = Features::NONE;
    for (cpu, os) in [(false, false), (true, false), (false, true)] {
        f.xsave = cpu;
        f.osxsave = os;
        assert!(compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f).is_err());
    }
    f.xsave = true;
    f.osxsave = true;
    compile(SelectionTier::Full, AllocationTier::Full, &EMPTY, f).unwrap();
    assert!(keel_codegen::aarch64::compile(&keel_ir::parse(&source(true)).unwrap()).is_err());
    for bad in [
        "x86_xsave64(v0)",
        "x86_xsave64(v1,v0)",
        "x86_xsave64(v0,v1,v1)",
    ] {
        let src = source(true).replace("x86_xsave64(v0,v1)", bad);
        assert!(keel_ir::parse(&src).map_or(true, |m| keel_ir::verify_module(&m).is_err()));
    }
    for id in [
        keel_ir::Intrinsic::X86Xsave64,
        keel_ir::Intrinsic::X86Xrstor64,
    ] {
        use keel_ir::{InstData, MemoryEffects};
        assert!(!id.info().pure);
        let src = format!(
            "func @f(ptr,i64) {{\nblock0(v0:ptr,v1:i64):\n intrinsic {id}(v0,v1)\n return\n}}\n"
        );
        let m = keel_ir::parse(&src).unwrap();
        let function = &m.functions[0];
        for (_, at) in function.layout_insts() {
            if let InstData::Intrinsic { .. } = function.inst_data(at) {
                assert_eq!(
                    function.inst_data(at).memory_effects(),
                    MemoryEffects::Barrier
                );
            }
        }
        assert!(keel_codegen::aarch64::compile(&m).is_err());
        for (cpu, os, ok) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, true),
        ] {
            let mut features = Features::NONE;
            features.xsave = cpu;
            features.osxsave = os;
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            assert_eq!(
                keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).is_ok(),
                ok
            );
        }

        for bad in [
            format!("{id}(v0)"),
            format!("{id}(v1,v0)"),
            format!("{id}(v0,v1,v1)"),
        ] {
            assert!(
                keel_ir::parse(&src.replace(&format!("{id}(v0,v1)"), &bad))
                    .map_or(true, |m| keel_ir::verify_module(&m).is_err())
            );
        }
    }
    let d = keel_test_support::TempDir::new("xstate");
    let test_c = include_str!("../x86_xstate/test.c");
    let probe_s = include_str!("../x86_xstate/probe.S");
    let builtin_c = include_str!("../x86_xstate/builtin.c");
    fs::write(d.join("test.c"), test_c).unwrap();
    fs::write(
        d.join("encoding.S"),
        include_str!("../x86_xstate/encoding.S"),
    )
    .unwrap();
    fs::write(d.join("builtin.c"), builtin_c).unwrap();
    fs::write(d.join("probe.S"), probe_s).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, p) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("spill", &EMPTY),
                ("custom", &CUSTOM),
            ] {
                let (obj, text) = compile(s, a, p, Features::for_level(CpuLevel::V3)).unwrap();
                // Every object runs 7 masks, 28 patterns, 57,344 live calls,
                // 8 fault cases and 8 raw MXCSR cases; see x86_xstate/README.md.
                native_program!(format!("{s:?}-{a:?}-{pool}"))
                    .requires(&["xsave", "avx"])
                    .file("test.c", test_c)
                    .file("probe.S", probe_s)
                    .file("builtin.c", builtin_c)
                    .file("keel.s", text)
                    .file("keel.o", obj)
                    .compile(["-O2", "-mxsave", "-c", "builtin.c", "-o", "builtin.o"])
                    .link([
                        "-O2",
                        "-fno-vectorize",
                        "-fno-slp-vectorize",
                        "-mavx",
                        "-mno-red-zone",
                        "test.c",
                        "probe.S",
                        "builtin.o",
                        "keel.o",
                    ])
                    .stdout("PASS xstate masks=7 patterns=28 live=57344 faults=8 raw=8\n")
                    .check();
            }
        }
    }
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-DKEEL_CROSS_SYNTAX",
                "-mavx",
                "-fsyntax-only",
            ])
            .arg(d.join("test.c")),
    );
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-mavx", "-c"])
            .arg(d.join("probe.S"))
            .arg("-o")
            .arg(d.join("probe.o")),
    );
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-DKEEL_CROSS_SYNTAX",
                "-mavx",
                "-O2",
                "-c",
            ])
            .arg(d.join("test.c"))
            .arg("-o")
            .arg(d.join("harness.o")),
    );
    // The pinned XED /4 and /5 REX.W forms agree with both primary assemblers.
    use object::{Object, ObjectSection};
    let expected = [
        0x48, 0x0f, 0xae, 0x21, 0x48, 0x0f, 0xae, 0x29, 0x49, 0x0f, 0xae, 0x64, 0x24, 0x40, 0x49,
        0x0f, 0xae, 0x6c, 0x24, 0x40,
    ];
    let asm = include_str!("../x86_xstate/encoding.S");
    let obj = keel_asm::x86::assemble_object_with_features(asm, Features::for_level(CpuLevel::V3))
        .unwrap();
    let text_bytes = |b: &[u8]| {
        object::File::parse(b)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    assert_eq!(text_bytes(&obj), expected);
    let gnu = keel_test_support::gnu_as();
    if gnu.is_none() {
        keel_test_support::skip_part!(keel_test_support::NO_GNU_AS);
    }
    for gnu in std::iter::once(None).chain(gnu.map(Some)) {
        let mut command = gnu.map_or_else(|| Command::new("clang"), |gnu| gnu.command());
        if gnu.is_none() {
            command.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        } else {
            command.arg("--64");
        }
        common::command(
            command
                .arg(d.join("encoding.S"))
                .arg("-o")
                .arg(d.join("encoding.o")),
        );
        assert_eq!(
            text_bytes(&fs::read(d.join("encoding.o")).unwrap()),
            expected
        );
    }
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-O2",
                "-mxsave",
                "-c",
            ])
            .arg(d.join("builtin.c"))
            .arg("-o")
            .arg(d.join("builtin.o")),
    );
    let builtin = text_bytes(&fs::read(d.join("builtin.o")).unwrap());
    assert!(builtin.windows(4).any(|b| b == [0x48, 0x0f, 0xae, 0x27]));
    assert!(builtin.windows(4).any(|b| b == [0x48, 0x0f, 0xae, 0x2f]));
}
