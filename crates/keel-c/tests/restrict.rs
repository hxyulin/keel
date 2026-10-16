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
            "int f(int *restrict p, int *restrict q) { int a=*p; q[1]=1; return a+*p; }",
            2,
        ),
    ] {
        let mut m = compile(source);
        keel_opt::run_passes(&mut m, &["memory-forward"]).unwrap();
        assert_eq!(loads(&m), expected, "{source}\n{m}");
    }
}
