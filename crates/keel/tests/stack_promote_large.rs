//! Large private-field promotion through the CLI, both object routes and an
//! independent unsigned C oracle. Run Cargo under suite/common/fast_opt_locked.py;
//! its lease covers all sequential compiler and native child commands here.
use object::{Object, ObjectSection};
use std::{fs, io::Write, path::Path, process::Command};

fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn run(dir: &Path, cmd: &mut Command) -> String {
    let output = cmd.output().unwrap();
    let args = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|s| json(&s.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(",");
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("commands.jsonl"))
        .unwrap();
    writeln!(
        log,
        "{{\"argv\":[{args}],\"cwd\":{},\"exit_code\":{},\"stdout\":{},\"stderr\":{}}}",
        json(&cmd.get_current_dir().unwrap_or(dir).display().to_string()),
        output.status.code().unwrap_or(-1),
        json(&String::from_utf8_lossy(&output.stdout)),
        json(&String::from_utf8_lossy(&output.stderr))
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn keel(dir: &Path, args: &[&str]) -> String {
    run(
        dir,
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(dir)
            .args(args),
    )
}
fn text(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).unwrap();
    let object = object::File::parse(bytes.as_slice()).unwrap();
    object
        .sections()
        .find(|s| matches!(s.name(), Ok(".text" | "__text")))
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn memory(f: &keel_ir::Function) -> usize {
    f.layout_insts()
        .filter(|(_, i)| {
            matches!(
                f.inst_data(*i),
                keel_ir::InstData::Load { .. } | keel_ir::InstData::Store { .. }
            )
        })
        .count()
}

fn fixture() -> String {
    let mut module = String::new();
    for kind in ["chain", "join", "loop"] {
        let mut s = format!(
            "func @large_{kind}(i64,i64,i64,ptr) -> i64 {{\n ss0 = stack_slot 16, align 8\nblock0(v0: i64,v1: i64,v2: i64,v3: ptr):\n v10 = stack_addr ss0\n v11 = iconst.i64 2\n v12 = ptradd v10,v11\n v13 = iconst.i64 4\n v14 = ptradd v10,v13\n v15 = iconst.i64 8\n v16 = ptradd v10,v15\n"
        );
        let initialize = |base: usize, first: usize| {
            format!(
                " v{first} = trunc.i8 v{base}\n v{} = trunc.i16 v{base}\n v{} = trunc.i32 v{base}\n store v{first},v10\n store v{},v12\n store v{},v14\n store v{base},v16\n",
                first + 1,
                first + 2,
                first + 1,
                first + 2
            )
        };
        if kind == "join" {
            s += " brif v2,block1,block2\nblock1:\n";
            s += &initialize(1, 20);
            s += " jump block3\nblock2:\n v23 = iconst.i64 123\n v24 = iadd v1,v23\n";
            s += &initialize(24, 25);
            s += " jump block3\n";
        } else {
            s += &initialize(1, 20);
            s += " jump block3\n";
        }
        for b in 3..323 {
            s += &format!("block{b}:\n jump block{}\n", b + 1);
        }
        if kind == "loop" {
            s += "block323:\n v30 = iconst.i64 0\n jump block324(v30)\nblock324(v31: i64):\n v32 = icmp.ult v31,v0\n brif v32,block325,block326\nblock325:\n";
            for (index, (ty, addr, step)) in [
                ("i8", 10, 1),
                ("i16", 12, 3),
                ("i32", 14, 7),
                ("i64", 16, 17),
            ]
            .into_iter()
            .enumerate()
            {
                let v = 40 + index * 3;
                s += &format!(
                    " v{v} = load.{ty} v{addr}\n v{} = iconst.{ty} {step}\n v{} = iadd v{v},v{}\n store v{},v{addr}\n",
                    v + 1,
                    v + 2,
                    v + 1,
                    v + 2
                );
            }
            s += " v60 = iconst.i64 1\n v61 = iadd v31,v60\n jump block324(v61)\nblock326:\n";
        } else {
            s += "block323:\n";
        }
        for (index, (ty, addr)) in [("i8", 10), ("i16", 12), ("i32", 14), ("i64", 16)]
            .into_iter()
            .enumerate()
        {
            let v = 100 + index * 5;
            s += &format!(" v{v} = load.{ty} v{addr}\n");
            if ty != "i64" {
                s += &format!(" v{} = uext.i64 v{v}\n", v + 1);
            } else {
                s += &format!(" v{} = iadd v{v},v30\n", v + 1);
            }
            // v30 is zero at the exit for every kind (loop entry dominates).
            s += &format!(
                " v{} = iconst.i64 {}\n v{} = ptradd v3,v{}\n store v{},v{}\n",
                v + 2,
                index * 8,
                v + 3,
                v + 2,
                v + 1,
                v + 3
            );
        }
        s += " v130 = iadd v101,v106\n v131 = iadd v111,v116\n v132 = iadd v130,v131\n return v132\n}\n";
        if kind != "loop" {
            s = s.replace("block323:\n", "block323:\n v30 = iconst.i64 0\n");
        }
        module += &s;
    }
    module
}

#[test]
fn large_cfg_object_routes_cross_targets_and_arm_native_oracle() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stack-promote-large");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("commands.jsonl"), "").unwrap();
    let source = fixture();
    fs::write(dir.join("before.keel"), &source).unwrap();
    let before = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&before).unwrap();
    assert!(before.functions.iter().all(|f| f.blocks().count() > 256));
    keel(
        &dir,
        &[
            "opt",
            "--verify-each",
            "-p",
            "stack-promote",
            "before.keel",
            "-o",
            "small.keel",
        ],
    );
    assert_eq!(
        fs::read_to_string(dir.join("small.keel")).unwrap(),
        before.to_string()
    );
    keel(
        &dir,
        &[
            "opt",
            "--verify-each",
            "-p",
            "stack-promote-large,phi-simplify,dce",
            "before.keel",
            "-o",
            "promoted.keel",
        ],
    );
    let promoted = keel_ir::parse(&fs::read_to_string(dir.join("promoted.keel")).unwrap()).unwrap();
    keel_ir::verify_module(&promoted).unwrap();
    for (before, after) in before.functions.iter().zip(&promoted.functions) {
        assert_eq!(
            after.blocks().count(),
            before.blocks().count(),
            "codegen must still see a large CFG"
        );
        assert_eq!(memory(after), 4, "only four caller-output stores remain");
        assert!(memory(before) > memory(after));
        assert!(
            !after
                .layout_insts()
                .any(|(_, i)| matches!(after.inst_data(i), keel_ir::InstData::Load { .. }))
        );
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        fs::write(
            dir.join("oracle.c"),
            include_str!("fixtures/stack_promote_large_oracle.c"),
        )
        .unwrap();
        run(
            &dir,
            Command::new("clang")
                .current_dir(&dir)
                .args(["-O2", "-fno-lto", "-c", "oracle.c", "-o", "oracle.o"]),
        );
    }
    for variant in ["before", "promoted"] {
        for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
            for selector in ["none", "all"] {
                let stem = format!("{variant}-{target}-{selector}");
                let source = format!("{variant}.keel");
                keel(
                    &dir,
                    &[
                        "compile",
                        "--target",
                        target,
                        "--isel",
                        selector,
                        "--emit-object",
                        &source,
                        "-o",
                        &format!("{stem}.o"),
                    ],
                );
                keel(
                    &dir,
                    &[
                        "compile",
                        "--target",
                        target,
                        "--isel",
                        selector,
                        &source,
                        "-o",
                        &format!("{stem}.s"),
                    ],
                );
                keel(
                    &dir,
                    &[
                        "asm",
                        "--target",
                        target,
                        &format!("{stem}.s"),
                        "-o",
                        &format!("{stem}-text.o"),
                    ],
                );
                // The routes have identical code, so execute the direct object only.
                assert_eq!(
                    text(&dir.join(format!("{stem}.o"))),
                    text(&dir.join(format!("{stem}-text.o")))
                );
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                if target == "aarch64-apple-darwin" {
                    let executable = format!("{stem}-check");
                    run(
                        &dir,
                        Command::new("clang").current_dir(&dir).args([
                            "oracle.o",
                            &format!("{stem}.o"),
                            "-o",
                            &executable,
                        ]),
                    );
                    assert_eq!(
                        run(&dir, Command::new(dir.join(&executable)).current_dir(&dir)).trim(),
                        "1344 by-field large-CFG oracle cases passed"
                    );
                }
            }
        }
    }
}
