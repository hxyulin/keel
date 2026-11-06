//! x86-64 SysV ABI tests: aggregates, vector arguments, varargs, tail calls and CPU levels.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[allow(dead_code)]
#[path = "common/matrix.rs"]
mod matrix;
#[allow(dead_code)]
#[path = "common/userspace.rs"]
mod userspace;
#[path = "x86_abi/x86_d8.rs"]
mod x86_d8;
#[path = "x86_abi/x86_f64x1_layout_bounds.rs"]
mod x86_f64x1_layout_bounds;
#[path = "x86_abi/x86_f64x1_recursive.rs"]
mod x86_f64x1_recursive;
#[path = "x86_abi/x86_f64x1_variadic_policy.rs"]
mod x86_f64x1_variadic_policy;
#[path = "x86_abi/x86_sysv_tail.rs"]
mod x86_sysv_tail;
#[path = "x86_abi/x86_unnamed_aggregate.rs"]
mod x86_unnamed_aggregate;
#[path = "x86_abi/x86_userspace_v1.rs"]
mod x86_userspace_v1;
#[path = "x86_abi/x86_userspace_v2.rs"]
mod x86_userspace_v2;
#[path = "x86_abi/x86_userspace_v3.rs"]
mod x86_userspace_v3;
#[path = "x86_abi/x86_vector_abi_policy.rs"]
mod x86_vector_abi_policy;
#[path = "x86_abi/x86_vector_varargs.rs"]
mod x86_vector_varargs;
#[path = "x86_abi/x86_vector_wrappers.rs"]
mod x86_vector_wrappers;
#[path = "x86_abi/x86_wide_aggregate.rs"]
mod x86_wide_aggregate;
#[path = "x86_abi/x86_wide_transport_native.rs"]
mod x86_wide_transport_native;

/// Each target accepts the multi-result signatures its rules place, both
/// as a function's own signature and at a call. `cc(c)` follows C's struct
/// rules for the target; `cc(fast)` follows its argument registers.
#[test]
fn multiple_results_follow_each_targets_rules() {
    use keel_codegen::Target;
    const ONE_REGISTER: &str = "does not give each";
    // The returns, then the error on AArch64 and on x86-64, if any.
    let cases: &[(&str, Option<&str>, Option<&str>)] = &[
        ("i64, i64", None, None),
        ("i8, ptr", None, None),
        ("i32, i32", Some(ONE_REGISTER), Some(ONE_REGISTER)),
        ("i64, i64, i64", Some(ONE_REGISTER), Some(ONE_REGISTER)),
        ("i64, f64", Some(ONE_REGISTER), None),
        ("f32, f32", None, Some(ONE_REGISTER)),
        ("f64, f64, f64, f64", None, Some(ONE_REGISTER)),
        (
            "i128, i64",
            Some("cannot return i128"),
            Some("cannot return i128"),
        ),
        (
            "i8 sext, i64",
            Some("does not extend"),
            Some("does not extend"),
        ),
        ("i64, i64, i64) cc(fast", None, None),
        ("i32, i32, f32, f32) cc(fast", None, None),
        (
            "i64, i64, i64, i64, i64, i64, i64, i64) cc(fast",
            None,
            Some("at most 7 integer"),
        ),
        (
            "i64, i64, i64, i64, i64, i64, i64, i64, i64) cc(fast",
            Some("at most 8 integer"),
            Some("at most 7 integer"),
        ),
    ];
    for &(returns, arm, x86) in cases {
        let (types, cc) = match returns.strip_suffix(") cc(fast") {
            Some(types) => (types, "cc(fast)"),
            None => (returns, ""),
        };
        let types: Vec<&str> = types.split(", ").collect();
        let returns_list = types.join(", ");
        // External code needs narrow arguments' extension spelled out.
        let params: Vec<&str> = types
            .iter()
            .map(|&t| if t == "i8" { "i8 sext" } else { t })
            .collect();
        let params = params.join(", ");
        let values: Vec<String> = (0..types.len()).map(|i| format!("v{i}")).collect();
        let block_params: Vec<String> = types
            .iter()
            .zip(&values)
            .map(|(t, v)| format!("{v}: {}", t.split(' ').next().unwrap()))
            .collect();
        let (values, block_params) = (values.join(", "), block_params.join(", "));
        let results: Vec<String> = (0..types.len()).map(|i| format!("r{i}")).collect();
        let results = results.join(", ");
        let local = if cc.is_empty() { "" } else { " local" };
        let definition = format!(
            "func @f({params}) -> ({returns_list}) {cc}{local} {{\nblock0({block_params}):\n return {values}\n}}\n"
        );
        // A fast callee must be a local definition, so it is checked as one
        // too; a C callee is an external declaration.
        let callee = if cc.is_empty() {
            format!("decl @f({params}) -> ({returns_list})\n")
        } else {
            definition.clone()
        };
        let call = format!(
            "{callee}func @g({params}) {{\nblock0({block_params}):\n {results} = call @f({values})\n return\n}}\n"
        );
        let mut sources = vec![definition, call];
        if cc.is_empty() {
            sources.push(format!(
                "func @g(ptr, {params}) {{\nblock0(p: ptr, {block_params}):\n {results} = call_indirect p({values}) : ({params}) -> ({returns_list})\n return\n}}\n"
            ));
        }
        for source in sources {
            let module = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            for (target, want) in [
                (Target::Aarch64AppleDarwin, arm),
                (Target::X86_64LinuxGnu, x86),
            ] {
                match (keel_codegen::compile(&module, target), want) {
                    (Ok(_), None) => {}
                    (Err(error), Some(want)) if error.to_string().contains(want) => {}
                    (result, _) => panic!(
                        "{target:?} ({returns}): want {want:?}, got {:?}\n{source}",
                        result.map(|_| ()).map_err(|e| e.to_string())
                    ),
                }
            }
        }
    }
}
