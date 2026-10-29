//! The generic contract is independent of the fixed-type overflow IR operation.
use keel_ir::{InstData, Target};

const TYPES: &[&str] = &[
    "signed char",
    "unsigned char",
    "short",
    "unsigned short",
    "int",
    "unsigned",
    "long",
    "unsigned long",
    "long long",
    "unsigned long long",
    "__int128",
    "unsigned __int128",
];

#[test]
fn generic_overflow_types_codegen_and_volatile_store() {
    let mut source = String::new();
    // Every input/result width and signedness, including i128 products. Rotate
    // the destination to exercise both widening and narrowing without creating
    // thousands of near-identical functions in the frontend regression suite.
    for (i, a) in TYPES.iter().enumerate() {
        for (j, b) in TYPES.iter().enumerate() {
            let r = TYPES[(i + j) % TYPES.len()];
            for op in ["add", "sub", "mul"] {
                source.push_str(&format!("_Bool {op}_{i}_{j}({a} a, {b} b, volatile {r} *r) {{ return __builtin_{op}_overflow(a,b,r); }}\n"));
            }
        }
    }
    for target in Target::NATIVE {
        let mut module = keel_c::compile(
            &source,
            "generic.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        for optimized in [false, true] {
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let stores = module
                .functions
                .iter()
                .flat_map(|f| f.layout_insts().map(move |(_, i)| f.inst_data(i)))
                .filter(|i| matches!(i, InstData::Store { flags, .. } if flags.volatile))
                .count();
            assert_eq!(stores, TYPES.len() * TYPES.len() * 3);
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn generic_overflow_diagnostics() {
    for source in [
        "int f(int *p) { return __builtin_add_overflow(p,1,p); }",
        "int f(int *p) { return __builtin_mul_overflow(1,2.0,p); }",
        "int f(void) { return __builtin_sub_overflow(1,2,0); }",
        "int f(void *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(_Bool *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(double *p) { return __builtin_add_overflow(1,2,p); }",
        "enum E { A }; int f(enum E *p) { return __builtin_add_overflow(1,2,p); }",
        "enum E { A }; typedef enum E E; int f(volatile E *p) { return __builtin_add_overflow(1,2,p); }",
        "typedef const int C; int f(volatile C *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(const int *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(_Atomic(int) *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(int *p) { return __builtin_add_overflow(1,p); }",
    ] {
        let error = keel_c::compile(source, "invalid-generic.c", Default::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("invalid-generic.c:1:")
                && (error.contains("generic overflow") || error.contains("builtin takes 3")),
            "{source}\n{error}"
        );
    }
    for source in [
        "enum E { A }; int f(int *p) { return __builtin_add_overflow(A,(_Bool)1,p); }",
        "typedef volatile unsigned long V; int f(V *p) { return __builtin_mul_overflow(1,2,p); }",
    ] {
        keel_c::compile(source, "valid-generic.c", Default::default()).unwrap();
    }
}
