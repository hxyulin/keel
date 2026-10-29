//! Architectural numerical instructions: independent results and raw status.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const OPS: [&str; 9] = [
    "fsin", "fcos", "f2xm1", "fpatan", "fyl2x", "fyl2xp1", "fscale", "fprem", "fprem1",
];
fn source() -> String {
    let mut src = String::new();
    for (n, op) in OPS.iter().enumerate() {
        for mode in ["both", "value", "status", "neither"] {
            src += &format!(
                "func @{op}_{mode}(ptr,ptr,ptr)->i32 {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.f80 align 1 v0\n"
            );
            if n >= 3 {
                src += " v4 = load.f80 align 1 v1\n";
            }
            src += &format!(
                " v5,v6 = intrinsic x86_x87_{op}({})\n",
                if n >= 3 { "v3,v4" } else { "v3" }
            );
            if mode == "both" || mode == "value" {
                src += " store align 1 v5,v2\n";
            }
            src += if mode == "both" || mode == "status" {
                " return v6\n}\n"
            } else {
                " v7 = iconst.i32 0\n return v7\n}\n"
            };
        }
    }
    src
}
#[test]
fn result_liveness_and_architectural_status() {
    let src = source();
    let mut m = keel_ir::parse(&src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(keel_codegen::aarch64::compile(&m).is_err());
    for op in OPS {
        let mut only = m.clone();
        only.functions.retain(|f| f.name == format!("{op}_neither"));
        assert_eq!(only.functions.len(), 1);
        assert!(keel_codegen::aarch64::compile(&only).is_err());
    }
    for bad in [
        src.replace("load.f80", "load.f64"),
        src.replace("v5,v6 =", "v5 ="),
        src.replace("x86_x87_fsin(v3)", "x86_x87_fsin(v3,v3)"),
    ] {
        assert!(keel_ir::parse(&bad).map_or(true, |m| keel_ir::verify_module(&m).is_err()));
    }
    for f in &m.functions {
        let ids: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| match f.inst_data(i) {
                keel_ir::InstData::Intrinsic { id, .. } => {
                    assert_eq!(f.inst_results(i).len(), 2);
                    assert!(!id.info().pure);
                    assert!(id.is_x86_target());
                    assert_eq!(
                        f.inst_data(i).memory_effects(),
                        keel_ir::MemoryEffects::Barrier
                    );
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 1, "pinned operation lost in {}", f.name);
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                let pipeline = CodegenPipeline {
                    selector: &s,
                    allocator: &a,
                };
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                let f = Features::for_level(CpuLevel::V1);
                let typed =
                    keel_asm::x86::assemble_program_object_with_features(&program, f).unwrap();
                assert_eq!(
                    typed,
                    keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
                );
                for op in OPS {
                    let lines: Vec<_> = text.lines().map(str::trim).collect();
                    let found: Vec<_> = lines
                        .iter()
                        .enumerate()
                        .filter(|(_, line)| line.split_whitespace().next() == Some(op))
                        .collect();
                    assert_eq!(found.len(), 4, "{text}");
                    for (i, _) in found {
                        assert!(
                            lines[i + 1].starts_with("fnstsw"),
                            "status must precede cleanup: {text}"
                        );
                        assert!(lines[i + 2].starts_with("fstp"), "{text}");
                    }
                }
            }
        }
    }
}

/// Exercise valid sparse plans sharing the result with either dying input.
struct SharedHome(usize);
impl keel_codegen::regalloc::RegisterAllocator for SharedHome {
    fn name(&self) -> &'static str {
        "x87-numerical-shared-home"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_codegen::regalloc::{Loc, ValuePart};
        use keel_ir::EntityRef;
        let mut a = AllocationTier::Full.allocate(input);
        let f = input.func;
        a.spill_slots = (f.num_values() * 2) as u32;
        for (i, loc) in a.locs[0].iter_mut().enumerate() {
            if loc.is_some() {
                *loc = Some(Loc::Stack(i as u32 * 2));
            }
        }
        for &v in f.block_params(f.entry_block().unwrap()) {
            a.edits.set_param(
                f.entry_block().unwrap(),
                ValuePart::low(v),
                Loc::Stack(v.index() as u32 * 2),
            );
        }
        for (_, i) in f.layout_insts() {
            if let keel_ir::InstData::Intrinsic { args, .. } = f.inst_data(i) {
                let result = f.inst_results(i)[0];
                if a.locs[0][result.index()].is_some() {
                    a.locs[0][result.index()] = a.locs[0][args[self.0.min(args.len() - 1)].index()];
                }
            }
        }
        a.validate(input).unwrap();
        a
    }
}
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;

#[test]
fn independent_oracle_and_packet() {
    use keel_codegen::regalloc::RegisterAllocator;
    use keel_codegen::x86_features::FeatureSelector;
    use object::{Object, ObjectSection};
    use std::{fs, path::PathBuf, process::Command};
    let export = std::env::var_os("KEEL_X87_NUMERICAL_EXPORT_DIR").map(PathBuf::from);
    let d = export
        .clone()
        .unwrap_or_else(|| common::temp("x87-numerical"));
    fs::create_dir_all(&d).unwrap();
    for (name, data) in [
        ("test.c", include_str!("x86_x87_numerical/test.c")),
        ("probe.S", include_str!("x86_x87_numerical/probe.S")),
        ("encoding.S", include_str!("x86_x87_numerical/encoding.S")),
        ("run.py", include_str!("x86_x87_numerical/run.py")),
        ("README.md", include_str!("x86_x87_numerical/README.md")),
    ] {
        fs::write(d.join(name), data).unwrap();
    }
    let src = source();
    fs::write(d.join("test.ir"), &src).unwrap();
    let tiny = Pools {
        callee_saved: &[3],
        float_caller_saved: &[0],
        ..EMPTY
    };
    let mut names = String::new();
    let raw = keel_ir::parse(&src).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    // Only the pool and the shared-home plans change this text. V2, V3 and
    // the Trees selector emit the same text as V1 with Baseline or Full.
    let features = Features::for_level(CpuLevel::V1);
    for s in [SelectionTier::Baseline, SelectionTier::Full] {
        let first = SharedHome(0);
        let second = SharedHome(1);
        let fast = AllocationTier::Fast;
        let full = AllocationTier::Full;
        let plans: [(&str, &dyn RegisterAllocator, &Pools); 8] = [
            ("Fast-normal", &fast, &keel_codegen::x86_64::REGISTERS),
            ("Fast-tiny", &fast, &tiny),
            ("Fast-spill", &fast, &EMPTY),
            ("Full-normal", &full, &keel_codegen::x86_64::REGISTERS),
            ("Full-tiny", &full, &tiny),
            ("Full-spill", &full, &EMPTY),
            ("shared-first", &first, &EMPTY),
            ("shared-second", &second, &EMPTY),
        ];
        for (plan, allocator, pools) in plans {
            for (stage, m) in [("raw", &raw), ("opt", &optimized)] {
                let name = format!("v1-{s:?}-{plan}-{stage}");
                let selector = FeatureSelector {
                    inner: &s,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator,
                };
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline)
                        .unwrap();
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
                let typed =
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap();
                // The text route assembles to the same bytes, so ship one image.
                let parsed = keel_asm::x86::assemble_object_with_features(&text, features).unwrap();
                assert_eq!(typed, parsed);
                fs::write(d.join(format!("{name}.s")), text).unwrap();
                names += &format!("{name}\n");
                fs::write(d.join(format!("{name}.o")), typed).unwrap();
            }
        }
    }
    fs::write(d.join("objects.txt"), names).unwrap();
    for name in ["probe.S", "test.c", "encoding.S"] {
        common::command(
            Command::new("clang")
                .args([
                    "-target",
                    "x86_64-unknown-linux-gnu",
                    "-DKEEL_CROSS_SYNTAX",
                    "-O2",
                    "-c",
                ])
                .arg(d.join(name))
                .arg("-o")
                .arg(d.join(format!("{name}.o"))),
        );
    }
    let bytes = |b: &[u8]| {
        object::File::parse(b)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    let expected = [
        0xd9, 0xfe, 0xd9, 0xff, 0xd9, 0xf0, 0xd9, 0xf3, 0xd9, 0xf1, 0xd9, 0xf9, 0xd9, 0xfd, 0xd9,
        0xf8, 0xd9, 0xf5, 0xdf, 0xe0, 0xdd, 0xd8,
    ];
    assert_eq!(bytes(&fs::read(d.join("encoding.S.o")).unwrap()), expected);
    assert_eq!(
        bytes(
            &keel_asm::x86::assemble_object(include_str!("x86_x87_numerical/encoding.S")).unwrap()
        ),
        expected
    );
    common::command(Command::new("python3").arg(d.join("run.py")).arg("--seal"));
    common::command(Command::new("python3").arg(d.join("run.py")).arg("--check"));
    let victim = d.join("v1-Baseline-Fast-normal-raw.o");
    let original = fs::read(&victim).unwrap();
    fs::write(&victim, b"tamper").unwrap();
    assert!(
        !Command::new("python3")
            .arg(d.join("run.py"))
            .arg("--check")
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(victim, original).unwrap();
    for flag in ["-O", "-OO"] {
        fs::write(d.join("results.json"), b"stale").unwrap();
        assert!(
            !Command::new("python3")
                .arg(flag)
                .arg(d.join("run.py"))
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(!d.join("results.json").exists());
    }
    if export.is_none() {
        fs::remove_dir_all(d).unwrap();
    }
}
