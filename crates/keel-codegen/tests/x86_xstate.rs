//! Owned standard-format xstate; Linux execution belongs to the parent runner.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::EntityRef;
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
mod common;
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
    let export = std::env::var_os("KEEL_X86_XSTATE_EXPORT_DIR").map(PathBuf::from);
    let d = export
        .clone()
        .unwrap_or_else(|| common::temp("xstate-packet"));
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("test.c"), include_str!("x86_xstate/test.c")).unwrap();
    fs::write(d.join("README.md"), include_str!("x86_xstate/README.md")).unwrap();
    fs::write(d.join("encoding.S"), include_str!("x86_xstate/encoding.S")).unwrap();
    fs::write(d.join("builtin.c"), include_str!("x86_xstate/builtin.c")).unwrap();
    fs::write(d.join("test.ir"), source(true)).unwrap();
    fs::write(d.join("probe.S"), include_str!("x86_xstate/probe.S")).unwrap();
    fs::write(d.join("run.py"), include_str!("x86_xstate/run.py")).unwrap();
    let mut names = String::new();
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
                let name = format!("{s:?}-{a:?}-{pool}");
                names += &format!("{name}\n");
                fs::write(d.join(format!("{name}.o")), obj).unwrap();
                fs::write(d.join(format!("{name}.s")), text).unwrap();
            }
        }
    }
    fs::write(d.join("objects.txt"), names).unwrap();
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
    let asm = include_str!("x86_xstate/encoding.S");
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
    let gnu = std::env::var("KEEL_XSTATE_GNU_AS").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/x86_64-elf-as".into()
        } else {
            "as".into()
        }
    });
    for tool in ["clang", gnu.as_str()] {
        let mut command = Command::new(tool);
        if tool == "clang" {
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
    common::command(Command::new("python3").arg(d.join("run.py")).arg("--seal"));
    common::command(Command::new("python3").arg(d.join("run.py")).arg("--check"));
    let victim = d.join("Baseline-Fast-normal.o");
    let original = fs::read(&victim).unwrap();
    fs::write(&victim, b"tampered").unwrap();
    let rejected = Command::new("python3")
        .arg(d.join("run.py"))
        .arg("--check")
        .output()
        .unwrap();
    fs::write(&victim, original).unwrap();
    assert!(
        !rejected.status.success()
            && String::from_utf8_lossy(&rejected.stderr).contains("packet hash mismatch")
    );
    assert!(
        !Command::new("python3")
            .arg("-O")
            .arg(d.join("run.py"))
            .arg("--check")
            .output()
            .unwrap()
            .status
            .success()
    );
    for flag in ["-O", "-OO"] {
        fs::write(d.join("results.json"), b"stale acceptance").unwrap();
        let rejected = Command::new("python3")
            .arg(flag)
            .arg(d.join("run.py"))
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("optimized Python"));
        assert!(!d.join("results.json").exists());
    }
    if export.is_none() {
        fs::remove_dir_all(d).unwrap();
    }
}
