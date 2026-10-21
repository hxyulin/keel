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
        let sigs: Vec<_> = ["addressed", "public_pair"]
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
        assert!(
            changes >= 3,
            "expected snapshot, pressure, result: {changes}\n{before}"
        );
        for (name, sig) in ["addressed", "public_pair"].iter().zip(sigs) {
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
