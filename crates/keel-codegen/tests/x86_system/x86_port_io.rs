//! Compile-only proof. Never execute emitted I/O, ELF, or guest code.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use object::{Object, ObjectSection};
use std::{fs, path::Path, process::Command};

fn ids() -> Vec<Intrinsic> {
    Intrinsic::ALL
        .into_iter()
        .filter(|i| i.x86_port_io().is_some())
        .collect()
}
fn source() -> String {
    let mut s = String::new();
    for (i, id) in ids().iter().enumerate() {
        let (out, w, imm) = id.x86_port_io().unwrap();
        for port in if imm { vec![0, 0xe0, 0xff] } else { vec![0] } {
            for mode in 0..4 {
                s += &format!(
                    "func @io_{i}_{port}_{mode}(ptr,ptr,ptr,ptr,ptr) {{\nentry(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr):\n"
                );
                s += &if imm {
                    format!(" v5 = iconst.i16 {port}\n")
                } else {
                    " v5 = load.i16 v0\n".into()
                };
                s += &format!(" v6 = load.i{w} v1\n");
                let data = if mode == 2 && w == 16 { "v5" } else { "v6" };
                if out {
                    s += &format!(" intrinsic {}(v5,{data})\n", id.info().name);
                } else {
                    s += &format!(" v7 = intrinsic {}(v5)\n", id.info().name);
                    if mode == 1 || mode == 3 {
                        s += " store v7,v2\n";
                    }
                    if mode == 2 {
                        s += &format!(" jump done(v7)\ndone(v8:i{w}):\n store v8,v2\n");
                    }
                }
                // Modes 0..2 keep inputs live. Mode 3 makes inputs die at
                // the I/O, allowing result homes to reuse input registers/slots.
                if mode != 3 {
                    s += " store v5,v3\n store v6,v4\n";
                }
                s += " return\n}\n";
            }
        }
    }
    s
}
fn text_bytes(bytes: &[u8]) -> Vec<u8> {
    let f = object::File::parse(bytes).unwrap();
    f.section_by_name(".text").unwrap().data().unwrap().to_vec()
}
fn io_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|s| matches!(s.split_whitespace().next(), Some("in" | "out")))
        .collect()
}
fn expected_io() -> Vec<Vec<u8>> {
    let mut expected = vec![];
    for id in ids() {
        let (out, w, imm) = id.x86_port_io().unwrap();
        for p in if imm { vec![0, 0xe0, 0xff] } else { vec![0] } {
            let mut bytes = vec![];
            if w == 16 {
                bytes.push(0x66);
            }
            bytes.push(match (out, w == 8, imm) {
                (false, true, true) => 0xe4,
                (false, false, true) => 0xe5,
                (false, true, false) => 0xec,
                (false, false, false) => 0xed,
                (true, true, true) => 0xe6,
                (true, false, true) => 0xe7,
                (true, true, false) => 0xee,
                (true, false, false) => 0xef,
            });
            if imm {
                bytes.push(p);
            }
            for _ in 0..4 {
                expected.push(bytes.clone());
            }
        }
    }
    expected
}
#[test]
fn registry_and_contract() {
    assert!(Intrinsic::ALL.len() >= 729);
    assert_eq!(&Intrinsic::ALL[717..729], ids().as_slice());
    assert_eq!(ids().len(), 12);
    for (n, id) in ids().iter().enumerate() {
        assert_eq!(*id as usize, 717 + n);
        let (out, w, imm) = id.x86_port_io().unwrap();
        assert_eq!((out, w, imm), ((n % 6) >= 3, [8, 16, 32][n % 3], n >= 6));
        let ty = match w {
            8 => Type::I8,
            16 => Type::I16,
            32 => Type::I32,
            _ => panic!(),
        };
        let name = format!(
            "x86_{}{w}{}",
            if out { "out" } else { "in" },
            if imm { "_imm" } else { "" }
        );
        assert_eq!(id.info().name, name);
        assert_eq!(Intrinsic::from_name(&name), Some(*id));
        assert_eq!(
            id.info().params,
            if out {
                vec![Type::I16, ty]
            } else {
                vec![Type::I16]
            }
        );
        assert_eq!(id.info().returns, if out { vec![] } else { vec![ty] });
        assert!(!id.info().pure);
        assert!(id.is_x86_target());
    }
    for name in ["x86_in64", "x86_out64", "x86_in64_imm", "x86_out64_imm"] {
        assert!(Intrinsic::from_name(name).is_none());
    }
    for (name, id) in [
        ("x86_ins8", Intrinsic::X86Ins8),
        ("x86_outs8", Intrinsic::X86Outs8),
    ] {
        assert_eq!(Intrinsic::from_name(name), Some(id));
        assert!(id.x86_port_io().is_none());
    }
}

#[test]
fn raw_opt_pipeline_matrix() {
    let raw = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let mut opt = raw.clone();
    keel_opt::optimize(&mut opt).unwrap();
    keel_ir::verify_module(&opt).unwrap();
    let tiny = Pools {
        caller_saved: &[6],
        callee_saved: &[3],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let expected = expected_io();
    assert_eq!(expected.len(), 96);
    let export = std::env::var_os("KEEL_PORT_IO_MATRIX_DIR").map(std::path::PathBuf::from);
    if let Some(dir) = &export {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("recipes.ir"), source()).unwrap();
        fs::write(dir.join("optimized.ir"), opt.to_string()).unwrap();
    }
    let mut configurations = 0;
    for (stage_index, m) in [&raw, &opt].into_iter().enumerate() {
        let mut count = 0;
        for f in &m.functions {
            for (_, i) in f.layout_insts() {
                if let InstData::Intrinsic { id, .. } = f.inst_data(i) {
                    assert!(id.x86_port_io().is_some());
                    assert_eq!(f.inst_data(i).memory_effects(), MemoryEffects::Barrier);
                    assert!(f.inst_data(i).is_pinned());
                    count += 1;
                }
            }
        }
        assert_eq!(count, 96, "including dead IN");
        assert!(
            keel_codegen::aarch64::compile(m)
                .unwrap_err()
                .to_string()
                .contains("x86 architectural intrinsic")
        );
        for (feature_index, features) in [
            Features::NONE,
            Features::for_level(CpuLevel::V1),
            Features::for_level(CpuLevel::V2),
            Features::for_level(CpuLevel::V3),
        ]
        .into_iter()
        .enumerate()
        {
            for (selection_index, selection) in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ]
            .into_iter()
            .enumerate()
            {
                for (allocation_index, allocation) in [AllocationTier::Fast, AllocationTier::Full]
                    .into_iter()
                    .enumerate()
                {
                    for (pool_index, pool) in [&keel_codegen::x86_64::REGISTERS, &tiny, &empty]
                        .into_iter()
                        .enumerate()
                    {
                        // Neither the feature level nor the allocator changes
                        // the text. Cover each stage, selector and pool at
                        // NONE with alternating allocators, plus one V3 row.
                        let planned = (feature_index == 0
                            && allocation_index
                                == (stage_index + selection_index + pool_index) % 2)
                            || (feature_index == 3
                                && selection == SelectionTier::Full
                                && allocation == AllocationTier::Full
                                && pool_index == 0);
                        if !planned {
                            continue;
                        }
                        let selector = FeatureSelector {
                            inner: &selection,
                            features,
                        };
                        let pipeline = CodegenPipeline {
                            selector: &selector,
                            allocator: &allocation,
                        };
                        let text = keel_codegen::x86_64::compile_with_pipeline(m, pool, &pipeline)
                            .unwrap();
                        let program =
                            keel_codegen::x86_64::compile_program_with_pipeline(m, pool, &pipeline)
                                .unwrap();
                        let object = keel_asm::x86::assemble_program_object_with_features(
                            &program, features,
                        )
                        .unwrap();
                        assert_eq!(
                            object,
                            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                        );
                        let lines = io_lines(&text);
                        assert_eq!(lines.len(), expected.len(), "no extra I/O");
                        for (line, bytes) in lines.into_iter().zip(&expected) {
                            let obj = keel_asm::x86::assemble_object(&format!(
                                ".intel_syntax noprefix\n.text\n{line}\n"
                            ))
                            .unwrap();
                            assert_eq!(text_bytes(&obj), *bytes, "{line}");
                        }
                        if let Some(dir) = &export {
                            fs::write(dir.join(format!("matrix-{configurations:03}.s")), &text)
                                .unwrap();
                            fs::write(dir.join(format!("matrix-{configurations:03}.o")), &object)
                                .unwrap();
                        }
                        configurations += 1;
                    }
                }
            }
        }
    }
    assert_eq!(configurations, 20);
}

#[test]
fn malformed_controls_and_types_rejected() {
    for id in ids() {
        let (out, w, imm) = id.x86_port_io().unwrap();
        let call = if out {
            format!(" intrinsic {}(v0,v1)\n", id.info().name)
        } else {
            format!(" v2 = intrinsic {}(v0)\n", id.info().name)
        };
        if imm {
            for port in [
                "iconst.i16 -1",
                "iconst.i16 256",
                "iconst.i16 65535",
                "iconst.i16 65536",
                "iconst.i16 -65536",
                "iconst.i32 224",
                "load.i16 v3",
            ] {
                let s = format!(
                    "func @bad(ptr) {{\nentry(v3:ptr):\n v0 = {port}\n v1 = iconst.i{w} 1\n{call} return\n}}\n"
                );
                if let Ok(m) = keel_ir::parse(&s) {
                    assert!(keel_ir::verify_module(&m).is_err(), "{s}");
                }
            }
            let s =
                format!("func @dynamic(i16,i{w}) {{\nentry(v0:i16,v1:i{w}):\n{call} return\n}}\n");
            let m = keel_ir::parse(&s).unwrap();
            assert!(keel_ir::verify_module(&m).is_err());
            assert!(keel_codegen::x86_64::compile(&m).is_err());
        }
        for port_ty in ["i8", "i32", "i64", "ptr"] {
            let s = format!(
                "func @bad({port_ty},i{w}) {{\nentry(v0:{port_ty},v1:i{w}):\n{call} return\n}}\n"
            );
            if let Ok(m) = keel_ir::parse(&s) {
                assert!(keel_ir::verify_module(&m).is_err(), "{s}");
            }
        }
        for args in ["", "v0,v1,v1"] {
            let call = format!(
                " {}intrinsic {}({args})",
                if out { "" } else { "v2 = " },
                id.info().name
            );
            let s =
                format!("func @bad(i16,i{w}) {{\nentry(v0:i16,v1:i{w}):\n{call}\n return\n}}\n");
            if let Ok(m) = keel_ir::parse(&s) {
                assert!(keel_ir::verify_module(&m).is_err(), "{s}");
            }
        }
        if out {
            let wrong = if w == 8 { 16 } else { 8 };
            let s = format!(
                "func @bad() {{\nentry:\n v0 = iconst.i16 224\n v1 = iconst.i{wrong} 1\n{call} return\n}}\n"
            );
            if let Ok(m) = keel_ir::parse(&s) {
                assert!(keel_ir::verify_module(&m).is_err(), "{s}");
            }
        }
    }
}

fn run(cmd: &mut Command) {
    let output = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    assert!(
        output.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn assemble_oracles(dir: &Path, source: &str, stem: &str) -> (Vec<u8>, Vec<u8>) {
    let input = dir.join(format!("{stem}.s"));
    fs::write(&input, source).unwrap();
    let clang = std::env::var("KEEL_PORT_IO_CLANG").expect("set KEEL_PORT_IO_CLANG explicitly");
    let gnu = std::env::var("KEEL_PORT_IO_GNU_AS").expect("set KEEL_PORT_IO_GNU_AS explicitly");
    let co = dir.join(format!("{stem}.clang.o"));
    let go = dir.join(format!("{stem}.gnu.o"));
    run(Command::new(clang)
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-c",
            "-x",
            "assembler",
        ])
        .arg(&input)
        .arg("-o")
        .arg(&co));
    run(Command::new(gnu).arg("--64").arg(&input).arg("-o").arg(&go));
    (
        text_bytes(&fs::read(co).unwrap()),
        text_bytes(&fs::read(go).unwrap()),
    )
}

#[test]
#[ignore = "requires explicitly forced Clang and GNU assembler; compile only"]
fn forced_clang_gnu_exact_crossoracle() {
    let dir = std::env::var_os("KEEL_PORT_IO_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .expect("set evidence directory");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("recipes.ir"), source()).unwrap();
    let mut m = keel_ir::parse(&source()).unwrap();
    let expected = expected_io();
    // Cross-check complete generated instruction streams for every matrix
    // configuration, including forced spills. Only alignment padding differs.
    let matrix = dir.join("matrix");
    for n in 0..144 {
        let generated = fs::read_to_string(matrix.join(format!("matrix-{n:03}.s"))).unwrap();
        let text = generated
            .lines()
            .filter(|l| !l.trim().starts_with(".p2align"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(io_lines(&text).len(), 96);
        let ours = text_bytes(&keel_asm::x86::assemble_object(&text).unwrap());
        let (clang, gnu) = assemble_oracles(&dir, &text, &format!("matrix-{n:03}"));
        assert!(
            ours == clang,
            "complete instruction stream Clang matrix {n}"
        );
        assert!(ours == gnu, "complete instruction stream GNU matrix {n}");
    }
    for stage in ["raw", "opt"] {
        if stage == "opt" {
            keel_opt::optimize(&mut m).unwrap();
        }
        fs::write(dir.join(format!("{stage}.ir")), m.to_string()).unwrap();
        // GNU and LLVM choose different NOP padding for .p2align. Remove
        // alignment directives from every oracle input; retain every instruction.
        let generated = keel_codegen::x86_64::compile(&m).unwrap();
        let text = generated
            .lines()
            .filter(|l| !l.trim().starts_with(".p2align"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let ours = text_bytes(&keel_asm::x86::assemble_object(&text).unwrap());
        let (clang, gnu) = assemble_oracles(&dir, &text, stage);
        assert_eq!(ours, clang, "complete body Clang {stage}");
        assert_eq!(ours, gnu, "complete body GNU {stage}");
        let lines = io_lines(&text);
        assert_eq!(lines.len(), 96);
        for (n, (line, bytes)) in lines.iter().zip(&expected).enumerate() {
            let (clang, gnu) = assemble_oracles(
                &dir,
                &format!(".intel_syntax noprefix\n.text\n{line}\n"),
                &format!("{stage}-io-{n}"),
            );
            assert_eq!(clang, *bytes, "{line}");
            assert_eq!(gnu, *bytes, "{line}");
        }
    }
}

#[test]
fn ordinary_memory_order_is_preserved() {
    for id in ids() {
        let (out, w, _) = id.x86_port_io().unwrap();
        let call = if out {
            format!(" intrinsic {}(v1,v2)\n", id.info().name)
        } else {
            format!(" v3 = intrinsic {}(v1)\n", id.info().name)
        };
        let src = format!(
            "func @ordered(ptr,i{w})->i{w} {{\nentry(v0:ptr,v2:i{w}):\n v1 = iconst.i16 224\n store v2,v0\n{call} v4 = load.i{w} v0\n return v4\n}}\n"
        );
        let mut m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        keel_opt::optimize(&mut m).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let text = m.to_string();
        assert!(text.find("store").unwrap() < text.find("intrinsic").unwrap());
        assert!(text.find("intrinsic").unwrap() < text.find(&format!("load.i{w}")).unwrap());
    }
}
