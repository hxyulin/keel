//! Standalone integration of the unregistered result-home prototype.
#[path = "../../keel-opt/src/aggregate_return.rs"]
mod aggregate_return;
use object::{Object, ObjectSection};
use std::{fs, path::Path, process::Command};
fn command(c: &mut Command) {
    let o = c.output().unwrap();
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
fn real_c_both_object_routes_and_native_oracle() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/aggregate-return-20261021/native");
    fs::create_dir_all(&dir).unwrap();
    let source = include_str!("fixtures/aggregate_return.c");
    fs::write(dir.join("fixture.c"), source).unwrap();
    fs::write(dir.join("oracle.c"), "extern long aggregate_result(long), aggregate_snapshot(long), aggregate_capture(long), aggregate_large(long);\nint main(void) { for (long x = -100; x <= 100; ++x) { if (aggregate_result(x) != 4*x+7 || aggregate_snapshot(x) != 13*x+27 || aggregate_capture(x) != 2*x+10 || aggregate_large(x) != 6*x+8) return 1; } return 0; }\n").unwrap();
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
        let load_counts = |m: &keel_ir::Module| -> Vec<usize> {
            ["aggregate_snapshot", "aggregate_capture", "aggregate_large"]
                .iter()
                .map(|name| {
                    let f = m.functions.iter().find(|f| f.name == *name).unwrap();
                    f.layout_insts()
                        .filter(|&(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Load { .. }))
                        .count()
                })
                .collect()
        };
        let old_loads = load_counts(&module);
        let copies = aggregate_return::run(&mut module);
        assert_eq!(
            copies,
            3,
            "actual C candidate coverage for {}:\n{before}",
            target.triple()
        );
        assert_eq!(
            old_loads
                .iter()
                .zip(load_counts(&module))
                .map(|(old, new)| old - new)
                .collect::<Vec<_>>(),
            [2, 2, 3]
        );
        keel_ir::verify_module(&module).unwrap();
        keel_opt::run_passes(&mut module, &["dce"]).unwrap();
        let stem = target.triple();
        fs::write(dir.join(format!("{stem}-before.keel")), before).unwrap();
        fs::write(dir.join(format!("{stem}-after.keel")), module.to_string()).unwrap();
        let input = format!("{stem}-after.keel");
        let direct = format!("{stem}-direct.o");
        let assembly = format!("{stem}.s");
        let via_text = format!("{stem}-text.o");
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
