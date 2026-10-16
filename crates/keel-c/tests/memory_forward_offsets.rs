use keel_ir::{InstData, Module, Target};
use std::process::{Command, Output};

fn run(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn loads(m: &Module, name: &str) -> usize {
    let f = m.functions.iter().find(|f| f.name == name).unwrap();
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
        .count()
}

#[test]
fn explicit_forwarding_offsets_matches_clang_and_both_targets() {
    let source = include_str!("fixtures/memory_forward_offsets.c");
    let mut m = keel_c::compile(source, "memory_forward_offsets.c", Default::default()).unwrap();
    // Simplify frontend index scaling without folding loads or running default passes.
    keel_opt::run_passes(&mut m, &["canonicalize", "instcombine"]).unwrap();
    let names = [
        ("fields", 1),
        ("derived", 1),
        ("ordinary", 2),
        ("mixed", 2),
        ("unknown", 2),
        ("readonly", 2),
    ];
    for (name, _) in names {
        assert_eq!(loads(&m, name), 2, "before {name}\n{m}");
    }
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-forward-offsets");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("before.keel"), m.to_string()).unwrap();
    keel_opt::run_passes(&mut m, &["memory-forward"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for (name, after) in names {
        assert_eq!(loads(&m, name), after, "after {name}\n{m}");
    }
    std::fs::write(dir.join("after.keel"), m.to_string()).unwrap();
    for target in Target::ALL {
        keel_codegen::compile(&m, target).unwrap();
    }
    let c = dir.join("input.c");
    std::fs::write(&c, source).unwrap();
    let mut references = Vec::new();
    for level in ["-O0", "-O2"] {
        let exe = dir.join(format!("clang{level}"));
        run(Command::new("clang")
            .args(["-std=c11", level])
            .arg(&c)
            .arg("-o")
            .arg(&exe));
        references.push(run(&mut Command::new(exe)).stdout);
    }
    assert_eq!(references[0], references[1]);
    for default in [false, true] {
        let mut native = m.clone();
        if default {
            keel_opt::optimize(&mut native).unwrap();
        }
        let asm = dir.join(format!("native-{default}.s"));
        std::fs::write(
            &asm,
            keel_codegen::compile(&native, Target::host().unwrap()).unwrap(),
        )
        .unwrap();
        let exe = dir.join(format!("native-{default}"));
        run(Command::new("clang").arg(&asm).arg("-o").arg(&exe));
        let actual = run(&mut Command::new(exe));
        assert_eq!(actual.stdout, references[0]);
        std::fs::write(dir.join(format!("native-{default}.stdout")), actual.stdout).unwrap();
    }
}

#[test]
fn native_modular_pointer_reducer_preserves_wrapped_alias() {
    // All actual accesses target the live object. Only non-dereferenced IR
    // ptradd intermediates wrap; no C out-of-bounds pointer arithmetic is used.
    let mut m = keel_ir::parse(
        "func @wrapped(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 9223372036854775807
 v3 = ptradd v0, v2
 v4 = ptradd v3, v2
 v5 = iconst.i64 2
 v6 = ptradd v4, v5
 v7 = load.i64 v0
 store v1, v6
 v8 = load.i64 v0
 v9 = iadd v7, v8
 return v9
}",
    )
    .unwrap();
    keel_opt::run_passes(&mut m, &["memory-forward"]).unwrap();
    assert_eq!(loads(&m, "wrapped"), 2);
    for target in Target::ALL {
        keel_codegen::compile(&m, target).unwrap();
    }
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-forward-wrap");
    std::fs::create_dir_all(&dir).unwrap();
    let driver = dir.join("driver.c");
    std::fs::write(&driver, "extern unsigned long long wrapped(unsigned long long *, unsigned long long); int main(void) { unsigned long long x=11; unsigned long long r=wrapped(&x,29); return !(r==40 && x==29); }").unwrap();
    let asm = dir.join("wrapped.s");
    std::fs::write(
        &asm,
        keel_codegen::compile(&m, Target::host().unwrap()).unwrap(),
    )
    .unwrap();
    let exe = dir.join("wrapped");
    run(Command::new("clang")
        .arg(&driver)
        .arg(&asm)
        .arg("-o")
        .arg(&exe));
    run(&mut Command::new(exe));
}
