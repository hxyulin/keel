use keel_ir::{InstData, Target};

fn compile(source: &str) -> keel_ir::Module {
    let m = keel_c::compile(source, "restrict.c", Default::default()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

#[test]
fn only_definition_parameter_object_qualifiers_supply_contracts() {
    for param in [
        "int *restrict p",
        "int *__restrict p",
        "int *__restrict__ p",
        "R p",
        "P restrict p",
        "int *const restrict p",
    ] {
        let m = compile(&format!(
            "typedef int *P; typedef int *restrict R; int f({param}) {{ return *p; }}"
        ));
        assert_eq!(m.functions[0].restrict_params, [0], "{param}");
    }
    for source in [
        "int f(int *restrict p); int f(int *p) { return *p; }",
        "typedef int *restrict R; int f(R *p) { return **p; }",
        "int f(int *p) { int *restrict q = p; return *q; }",
        "int f(int (*restrict p)(void)) { return p(); }",
        "int f(int p[restrict 4]) { return p[0]; }",
    ] {
        let m = compile(source);
        assert!(m.functions[0].restrict_params.is_empty(), "{source}");
    }
    let m = compile("int f(int *p); int f(int *restrict p) { return *p; } int f(int *p);");
    assert_eq!(m.functions[0].restrict_params, [0]);
}

fn loads(m: &keel_ir::Module) -> usize {
    let f = &m.functions[0];
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
        .count()
}

#[test]
fn c_restrict_removes_one_redundant_load_with_codegen_smoke() {
    for (qualifier, expected) in [("restrict", 1), ("", 2)] {
        let mut m = compile(&format!(
            "int f(int *{qualifier} p, int *{qualifier} q) {{ int a=*p; *q=9; return a+*p; }}"
        ));
        assert_eq!(loads(&m), 2);
        keel_opt::run_passes(&mut m, &["memory-forward"]).unwrap();
        assert_eq!(loads(&m), expected, "{m}");
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            keel_codegen::compile(&m, target).unwrap();
        }
    }
}

#[test]
fn inline_drops_callee_scope_and_retains_caller_indices() {
    let mut m = compile(
        "static int inner(int *restrict p, int *restrict q) { int a=*p; *q=7; return a+*p; } int f(int x, int *restrict p, int *restrict q) { return x+inner(p,q); }",
    );
    keel_opt::run_passes(&mut m, &["inline"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    assert_eq!(f.restrict_params, [1, 2]);
    assert!(
        !f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. }))
    );
    let mut m = compile(
        "static int inner(int *restrict p, int *restrict q) { int a=*p; *q=7; return a+*p; } int f(int *p, int *q) { return inner(p,q); }",
    );
    keel_opt::run_passes(&mut m, &["inline", "memory-forward"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    assert!(f.restrict_params.is_empty());
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
            .count(),
        2
    );
}

#[test]
fn c_aliases_and_barriers_keep_loads() {
    for (source, expected) in [
        (
            "int f(const int *restrict p, const int *restrict q) { return *p+*q; }",
            2,
        ),
        (
            "void use(int *); int f(int *restrict p, int *restrict q) { int a=*p; use(p); *q=1; return a+*p; }",
            2,
        ),
        (
            "int *escaped; int f(int *restrict p, int *restrict q) { int a=*p; escaped=p; *q=1; return a+*p; }",
            2,
        ),
        (
            "int f(volatile int *restrict p, int *restrict q) { int a=*p; *q=1; return a+*p; }",
            2,
        ),
        (
            // Recognized constant derivations retain the entry contract.
            "int f(int *restrict p, int *restrict q) { int a=*p; q[1]=1; return a+*p; }",
            1,
        ),
        (
            "int f(int *restrict p, int *restrict q, int n) { int a=*p; q[n]=1; return a+*p; }",
            2,
        ),
    ] {
        let mut m = compile(source);
        keel_opt::run_passes(&mut m, &["memory-forward"]).unwrap();
        assert_eq!(loads(&m), expected, "{source}\n{m}");
    }
}

#[test]
fn hidden_sret_is_never_a_restrict_parameter() {
    for (qualifier, expected) in [("restrict", vec![1, 2]), ("", vec![])] {
        let source = format!(
            "struct S {{ int x; }}; struct S f(int *{qualifier} p, int *{qualifier} q) {{ struct S s={{*p}}; *q=9; s.x+=*p; return s; }}"
        );
        for target in Target::ALL {
            let mut m = keel_c::compile(
                &source,
                "restrict-sret.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(matches!(
                m.functions[0].sig.params[0].purpose,
                keel_ir::Purpose::Sret(_)
            ));
            assert_eq!(m.functions[0].restrict_params, expected);
            keel_ir::verify_module(&m).unwrap();
            keel_opt::optimize(&mut m).unwrap();
            keel_ir::verify_module(&m).unwrap();
            assert_eq!(m.functions[0].restrict_params, expected);
        }
    }
}

#[test]
fn memory_forward_native_differential_against_clang() {
    use std::path::Path;
    use std::process::{Command, Output};

    fn run(command: &mut Command) -> Output {
        let output = command
            .output()
            .expect("running native restrict test command");
        assert!(
            output.status.success(),
            "{command:?} failed: {:?}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn function_loads(module: &keel_ir::Module, name: &str) -> usize {
        let f = module.functions.iter().find(|f| f.name == name).unwrap();
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
            .count()
    }

    let target = Target::host().expect("native restrict test requires a supported host");
    let source = include_str!("fixtures/restrict_memory_forward.c");
    let mut module = compile(source);
    // R1-1: prove the consumer changed this exact module before native codegen.
    // The ordinary/default pipeline alone does not run memory-forward.
    let shapes = [
        ("separate", 2, 1),
        ("readonly", 2, 2),
        ("readonly_store", 3, 2),
        ("ordinary", 2, 2),
        ("observed", 2, 2),
        ("call_barrier", 2, 2),
    ];
    for &(name, before, _) in &shapes {
        assert_eq!(function_loads(&module, name), before, "{name}\n{module}");
    }
    let before_ir = module.to_string();
    keel_opt::run_passes(&mut module, &["memory-forward"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for &(name, _, after) in &shapes {
        assert_eq!(function_loads(&module, name), after, "{name}\n{module}");
    }

    // Cargo's target tmp directory is ignored by the repository. Retain source,
    // IR, assembly and binaries there so the executed elimination is reviewable.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("restrict-memory-forward");
    std::fs::create_dir_all(&directory).unwrap();
    let input = directory.join("input.c");
    std::fs::write(&input, source).unwrap();
    std::fs::write(directory.join("before.keel"), before_ir).unwrap();
    std::fs::write(directory.join("memory-forward.keel"), module.to_string()).unwrap();
    let mut shapes_text = String::from("function before_loads after_loads\n");
    for (name, before, after) in shapes {
        shapes_text.push_str(&format!("{name} {before} {after}\n"));
    }
    std::fs::write(directory.join("load-shapes.txt"), shapes_text).unwrap();

    let mut references = Vec::new();
    for optimization in ["-O0", "-O2"] {
        let reference = directory.join(format!("clang{optimization}"));
        run(Command::new("clang")
            .args(["-std=c11", optimization, "-o"])
            .arg(&reference)
            .arg(&input));
        let output = run(&mut Command::new(reference));
        std::fs::write(
            directory.join(format!("clang{optimization}.stdout")),
            &output.stdout,
        )
        .unwrap();
        references.push(output);
    }
    assert_eq!(references[0].stdout, references[1].stdout);
    assert!(!references[0].stdout.is_empty());

    for default_pipeline in [false, true] {
        let name = if default_pipeline {
            "memory-forward-then-default"
        } else {
            "memory-forward"
        };
        let mut native = module.clone();
        if default_pipeline {
            keel_opt::optimize(&mut native).unwrap();
            keel_ir::verify_module(&native).unwrap();
        }
        std::fs::write(directory.join(format!("{name}.keel")), native.to_string()).unwrap();
        let assembly = directory.join(format!("{name}.s"));
        std::fs::write(&assembly, keel_codegen::compile(&native, target).unwrap()).unwrap();
        let executable = directory.join(name);
        run(Command::new("clang")
            .arg("-o")
            .arg(&executable)
            .arg(&assembly));
        let actual = run(&mut Command::new(executable));
        std::fs::write(directory.join(format!("{name}.stdout")), &actual.stdout).unwrap();
        for reference in &references {
            assert_eq!(actual.status.code(), reference.status.code(), "{name}");
            assert_eq!(actual.stdout, reference.stdout, "{name}");
        }
    }
}
