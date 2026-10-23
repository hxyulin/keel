//! Narrow [prior.high64, current.low64] selection; arbitrary bits, no FP math.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
};
use std::{fs, path::PathBuf, process::Command};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const LAYOUTS: [&str; 9] = [
    "aliasa",
    "aliasb",
    "distinct",
    "spilla",
    "spillb",
    "spillresult",
    "allspills",
    "same",
    "samespill",
];
struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-shuffle-window"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let r = |i: usize| Loc::FReg(input.pools.float_caller_saved[i]);
        let s = Loc::Stack;
        let locs = match input.func.name.rsplit('_').next().unwrap() {
            "aliasa" => [r(0), r(1), r(0)],
            "aliasb" => [r(0), r(1), r(1)],
            "distinct" | "same" => [r(0), r(1), r(2)],
            "spilla" => [s(0), r(1), r(2)],
            "spillb" => [r(0), s(0), r(2)],
            "spillresult" => [r(0), r(1), s(0)],
            "allspills" | "samespill" => [s(0), s(2), s(4)],
            _ => unreachable!(),
        };
        a.spill_slots = 6;
        for (i, loc) in locs.into_iter().enumerate() {
            a.locs[0][i] = Some(loc);
        }
        a
    }
}

fn fixture(controlled: bool) -> (String, String) {
    fixture_layouts(controlled, &LAYOUTS)
}
fn fixture_layouts(controlled: bool, layouts: &[&str]) -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(include_str!("vector_shuffle/header.c"));
    let mut cases = String::from("static const struct Case cases[]={\n");
    for shape in ["i64x2", "f64x2"] {
        for layout in if controlled { layouts } else { &["live"][..] } {
            let name = format!("window_{shape}_{layout}");
            let same = layout.starts_with("same");
            if controlled {
                ir += &format!(
                    "func @{name}({shape},{shape}) -> {shape} {{\nblock0(v0: {shape},v1: {shape}):\n v2 = vshuffle v0,{},[1,2]\n return v2\n}}\n",
                    if same { "v0" } else { "v1" }
                );
                c += &format!(
                    "extern V_{shape} {name}(V_{shape},V_{shape});\nstatic void call_{name}(const void *p,const void *q,void *o){{V_{shape} a,b,r;memcpy(&a,p,16);memcpy(&b,q,16);r={name}(a,b);memcpy(o,&r,16);}}\n"
                );
            } else {
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr):\n v3 = load.{shape} align 1 v0\n v4 = load.{shape} align 1 v1\n v5 = vshuffle v3,v4,[1,2]\n store align 1 v5,v2\n v6 = iconst.i64 16\n v7 = ptradd v2,v6\n store align 1 v3,v7\n v8 = iconst.i64 32\n v9 = ptradd v2,v8\n store align 1 v4,v9\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,const void*,void*);\n");
            }
            cases += &format!(
                "{{\"{name}\",{call},{},{}}},\n",
                u8::from(same),
                u8::from(!controlled),
                call = if controlled {
                    format!("call_{name}")
                } else {
                    name.clone()
                }
            );
        }
    }
    (
        ir,
        c + &cases + "};\n" + include_str!("vector_shuffle/oracle.c"),
    )
}

fn compile(
    arm: bool,
    source: &str,
    sel: SelectionTier,
    alloc: &dyn RegisterAllocator,
    spill: bool,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    compile_module(arm, &m, &sel, alloc, spill)
}
fn compile_module(
    arm: bool,
    m: &keel_ir::Module,
    selector: &dyn keel_codegen::InstructionSelector,
    alloc: &dyn RegisterAllocator,
    spill: bool,
) -> (String, Vec<u8>) {
    let pipeline = CodegenPipeline {
        selector,
        allocator: alloc,
    };
    if arm {
        let pools = if spill {
            &EMPTY
        } else {
            &keel_codegen::aarch64::REGISTERS
        };
        let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
        let p = keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
        assert_eq!(p.stats().text_bytes, 0);
        let obj = keel_asm::assemble_program_object(&p, Default::default()).unwrap();
        assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
        (text, obj)
    } else {
        let pools = if spill {
            &EMPTY
        } else {
            &keel_codegen::x86_64::REGISTERS
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
        let p = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
        assert_eq!(p.stats().text_bytes, 0);
        let obj = keel_asm::x86::assemble_program_object(&p).unwrap();
        assert_eq!(obj, keel_asm::x86::assemble_object(&text).unwrap());
        (text, obj)
    }
}
fn command(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(out.status.success(), "{c:?}: {out:?}");
}
fn oracle(arm: bool, stem: &str, source: &str, c: &str, asm: &str, obj: &[u8]) {
    let root = std::env::var_os("KEEL_SHUFFLE_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-shuffle-{}", std::process::id()))
        });
    let dir = root.join(if arm { "arm" } else { "x86" });
    fs::create_dir_all(&dir).unwrap();
    for (ext, bytes) in [
        ("keel", source.as_bytes()),
        ("c", c.as_bytes()),
        ("s", asm.as_bytes()),
        ("o", obj),
    ] {
        fs::write(dir.join(format!("{stem}.{ext}")), bytes).unwrap();
    }
    if !arm {
        fs::write(
            dir.join("run_native.py"),
            include_str!("vector_shuffle/run_native.py"),
        )
        .unwrap();
    }
    let native = if arm {
        cfg!(all(target_arch = "aarch64", target_os = "macos"))
    } else {
        cfg!(all(target_arch = "x86_64", target_os = "linux"))
    };
    if native {
        let exe = dir.join(format!("{stem}.native"));
        let mut cc = Command::new("clang");
        cc.args(["-O2", "-Wall", "-Wextra", "-Werror"]);
        if !arm {
            cc.args(["-march=x86-64", "-msse2", "-mno-avx"]);
        }
        command(
            cc.arg(dir.join(format!("{stem}.c")))
                .arg(dir.join(format!("{stem}.o")))
                .arg("-o")
                .arg(&exe),
        );
        command(&mut Command::new(exe));
    } else {
        command(
            Command::new("clang")
                .args([
                    "-target",
                    if arm {
                        "arm64-apple-macos11"
                    } else {
                        "x86_64-unknown-linux-gnu"
                    },
                    "-ffreestanding",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-fsyntax-only",
                ])
                .arg(dir.join(format!("{stem}.c"))),
        );
    }
    // Check instruction bytes against an independent assembler as well as the
    // typed/text object equality above. The ELF/Mach-O containers may differ.
    let clang_obj = dir.join(format!("{stem}.clang.o"));
    command(
        Command::new("clang")
            .args([
                "-target",
                if arm {
                    "arm64-apple-macos11"
                } else {
                    "x86_64-unknown-linux-gnu"
                },
                "-c",
            ])
            .arg(dir.join(format!("{stem}.s")))
            .arg("-o")
            .arg(&clang_obj),
    );
    {
        use object::{Object, ObjectSection};
        let reference = fs::read(&clang_obj).unwrap();
        let name = if arm { "__text" } else { ".text" };
        let actual = object::File::parse(obj).unwrap();
        let expected = object::File::parse(reference.as_slice()).unwrap();
        assert_eq!(
            actual.section_by_name(name).unwrap().data().unwrap(),
            expected.section_by_name(name).unwrap().data().unwrap()
        );
    }
    if std::env::var_os("KEEL_SHUFFLE_EVIDENCE").is_some() {
        fs::write(dir.join(format!("{stem}.json")),format!("{{\"passed\":true,\"native_executed\":{native},\"typed_text_objects_equal\":true,\"clang_text_equal\":true}}\n")).unwrap();
        fs::write(dir.join(format!("{stem}.html")),format!("<!doctype html><meta charset=utf-8><title>{stem}</title><h1>{stem}</h1><p>Passed. Native execution: {native}. Typed/text objects match. Clang instruction bytes match.</p>")).unwrap();
    }
    if std::env::var_os("KEEL_SHUFFLE_EVIDENCE").is_none() {
        for ext in ["keel", "c", "s", "o", "native", "clang.o"] {
            let path = dir.join(format!("{stem}.{ext}"));
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
    }
}

#[test]
fn native_window_controlled_aliases_and_spills() {
    let (source, c) = fixture(true);
    for arm in [true, false] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let (text, obj) = compile(arm, &source, sel, &Controlled, false);
            assert_eq!(
                text.matches(if arm { "ext\t" } else { "shufpd\t" }).count(),
                if arm { 18 } else { 14 },
                "{text}"
            );
            if !arm {
                // The four same-input windows are single-source permutations.
                assert_eq!(text.matches("pshufd\t").count(), 4, "{text}");
            }
            oracle(
                arm,
                &format!("controlled-{sel:?}"),
                &source,
                &c,
                &text,
                &obj,
            );
        }
    }
}
#[test]
fn native_window_preserves_both_live_sources_and_exact_loads() {
    let (source, c) = fixture(false);
    for arm in [true, false] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for spill in [false, true] {
                    let (text, obj) = compile(arm, &source, sel, &alloc, spill);
                    assert_eq!(
                        text.matches(if arm { "ext\t" } else { "shufpd\t" }).count(),
                        2,
                        "{text}"
                    );
                    oracle(
                        arm,
                        &format!("live-{sel:?}-{alloc:?}-{spill}"),
                        &source,
                        &c,
                        &text,
                        &obj,
                    );
                }
            }
        }
    }
}
#[test]
fn general_shuffles_use_packed_arm_and_supported_x86_recipes() {
    for (shape, lanes) in [
        ("i64x2", "0,2"),
        ("i64x2", "1,3"),
        ("f64x2", "2,1"),
        ("i32x4", "1,2,3,4"),
        ("f32x4", "1,2,3,4"),
    ] {
        let source = format!(
            "func @other({shape},{shape}) -> {shape} {{\nblock0(v0: {shape},v1: {shape}):\n v2 = vshuffle v0,v1,[{lanes}]\n return v2\n}}\n"
        );
        for arm in [true, false] {
            for sel in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                let (text, _) = compile(arm, &source, sel, &AllocationTier::Full, false);
                if arm {
                    match (shape, lanes) {
                        ("i64x2", "0,2") => assert!(text.contains("zip1\t"), "{text}"),
                        ("i64x2", "1,3") => assert!(text.contains("zip2\t"), "{text}"),
                        _ => assert!(text.contains("ext\t") || text.contains("ins\t"), "{text}"),
                    }
                    assert!(!text.contains(" = stack_addr"), "{text}");
                } else if shape.ends_with("64x2") {
                    assert!(text.contains("shufpd\t"), "{text}");
                    assert!(!text.contains(" = stack_addr"), "{text}");
                } else {
                    assert!(!text.contains("shufpd\t"), "{text}");
                }
            }
        }
    }
}

#[test]
fn custom_fixed_and_reused_boundary_edits_preserve_shuffle_operands() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Constrained(bool);
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "window-fixed-reuse"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Baseline.select(f, t, folds);
            let pools = match t {
                Target::Aarch64 => keel_codegen::aarch64::REGISTERS,
                Target::X86 => keel_codegen::x86_64::REGISTERS,
            };
            let inst = f.layout_insts().next().unwrap().1;
            s.set_constraint(
                inst,
                ValuePart::low(Value::new(0)),
                OperandKind::Use,
                Constraint::Fixed(if self.0 {
                    Loc::FReg(pools.float_caller_saved[0])
                } else {
                    Loc::Stack(0)
                }),
            );
            s.set_constraint(
                inst,
                ValuePart::low(Value::new(1)),
                OperandKind::Use,
                if self.0 {
                    Constraint::Register
                } else {
                    Constraint::Any
                },
            );
            s.set_constraint(
                inst,
                ValuePart::low(Value::new(2)),
                OperandKind::Def,
                Constraint::Reuse(ValuePart::low(Value::new(1))),
            );
            s
        }
    }
    struct Edited(bool);
    impl RegisterAllocator for Edited {
        fn name(&self) -> &'static str {
            "window-boundary-edits"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let mut a = AllocationTier::Full.allocate(input);
            if self.0 {
                for (i, r) in [0, 1, 1].into_iter().enumerate() {
                    a.locs[0][i] = Some(Loc::FReg(input.pools.float_caller_saved[r]));
                }
                return a;
            }
            a.spill_slots = 6;
            for i in 0..3 {
                a.locs[0][i] = Some(Loc::Stack(i as u32 * 2));
            }
            let inst = input.func.layout_insts().next().unwrap().1;
            let x = Loc::Stack(0);
            let y = Loc::Stack(2);
            let edit = a.edits.instruction_mut(inst);
            edit.uses = vec![
                (ValuePart::low(Value::new(0)), x),
                (ValuePart::low(Value::new(1)), y),
            ];
            edit.defs = vec![(ValuePart::low(Value::new(2)), y)];
            edit.after = vec![(Loc::Stack(4), y)];
            a
        }
    }
    let (source, c) = fixture_layouts(true, &["distinct"]);
    let m = keel_ir::parse(&source).unwrap();
    for arm in [true, false] {
        for registers in [false, true] {
            let (text, obj) =
                compile_module(arm, &m, &Constrained(registers), &Edited(registers), false);
            assert_eq!(
                text.matches(if arm { "ext\t" } else { "shufpd\t" }).count(),
                2
            );
            oracle(
                arm,
                if registers {
                    "fixed-register-reuse"
                } else {
                    "fixed-stack-reuse"
                },
                &source,
                &c,
                &text,
                &obj,
            );
        }
    }
}

#[test]
fn load_shuffle_store_debug_lines_keep_their_order() {
    use keel_ir::EntityRef;
    let (source, c) = fixture(false);
    let mut m = keel_ir::parse(&source).unwrap();
    m.files.push("window.c".into());
    for f in &mut m.functions {
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        for (n, inst) in insts.into_iter().enumerate() {
            f.set_inst_loc(
                inst,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line: 10 + n as u32,
                    col: 1,
                    scope: None,
                }),
            );
        }
    }
    for arm in [true, false] {
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let (text, obj) = compile_module(arm, &m, &sel, &AllocationTier::Full, true);
            let mut cursor = 0;
            for _ in 0..2 {
                for line in 10..=13 {
                    let marker = format!(".loc\t1 {line} 1");
                    cursor += text[cursor..]
                        .find(&marker)
                        .unwrap_or_else(|| panic!("missing {marker}: {text}"))
                        + marker.len();
                    if line == 12 {
                        let next = text[cursor..].find(".loc\t").unwrap();
                        assert!(
                            text[cursor..cursor + next].contains(if arm {
                                "ext\t"
                            } else {
                                "shufpd\t"
                            }),
                            "{text}"
                        );
                    }
                }
            }
            oracle(arm, &format!("debug-{sel:?}"), &source, &c, &text, &obj);
        }
    }
}

#[test]
fn arm_narrow_window_all_lanes_aliases_spills_and_boundary_edits() {
    for controlled in [false, true] {
        let (ir, c) = fixture(controlled);
        // Keep the i64 fixtures above unchanged, and reinterpret their arbitrary
        // bytes here as four integer lanes. Check all 16 result bytes.
        let ir = ir.replace("f64x2", "i64x2");
        let ir = ir
            .split("func @window_i64x2_")
            .skip(1)
            .take(if controlled { 9 } else { 1 })
            .map(|part| format!("func @window_i64x2_{part}"))
            .collect::<String>()
            .replace("i64x2", "i32x4")
            .replace("[1,2]", "[3,4,5,6]");
        let c = c
            .replace("V_i64x2", "V_i32x4")
            .replace("window_i64x2", "window_i32x4")
            .replace("typedef u64 V_i32x4", "typedef unsigned V_i32x4");
        // Drop the unused f64 cases/declarations by regenerating a single shape fixture.
        let c = c
            .lines()
            .filter(|line| !line.contains("window_f64x2"))
            .collect::<Vec<_>>()
            .join("\n")
            .replace(
                "memcpy(expected,x+8,8);memcpy(expected+8,c->same?x:y,8);",
                "memcpy(expected,x+12,4);memcpy(expected+4,c->same?x:y,12);",
            );
        for sel in [SelectionTier::Baseline, SelectionTier::Full] {
            for spill in [false, true] {
                if controlled && spill {
                    continue;
                }
                for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                    let allocator: &dyn RegisterAllocator =
                        if controlled { &Controlled } else { &alloc };
                    let (text, obj) = compile(true, &ir, sel, allocator, spill);
                    assert_eq!(
                        text.matches("#12").count(),
                        if controlled { 9 } else { 1 },
                        "{text}"
                    );
                    oracle(
                        true,
                        &format!("narrow-{controlled}-{sel:?}-{alloc:?}-{spill}"),
                        &ir,
                        &c,
                        &text,
                        &obj,
                    );
                }
            }
        }
    }
}

#[test]
fn arm_narrow_custom_boundary_edits() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Constrained(bool);
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "window-fixed-reuse"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut s = SelectionTier::Baseline.select(f, t, folds);
            let pools = match t {
                Target::Aarch64 => keel_codegen::aarch64::REGISTERS,
                Target::X86 => keel_codegen::x86_64::REGISTERS,
            };
            let inst = f.layout_insts().next().unwrap().1;
            s.set_constraint(
                inst,
                ValuePart::low(Value::new(0)),
                OperandKind::Use,
                Constraint::Fixed(if self.0 {
                    Loc::FReg(pools.float_caller_saved[0])
                } else {
                    Loc::Stack(0)
                }),
            );
            s.set_constraint(
                inst,
                ValuePart::low(Value::new(1)),
                OperandKind::Use,
                if self.0 {
                    Constraint::Register
                } else {
                    Constraint::Any
                },
            );
            s.set_constraint(
                inst,
                ValuePart::low(Value::new(2)),
                OperandKind::Def,
                Constraint::Reuse(ValuePart::low(Value::new(1))),
            );
            s
        }
    }
    struct Edited(bool);
    impl RegisterAllocator for Edited {
        fn name(&self) -> &'static str {
            "window-boundary-edits"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let mut a = AllocationTier::Full.allocate(input);
            if self.0 {
                for (i, r) in [0, 1, 1].into_iter().enumerate() {
                    a.locs[0][i] = Some(Loc::FReg(input.pools.float_caller_saved[r]));
                }
                return a;
            }
            a.spill_slots = 6;
            for i in 0..3 {
                a.locs[0][i] = Some(Loc::Stack(i as u32 * 2));
            }
            let inst = input.func.layout_insts().next().unwrap().1;
            let x = Loc::Stack(0);
            let y = Loc::Stack(2);
            let edit = a.edits.instruction_mut(inst);
            edit.uses = vec![
                (ValuePart::low(Value::new(0)), x),
                (ValuePart::low(Value::new(1)), y),
            ];
            edit.defs = vec![(ValuePart::low(Value::new(2)), y)];
            edit.after = vec![(Loc::Stack(4), y)];
            a
        }
    }
    let (source, c) = fixture_layouts(true, &["distinct"]);
    let source = source
        .split("func @window_f64x2")
        .next()
        .unwrap()
        .replace("i64x2", "i32x4")
        .replace("[1,2]", "[3,4,5,6]");
    let c = c
        .lines()
        .filter(|line| !line.contains("window_f64x2"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("V_i64x2", "V_i32x4")
        .replace("window_i64x2", "window_i32x4")
        .replace("typedef u64 V_i32x4", "typedef unsigned V_i32x4")
        .replace(
            "memcpy(expected,x+8,8);memcpy(expected+8,c->same?x:y,8);",
            "memcpy(expected,x+12,4);memcpy(expected+4,c->same?x:y,12);",
        );
    let m = keel_ir::parse(&source).unwrap();
    for arm in [true] {
        for registers in [false, true] {
            let (text, obj) =
                compile_module(arm, &m, &Constrained(registers), &Edited(registers), false);
            assert_eq!(
                text.matches(if arm { "ext\t" } else { "shufpd\t" }).count(),
                1
            );
            oracle(
                arm,
                if registers {
                    "narrow-fixed-register-reuse"
                } else {
                    "narrow-fixed-stack-reuse"
                },
                &source,
                &c,
                &text,
                &obj,
            );
        }
    }
}
