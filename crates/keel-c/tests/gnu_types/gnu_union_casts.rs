//! GNU union casts construct an independent aggregate rvalue.
//! Matching rules: https://gcc.gnu.org/onlinedocs/gcc/Cast-to-Union.html.
//! Clang Linux64 witnesses also pin qualifiers and bit-fields. GCC uses compatible
//! member types (gcc/c/c-typeck.cc, build_c_cast and comptypes_internal).
use keel_ir::Target;
use std::{fs, process::Command};

const SOURCE: &str = r#"
typedef unsigned long long u64;
union ibs_fetch_ctl { u64 val; struct { u64 enable:1; u64 count:63; }; };
union ibs_fetch_ctl kernel_cast(u64 config) {
    union ibs_fetch_ctl fetch_ctl = (union ibs_fetch_ctl)config;
    return fetch_ctl;
}
union Number { int i; double d; unsigned char storage[24]; };
union Pointer { int *p; const int *cp; void (*fn)(void); };
struct Payload { int words[3]; };
union PayloadUnion { struct Payload payload; unsigned char storage[40]; };
struct Large { long words[5]; };
union LargeUnion { struct Large large; unsigned char storage[64]; };
union Nested { union Number n; unsigned char storage[32]; };
union Bits { int i:3; unsigned char storage[8]; };
union Number static_int = (union Number)17;
union Number static_double = (union Number)2.5;
int global = 31;
union Pointer static_pointer = (union Pointer)&global;
union PayloadUnion static_payload = (union PayloadUnion)(struct Payload){{3, 5, 7}};
union Nested static_nested = (union Nested)(union Number)19;
union Bits static_bits = (union Bits)7;
int calls;
int next(void) { ++calls; return 37; }
int *next_pointer(int *p) { ++calls; return p; }
struct Payload next_payload(void) {
    struct Payload p = {{11, 13, 17}};
    ++calls;
    return p;
}
int preserve(int *snapshot, struct Payload *source) {
    source->words[0] = 99;
    return snapshot[0];
}
long sum(union LargeUnion u) {
    return u.large.words[0] + u.large.words[4];
}
union LargeUnion wrap(struct Large l) { return (union LargeUnion)l; }
void callback(void) { ++calls; }
int union_cast_test(void) {
    union ibs_fetch_ctl ctl = kernel_cast(0xfedcba9876543210ULL);
    if (ctl.val != 0xfedcba9876543210ULL) return 1;
    union Number n = (union Number)next();
    if (n.i != 37 || calls != 1) return 2;
    n = (union Number)2.5;
    if (n.d != 2.5) return 3;
    union Pointer p = (union Pointer)next_pointer(&global);
    if (p.p != &global || *p.p != 31 || calls != 2) return 4;
    union PayloadUnion u = (union PayloadUnion)next_payload();
    if (calls != 3 || u.payload.words[0] != 11 || u.payload.words[2] != 17) return 5;
    struct Payload source = {{23, 29, 31}};
    if (preserve(((union PayloadUnion)source).payload.words, &source) != 23
        || source.words[0] != 99) return 6;
    struct Large large = {{2, 3, 5, 7, 11}};
    union LargeUnion big = wrap(large);
    if (sum((union LargeUnion)large) != 13 || big.large.words[3] != 7) return 7;
    union Nested nested = (union Nested)n;
    n.d = 9.0;
    if (nested.n.d != 2.5) return 8;
    if (static_int.i != 17 || static_double.d != 2.5 || *static_pointer.p != 31
        || static_payload.payload.words[2] != 7 || static_nested.n.i != 19
        || static_bits.i != -1) return 9;
    union Bits bits = (union Bits)7;
    if (bits.i != -1) return 10;
    p = (union Pointer)callback;
    p.fn();
    if (calls != 4) return 11;
    (void)(union Number)next();
    if (calls != 5 || sizeof((union Number)next()) != sizeof(union Number)) return 12;
    if (calls != 5) return 13;
    const int ci = 41;
    union Number qualified = (union Number)ci;
    if (qualified.i != 41) return 14;
    const int *cp = &ci;
    p = (union Pointer)cp;
    if (*p.cp != 41) return 15;
    return 0;
}
"#;

fn compile(
    source: &str,
    target: Target,
    debug_info: bool,
) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "union-cast.c",
        keel_c::Options { target, debug_info },
    )
}

#[test]
fn union_cast_storage_and_runtime() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gnu-union-casts");
    fs::create_dir_all(&dir).unwrap();
    let runner = dir.join("main.c");
    fs::write(
        &runner,
        "int union_cast_test(void); int main(void) { return union_cast_test(); }",
    )
    .unwrap();
    for target in Target::NATIVE {
        for debug_info in [false, true] {
            let module = compile(SOURCE, target, debug_info).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let module = keel_ir::parse(&module.to_string()).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                keel_ir::verify_module(&module).unwrap();
                let assembly = keel_codegen::compile(&module, target).unwrap();
                if Some(target) == Target::host() {
                    let generated = dir.join(format!("generated-{debug_info}-{optimize}.s"));
                    fs::write(&generated, assembly).unwrap();
                    run(&generated, &runner);
                }
            }
        }
    }
    // Independently execute the same assertions with the system C compiler.
    let source = dir.join("witness.c");
    fs::write(&source, SOURCE).unwrap();
    run(&source, &runner);
}

fn run(source: &std::path::Path, runner: &std::path::Path) {
    let exe = source.with_extension("exe");
    let output = Command::new("clang")
        .args(["-std=gnu11", "-O2"])
        .arg(source)
        .arg(runner)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        source.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let status = Command::new(&exe).status().unwrap();
    assert_eq!(status.code(), Some(0), "{}", exe.display());
}

#[test]
fn union_cast_matching_and_rvalue_diagnostics() {
    // No arithmetic conversion, null-pointer conversion, pointer qualification
    // conversion or structural equivalence can make a member match.
    for source in [
        "union U { long x; }; void f(int x) { (union U)x; }",
        "union U { int x; }; void f(unsigned x) { (union U)x; }",
        "union U { double x; }; void f(float x) { (union U)x; }",
        "union U { int *x; }; void f(void) { (union U)0; }",
        "union U { void *x; }; void f(int *x) { (union U)x; }",
        "union U { const int *x; }; void f(int *x) { (union U)x; }",
        "union U { int *x; }; void f(const int *x) { (union U)x; }",
        "struct A { int x; }; struct B { int x; }; union U { struct A a; }; void f(struct B x) { (union U)x; }",
        "enum E { A }; enum F { B }; union U { enum E x; }; void f(enum F x) { (union U)x; }",
        "union U { int x; }; void f(void) { (union U)1 = (union U)2; }",
        "union U { int x; }; void f(void) { &((union U)1); }",
        "union U { int x; }; void f(void) { ((union U)1).x = 2; }",
        "struct S { int x; }; void f(void) { (struct S)1; }",
    ] {
        for target in Target::NATIVE {
            assert!(
                compile(source, target, false).is_err(),
                "accepted: {source} on {target:?}"
            );
        }
    }
    for source in [
        "union U { const int x; }; int f(int x) { union U u = (union U)x; return u.x; }",
        "enum E { A = -1 }; union U { enum E x; }; int f(int x) { union U u = (union U)x; return u.x; }",
        "union U { short x; }; short f(short x) { union U u = (union U)x; return u.x; }",
        "enum E { A }; union U { enum E x; }; enum E f(enum E x) { union U u = (union U)x; return u.x; }",
        "union U { int *p; }; int f(int a[2]) { union U u = (union U)a; return *u.p; }",
    ] {
        let module = compile(source, Target::X86_64LinuxGnu, false).unwrap();
        keel_ir::verify_module(&module).unwrap();
    }
}
