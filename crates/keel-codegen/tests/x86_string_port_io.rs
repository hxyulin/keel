//! Source/crossbuild only: never execute generated target code or a guest.
use keel_asm_ir::x86::{CoreOpcode, CpuLevel, Features, Opcode, Repeat, StringKind};
use keel_asm_ir::{Item, Program};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use object::{Object, ObjectSection, ObjectSymbol};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn ids() -> Vec<Intrinsic> {
    Intrinsic::ALL
        .into_iter()
        .filter(|i| i.x86_string_port_io().is_some())
        .collect()
}
fn cases() -> Vec<(Intrinsic, usize, usize)> {
    ids()
        .into_iter()
        .flat_map(|id| {
            let (_, _, rep, _) = id.x86_string_port_io().unwrap();
            (0..if rep { 5 } else { 1 })
                .flat_map(move |count| (0..4).map(move |mode| (id, count, mode)))
        })
        .collect()
}
fn symbol(id: Intrinsic, count: usize, mode: usize) -> String {
    format!("stringio_{}_{}_{}", id as usize, count, mode)
}
fn recipes() -> String {
    let mut s = String::new();
    for (id, count, mode) in cases() {
        let (_, _, rep, owned) = id.x86_string_port_io().unwrap();
        let name = symbol(id, count, mode);
        s += &format!(
            "func @{name}(i16,ptr,i64,i64,ptr) {{\nentry(v0:i16,v1:ptr,v2:i64,v3:i64,v4:ptr):\n"
        );
        let mut n = 5;
        let mut op = |text: String| {
            let v = format!("v{n}");
            n += 1;
            s += &format!(" {v} = {text}\n");
            v
        };
        let count_value = if count == 0 {
            "v2".into()
        } else {
            op(format!(
                "iconst.i64 {}",
                [0, 0, 1i64 << 32, i64::MIN, -1][count]
            ))
        };
        let mut pressure = vec![];
        if mode == 3 {
            for i in 0..16 {
                let off = op(format!("iconst.i64 {}", 256 + 8 * i));
                let ptr = op(format!("ptradd v4,{off}"));
                let v = op(format!("load.i64 {ptr}"));
                pressure.push((ptr, v));
            }
        }
        let mut args = if owned {
            vec!["v3".to_string()]
        } else {
            vec![]
        };
        args.extend(["v0".to_string(), "v1".to_string()]);
        if rep {
            args.push(if mode == 3 && count == 0 {
                "v3".to_string()
            } else {
                count_value.clone()
            });
        }
        let types = id.info().returns;
        let results = (n..n + types.len())
            .map(|i| format!("v{i}"))
            .collect::<Vec<_>>();
        n += types.len();
        s += &format!(
            " {} = intrinsic {}({})\n",
            results.join(","),
            id.info().name,
            args.join(",")
        );
        if mode != 0 {
            let used = if mode == 2 {
                let r = (n..n + types.len())
                    .map(|i| format!("v{i}"))
                    .collect::<Vec<_>>();
                n += types.len();
                s += &format!(
                    " jump done({})\ndone({}):\n",
                    results.join(","),
                    r.iter()
                        .zip(types)
                        .map(|(r, t)| format!("{r}:{t}"))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                r
            } else {
                results
            };
            for (i, v) in used.iter().enumerate() {
                s += &format!(
                    " v{n} = iconst.i64 {}\n v{} = ptradd v4,v{n}\n store {v},v{}\n",
                    8 * i,
                    n + 1,
                    n + 1
                );
                n += 2;
            }
        }
        if mode != 2 {
            for (i, v) in ["v0", "v1", "v2", "v3"].iter().enumerate() {
                s += &format!(
                    " v{n} = iconst.i64 {}\n v{} = ptradd v4,v{n}\n store {v},v{}\n",
                    64 + 8 * i,
                    n + 1,
                    n + 1
                );
                n += 2;
            }
        }
        for (ptr, v) in pressure {
            s += &format!(" store {v},{ptr}\n");
        }
        s += " return\n}\n";
    }
    s
}
fn io(p: &Program) -> Vec<(bool, u16, bool)> {
    p.items
        .iter()
        .filter_map(|i| match i {
            Item::X86Instruction(ins) => match ins.opcode {
                Opcode::Core(CoreOpcode::In | CoreOpcode::Out) | Opcode::Call => {
                    panic!("unexpected extra scalar I/O or call")
                }
                Opcode::Core(CoreOpcode::String {
                    kind,
                    width,
                    repeat,
                    address32,
                    segment,
                }) => {
                    assert!(!ins.lock);
                    assert!(!address32);
                    assert_eq!(segment, None);
                    assert!(matches!(kind, StringKind::Ins | StringKind::Outs));
                    assert!(matches!(repeat, Repeat::None | Repeat::Rep));
                    Some((kind == StringKind::Outs, width, repeat == Repeat::Rep))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}
fn section(bytes: &[u8]) -> Vec<u8> {
    let f = object::File::parse(bytes).unwrap();
    f.section_by_name(".text").unwrap().data().unwrap().to_vec()
}
fn expected_bytes(out: bool, w: u16, rep: bool) -> Vec<u8> {
    let mut v = vec![];
    if w == 16 {
        v.push(0x66);
    }
    if rep {
        v.push(0xf3);
    }
    v.push(match (out, w == 8) {
        (false, true) => 0x6c,
        (false, false) => 0x6d,
        (true, true) => 0x6e,
        (true, false) => 0x6f,
    });
    v
}
fn principal_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| {
            let l = l.strip_prefix("rep ").unwrap_or(l);
            matches!(l, "insb" | "insw" | "insd" | "outsb" | "outsw" | "outsd")
        })
        .collect()
}
#[test]
fn exact_registry_signatures_and_all743_prefix() {
    assert!(Intrinsic::ALL.len() >= 767);
    let prefix = include_str!("x86_string_port_io/prefix.txt")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(prefix.len(), 743);
    for (n, id) in Intrinsic::ALL[..743].iter().enumerate() {
        assert_eq!(format!("{id:?} {}", *id as usize), prefix[n]);
    }
    assert_eq!(ids().len(), 24);
    for (n, id) in ids().iter().enumerate() {
        assert_eq!(*id as usize, 743 + n);
        let (out, w, rep, owned) = id.x86_string_port_io().unwrap();
        assert_eq!(
            (out, w, rep, owned),
            ((n % 6) >= 3, [8, 16, 32][n % 3], (n % 12) >= 6, n >= 12)
        );
        let name = format!(
            "x86_{}{}{w}{}",
            if rep { "rep_" } else { "" },
            if out { "outs" } else { "ins" },
            if owned { "_owned_terminal" } else { "" }
        );
        assert_eq!(id.info().name, name);
        assert_eq!(Intrinsic::from_name(&name), Some(*id));
        let mut params = if owned { vec![Type::I64] } else { vec![] };
        params.extend([Type::I16, Type::Ptr]);
        if rep {
            params.push(Type::I64);
        }
        let mut returns = vec![Type::Ptr];
        if rep {
            returns.push(Type::I64);
        }
        if owned {
            returns.extend([Type::I64, Type::I64]);
        }
        assert_eq!(id.info().params, params);
        assert_eq!(id.info().returns, returns);
        assert!(!id.info().pure);
        assert!(id.is_x86_target());
    }
    for name in ["x86_ins64", "x86_outs64", "x86_rep_ins64", "x86_rep_outs64"] {
        assert!(Intrinsic::from_name(name).is_none());
    }
}
#[test]
fn exhaustive_source_only_matrix() {
    check_source_matrix(true);
}

#[test]
fn source_matrix_boundary_smoke() {
    check_source_matrix(false);
}

fn check_source_matrix(exhaustive: bool) {
    let source = recipes();
    let raw = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let mut optimized = raw.clone();
    keel_opt::optimize(&mut optimized).unwrap();
    keel_ir::verify_module(&optimized).unwrap();
    let export = exhaustive
        .then(|| std::env::var_os("KEEL_STRING_IO_EXPORT"))
        .flatten()
        .map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("recipes.ir"), source).unwrap();
        fs::write(d.join("optimized.ir"), optimized.to_string()).unwrap();
    }
    let tiny = Pools {
        caller_saved: &[6, 7],
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
    let expected = cases()
        .iter()
        .map(|(id, _, _)| {
            let (o, w, r, _) = id.x86_string_port_io().unwrap();
            (o, w, r)
        })
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 288);
    let mut config = 0;
    let mut checked_lines = std::collections::HashSet::new();
    for (stage, m) in [("raw", &raw), ("opt", &optimized)] {
        let mut count = 0;
        for f in &m.functions {
            for (_, i) in f.layout_insts() {
                if let InstData::Intrinsic { id, .. } = f.inst_data(i) {
                    assert!(id.x86_string_port_io().is_some());
                    assert!(f.inst_data(i).is_pinned());
                    assert_eq!(f.inst_data(i).memory_effects(), MemoryEffects::Barrier);
                    count += 1;
                }
            }
        }
        assert_eq!(count, 288, "all outputs dead and zero count retained");
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
            for selection in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    for (pool_index, pool) in [&keel_codegen::x86_64::REGISTERS, &tiny, &empty]
                        .into_iter()
                        .enumerate()
                    {
                        if !exhaustive
                            && !(feature_index == 1
                                && ((selection == SelectionTier::Baseline
                                    && allocation == AllocationTier::Fast
                                    && pool_index == 0)
                                    || (selection == SelectionTier::Full
                                        && allocation == AllocationTier::Full
                                        && pool_index == 2)))
                        {
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
                        let p =
                            keel_codegen::x86_64::compile_program_with_pipeline(m, pool, &pipeline)
                                .unwrap();
                        assert_eq!(io(&p), expected);
                        let object =
                            keel_asm::x86::assemble_program_object_with_features(&p, features)
                                .unwrap();
                        assert_eq!(
                            object,
                            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                        );
                        let lines = principal_lines(&text);
                        assert_eq!(lines.len(), 288);
                        for (line, &(out, w, rep)) in lines.iter().zip(&expected) {
                            // Whole-object parity is checked for every configuration above.
                            // The literal byte witness depends only on this instruction.
                            if checked_lines.insert(line.to_string()) {
                                let isolated = format!(".intel_syntax noprefix\n.text\n{line}\n");
                                let b = keel_asm::x86::assemble_object(&isolated).unwrap();
                                assert_eq!(section(&b), expected_bytes(out, w, rep), "{line}");
                            }
                        }
                        assert!(
                            !text.lines().any(|l| matches!(l.trim(), "cld" | "std")),
                            "no implicit DF normalization"
                        );
                        if let Some(d) = &export {
                            fs::write(d.join(format!("matrix-{config:03}.s")), text).unwrap();
                            fs::write(d.join(format!("matrix-{config:03}.o")), object).unwrap();
                        }
                        config += 1;
                    }
                }
            }
        }
        eprintln!("string I/O {stage} matrix complete: {config} configurations");
    }
    assert_eq!(config, if exhaustive { 144 } else { 4 });
    assert_eq!(checked_lines.len(), 12);
}
#[test]
fn malformed_types_and_memory_ordering() {
    for id in ids() {
        let mut m = keel_ir::parse(&recipes()).unwrap();
        // A valid per-ID function with all correctly inferred result types.
        m.functions.retain(|f| f.name == symbol(id, 0, 1));
        assert_eq!(m.functions.len(), 1);
        let f = &mut m.functions[0];
        let inst = f
            .layout_insts()
            .find_map(|(_, i)| matches!(f.inst_data(i), InstData::Intrinsic { .. }).then_some(i))
            .unwrap();
        let args = match f.inst_data(inst) {
            InstData::Intrinsic { args, .. } => args.clone(),
            _ => unreachable!(),
        };
        for index in 0..args.len() {
            let mut bad = m.clone();
            let f = &mut bad.functions[0];
            let wrong = *args
                .iter()
                .find(|&&v| f.value_type(v) != id.info().params[index])
                .unwrap();
            if let InstData::Intrinsic { args, .. } = f.inst_data_mut(inst) {
                args[index] = wrong;
            }
            assert!(keel_ir::verify_module(&bad).is_err());
            assert!(keel_codegen::x86_64::compile(&bad).is_err());
        }
        for remove in [true, false] {
            let mut bad = m.clone();
            if let InstData::Intrinsic { args, .. } = bad.functions[0].inst_data_mut(inst) {
                if remove {
                    args.pop();
                } else {
                    args.push(args[0]);
                }
            }
            assert!(keel_ir::verify_module(&bad).is_err());
            assert!(keel_codegen::x86_64::compile(&bad).is_err());
        }
        let (_, _, rep, owned) = id.x86_string_port_io().unwrap();
        let args = if owned {
            if rep { "v3,v0,v1,v2" } else { "v3,v0,v1" }
        } else if rep {
            "v0,v1,v2"
        } else {
            "v0,v1"
        };
        let results = (5..5 + id.info().returns.len())
            .map(|n| format!("v{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let source = format!(
            "func @ordered(i16,ptr,i64,i64,ptr)->i64 {{\nentry(v0:i16,v1:ptr,v2:i64,v3:i64,v4:ptr):\n store v2,v4\n {results} = intrinsic {}({args})\n v10 = load.i64 v4\n return v10\n}}\n",
            id.info().name
        );
        let mut m = keel_ir::parse(&source).unwrap();
        keel_opt::optimize(&mut m).unwrap();
        let text = m.to_string();
        assert!(text.find("store").unwrap() < text.find("intrinsic").unwrap());
        assert!(text.find("intrinsic").unwrap() < text.find("load.i64").unwrap());
    }
}
fn body(object: &[u8], name: &str) -> Vec<u8> {
    let f = object::File::parse(object).unwrap();
    let s = f
        .symbols()
        .find(|s| s.name().is_ok_and(|n| n == name))
        .unwrap();
    let sec = f.section_by_index(s.section_index().unwrap()).unwrap();
    let n = (s.address() - sec.address()) as usize;
    sec.data().unwrap()[n..n + s.size() as usize].to_vec()
}
#[test]
fn unrelated_allocation_and_existing_region_restrictions_unchanged() {
    let plain =
        "func @plain(i64,i64)->i64 {\nentry(v0:i64,v1:i64):\n v2 = iadd v0,v1\n return v2\n}\n";
    let a = keel_ir::parse(plain).unwrap();
    let b = keel_ir::parse(&(plain.to_owned() + &recipes())).unwrap();
    let compile = |m: &keel_ir::Module| {
        keel_asm::x86::assemble_object(&keel_codegen::x86_64::compile(m).unwrap()).unwrap()
    };
    assert_eq!(body(&compile(&a), "plain"), body(&compile(&b), "plain"));
    assert!(keel_ir::rflags_region::Plan::parse("save; ins8 0 1; reapply; normal", 2, 0).is_err());
    let m = keel_ir::parse(
        "func @blocked(i64) {\nentry(v0:i64):\n v1 = intrinsic x86_rflags_swap(v0)\n return\n}\n",
    )
    .unwrap();
    assert!(keel_codegen::x86_64::compile(&m).is_err());
}
fn run(c: &mut Command) {
    let o = c.output().unwrap_or_else(|e| panic!("{c:?}: {e}"));
    assert!(
        o.status.success(),
        "{c:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
}
fn oracles(d: &Path, stem: &str, text: &str) -> (Vec<u8>, Vec<u8>) {
    let clang = std::env::var("KEEL_STRING_IO_CLANG").expect("explicit Clang path required");
    let gas = std::env::var("KEEL_STRING_IO_GAS").expect("explicit GNU as path required");
    let input = d.join(format!("{stem}.s"));
    fs::write(&input, text).unwrap();
    // LLVM emits F3 66 for REP word I/O; GAS/Keel emit 66 F3. Force
    // the same prefix order in LLVM without changing any instruction: the
    // explicit 66 prefix changes the following dword mnemonic to word width.
    let clang_text = text
        .lines()
        .map(|line| match line.trim() {
            "rep insw" => ".byte 0x66\nrep insd",
            "rep outsw" => ".byte 0x66\nrep outsd",
            _ => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let clang_input = d.join(format!("{stem}.clang.s"));
    fs::write(&clang_input, clang_text).unwrap();
    let co = d.join(format!("{stem}.clang.o"));
    let go = d.join(format!("{stem}.gas.o"));
    run(Command::new(clang)
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-c",
            "-x",
            "assembler",
        ])
        .arg(&clang_input)
        .arg("-o")
        .arg(&co));
    run(Command::new(gas).arg("--64").arg(&input).arg("-o").arg(&go));
    (
        section(&fs::read(co).unwrap()),
        section(&fs::read(go).unwrap()),
    )
}
#[test]
#[ignore = "requires exported compile matrix and explicitly forced Clang/GAS"]
fn forced_clang_gas() {
    let d =
        PathBuf::from(std::env::var_os("KEEL_STRING_IO_EXPORT").expect("matrix export required"));
    let output = d.join("oracles");
    fs::create_dir_all(&output).unwrap();
    for n in 0..144 {
        let source = fs::read_to_string(d.join(format!("matrix-{n:03}.s"))).unwrap();
        // Normalize only assembler-specific NOP alignment padding. Every
        // compiler-generated instruction and checked boundary remains present.
        let source = source
            .lines()
            .filter(|l| !l.trim().starts_with(".p2align"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let ours = section(&keel_asm::x86::assemble_object(&source).unwrap());
        let (c, g) = oracles(&output, &format!("matrix-{n:03}"), &source);
        assert!(
            ours == c,
            "Clang stream {n}: first mismatch {:?}, lengths {} {}",
            ours.iter().zip(&c).position(|(a, b)| a != b),
            ours.len(),
            c.len()
        );
        assert!(ours == g, "GAS stream {n}");
    }
    for id in ids() {
        let (out, w, rep, _) = id.x86_string_port_io().unwrap();
        let text = format!(
            ".intel_syntax noprefix\n.text\n{}{}{}\n",
            if rep { "rep " } else { "" },
            if out { "outs" } else { "ins" },
            match w {
                8 => "b",
                16 => "w",
                32 => "d",
                _ => panic!(),
            }
        );
        let (c, g) = oracles(&output, &format!("literal-{}", id as usize), &text);
        let bytes = expected_bytes(out, w, rep);
        assert_eq!(c, bytes);
        assert_eq!(g, bytes);
        // Independently retain LLVM's unconstrained mnemonic encoding too.
        let native = output.join(format!("literal-{}.native-clang.o", id as usize));
        let clang = std::env::var("KEEL_STRING_IO_CLANG").unwrap();
        run(Command::new(clang)
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-c",
                "-x",
                "assembler",
            ])
            .arg(output.join(format!("literal-{}.s", id as usize)))
            .arg("-o")
            .arg(&native));
        let mut native_bytes = bytes.clone();
        if rep && w == 16 {
            native_bytes.swap(0, 1);
        }
        assert_eq!(section(&fs::read(native).unwrap()), native_bytes);
    }
}
