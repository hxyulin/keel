use object::{Object, ObjectSection};
use std::{fs, io::Write, path::Path, process::Command};
fn command(c: &mut Command) {
    let o = c.output().unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/private-aggregate-evidence/native");
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("commands.jsonl"))
        .unwrap();
    let argv: Vec<_> = std::iter::once(c.get_program())
        .chain(c.get_args())
        .map(|s| s.to_str().unwrap())
        .collect();
    writeln!(
        log,
        "{{\"argv\":{argv:?},\"cwd\":{:?},\"returncode\":{},\"stdout\":{:?},\"stderr\":{:?}}}",
        c.get_current_dir()
            .map(|p| p.to_str().unwrap())
            .unwrap_or("."),
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
    .unwrap();
    assert!(
        o.status.success(),
        "{c:?}: {}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
fn text(path: &Path) -> Vec<u8> {
    let b = fs::read(path).unwrap();
    let obj = object::File::parse(b.as_slice()).unwrap();
    obj.sections()
        .find(|s| matches!(s.name(), Ok(".text" | "__text")))
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
#[test]
fn real_c_private_transport_both_routes_native_and_debug() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/private-aggregate-evidence/native");
    fs::create_dir_all(&dir).unwrap();
    let source = include_str!("fixtures/private_aggregate.c");
    fs::write(dir.join("fixture.c"), source).unwrap();
    fs::write(dir.join("oracle.c"), "extern long aggregate_transport(long);\nint main(void) { for (long x=-100; x<=100; ++x) if (aggregate_transport(x)!=38*x+163) return 1; return 0; }\n").unwrap();
    command(Command::new("clang").current_dir(&dir).args([
        "fixture.c",
        "oracle.c",
        "-o",
        "clang-oracle",
    ]));
    command(&mut Command::new(dir.join("clang-oracle")));
    for target in [
        keel_ir::Target::Aarch64AppleDarwin,
        keel_ir::Target::X86_64LinuxGnu,
    ] {
        let mut module = keel_c::compile(
            source,
            "fixture.c",
            keel_c::Options {
                target,
                debug_info: false,
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_opt::optimize(&mut module).unwrap();
        let before = module.to_string();
        let sigs: Vec<_> = ["addressed", "public_pair", "pressure"]
            .iter()
            .map(|name| {
                module
                    .functions
                    .iter()
                    .find(|f| f.name == *name)
                    .unwrap()
                    .sig
                    .clone()
            })
            .collect();
        let changes = keel_opt::private_aggregate::run(&mut module);
        assert_eq!(
            changes, 2,
            "expected snapshot and result (mixed pressure excluded): {changes}\n{before}"
        );
        for (name, sig) in ["addressed", "public_pair", "pressure"].iter().zip(sigs) {
            assert_eq!(
                module
                    .functions
                    .iter()
                    .find(|f| f.name == *name)
                    .unwrap()
                    .sig,
                sig
            );
        }
        keel_ir::verify_module(&module).unwrap();
        // The transport stage is sound on its own; cleanup is a separate step.
        fs::write(
            dir.join(format!("{}-transport.keel", target.triple())),
            module.to_string(),
        )
        .unwrap();
        keel_opt::run_passes(
            &mut module,
            &[
                "stack-forward",
                "stack-promote",
                "dce",
                "internal-fast-calls",
            ],
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut debug = keel_c::compile(
            source,
            "fixture.c",
            keel_c::Options {
                target,
                debug_info: true,
            },
        )
        .unwrap();
        let debug_before = debug.to_string();
        assert_eq!(keel_opt::private_aggregate::run(&mut debug), 0);
        assert_eq!(debug_before, debug.to_string());
        let stem = target.triple();
        fs::write(dir.join(format!("{stem}-before.keel")), before).unwrap();
        fs::write(dir.join(format!("{stem}-after.keel")), module.to_string()).unwrap();
        for stage in ["transport", "after"] {
            let input = format!("{stem}-{stage}.keel");
            let direct = format!("{stem}-{stage}-direct.o");
            let assembly = format!("{stem}-{stage}.s");
            let via_text = format!("{stem}-{stage}-text.o");
            for (output, object) in [(&direct, true), (&assembly, false)] {
                let mut c = Command::new(env!("CARGO_BIN_EXE_keel"));
                c.current_dir(&dir).args([
                    "compile",
                    "--target",
                    stem,
                    "--verify-each",
                    &input,
                    "-o",
                    output,
                ]);
                if object {
                    c.arg("--emit-object");
                }
                command(&mut c);
            }
            command(
                Command::new(env!("CARGO_BIN_EXE_keel"))
                    .current_dir(&dir)
                    .args(["asm", "--target", stem, &assembly, "-o", &via_text]),
            );
            assert_eq!(text(&dir.join(&direct)), text(&dir.join(&via_text)));
            if (cfg!(all(target_arch = "aarch64", target_os = "macos"))
                && target == keel_ir::Target::Aarch64AppleDarwin)
                || (cfg!(all(target_arch = "x86_64", target_os = "linux"))
                    && target == keel_ir::Target::X86_64LinuxGnu)
            {
                for obj in [&direct, &via_text] {
                    command(Command::new("clang").current_dir(&dir).args([
                        obj,
                        "oracle.c",
                        "-o",
                        "native-check",
                    ]));
                    command(&mut Command::new(dir.join("native-check")));
                }
            }
        }
    }
}

fn fault_module(pairs: usize, prefix: usize) -> keel_ir::Module {
    let params = [vec!["i64"; prefix], vec!["ptr byval [2 x i64]"; pairs]].concat();
    let block = (0..prefix + pairs)
        .map(|i| format!("v{i}:{}", if i < prefix { "i64" } else { "ptr" }))
        .collect::<Vec<_>>()
        .join(",");
    let args = [
        vec!["v3"; prefix],
        (0..pairs)
            .map(|i| {
                if i == 0 {
                    "v0"
                } else if i == pairs - 1 {
                    "v2"
                } else {
                    "v1"
                }
            })
            .collect(),
    ]
    .concat()
    .join(",");
    // The sink need not read anything: ABI transport itself must read all bytes.
    let source = format!(
        "func @sink({}) -> i64 noinline local {{\nblock0({block}):\n v{} = iconst.i64 0\n return v{}\n}}\nfunc @entry(ptr,ptr,ptr) -> i64 {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = iconst.i64 0\n v4 = call @sink({args})\n return v4\n}}",
        params.join(","),
        prefix + pairs,
        prefix + pairs
    );
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

#[test]
fn distinct_guard_pages_preserve_first_source_fault() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/private-aggregate-evidence/native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("fault.c"),
        include_str!("fixtures/private_aggregate_fault.c"),
    )
    .unwrap();
    for target in [
        keel_ir::Target::Aarch64AppleDarwin,
        keel_ir::Target::X86_64LinuxGnu,
    ] {
        for (name, pairs, prefix, activation) in [
            ("mixed", 5, 0, 0),
            ("register", 3, 0, 1),
            ("sysv-mixed", 4, 0, 1),
            ("stack", 3, 8, 1),
        ] {
            let base = fault_module(pairs, prefix);
            let mut transformed = base.clone();
            assert_eq!(
                keel_opt::private_aggregate::run(&mut transformed),
                activation
            );
            keel_ir::verify_module(&transformed).unwrap();
            let mut observed = Vec::new();
            let mut stages = vec![("base", base.clone()), ("transport", transformed)];
            if name == "mixed" {
                // Negative control for the reviewed bug: explicitly snapshot
                // the first pair before ABI transport reads the last pair.
                // On ARM this MUST fault differently from the raw baseline.
                let control = base.to_string().replace(
                    "    v3 = iconst.i64 0",
                    "    v5 = load.i64 v0\n    v6 = iconst.i64 8\n    v7 = ptradd v0, v6\n    v8 = load.i64 v7\n    v3 = iconst.i64 0",
                );
                assert_ne!(control, base.to_string());
                let control = keel_ir::parse(&control).unwrap();
                keel_ir::verify_module(&control).unwrap();
                stages.push(("parameter-order-control", control));
            }
            for (stage, module) in stages {
                let stem = format!("fault-{}-{name}-{stage}", target.triple());
                let input = format!("{stem}.keel");
                fs::write(dir.join(&input), module.to_string()).unwrap();
                // No default optimization before either baseline or transformed IR.
                for object in [true, false] {
                    let output = format!("{stem}.{}", if object { "o" } else { "s" });
                    let mut c = Command::new(env!("CARGO_BIN_EXE_keel"));
                    c.current_dir(&dir).args([
                        "compile",
                        "--opt-tier",
                        "none",
                        "--target",
                        target.triple(),
                        &input,
                        "-o",
                        &output,
                    ]);
                    if object {
                        c.arg("--emit-object");
                    }
                    command(&mut c);
                }
                let direct = format!("{stem}.o");
                let via_text = format!("{stem}-text.o");
                command(
                    Command::new(env!("CARGO_BIN_EXE_keel"))
                        .current_dir(&dir)
                        .args([
                            "asm",
                            "--target",
                            target.triple(),
                            &format!("{stem}.s"),
                            "-o",
                            &via_text,
                        ]),
                );
                assert_eq!(text(&dir.join(&direct)), text(&dir.join(&via_text)));
                let native = (cfg!(all(target_arch = "aarch64", target_os = "macos"))
                    && target == keel_ir::Target::Aarch64AppleDarwin)
                    || (cfg!(all(target_arch = "x86_64", target_os = "linux"))
                        && target == keel_ir::Target::X86_64LinuxGnu);
                if native {
                    for obj in [&direct, &via_text] {
                        let exe = format!("{obj}-check");
                        command(
                            Command::new("clang")
                                .current_dir(&dir)
                                .args([obj, "fault.c", "-o", &exe]),
                        );
                        let mut codes = Vec::new();
                        for mode in 0..4 {
                            let result = Command::new(dir.join(&exe))
                                .arg(mode.to_string())
                                .output()
                                .unwrap();
                            let code = result.status.code().unwrap();
                            let expected = if mode == 1
                                || (name == "mixed"
                                    && stage != "parameter-order-control"
                                    && target == keel_ir::Target::Aarch64AppleDarwin)
                            {
                                15
                            } else {
                                11
                            };
                            assert_eq!(code, expected, "{stem} {obj} mode={mode}");
                            codes.push(code);
                        }
                        fs::write(
                            dir.join(format!("{exe}.faults.json")),
                            format!("{codes:?}\n"),
                        )
                        .unwrap();
                        if stage != "parameter-order-control" {
                            observed.push(codes);
                        }
                    }
                }
            }
            if !observed.is_empty() {
                assert!(observed.iter().all(|codes| codes == &observed[0]));
            }
        }
    }
}
