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

/// Call lowering moves only one result out of its register, so a call
/// whose C signature has several must not compile.
#[test]
fn calls_with_multiple_c_results_are_rejected() {
    for call in [
        "v2, v3 = call @pair(v0)",
        "v2, v3 = call_indirect v1(v0) : (i64) -> (i64, i64)",
    ] {
        let module = keel_ir::parse(&format!(
            "decl @pair(i64) -> (i64, i64)\nfunc @f(i64, ptr) -> i64 {{\nblock0(v0: i64, v1: ptr):\n {call}\n v4 = iadd v2, v3\n return v4\n}}\n"
        ))
        .unwrap();
        for target in keel_codegen::Target::NATIVE {
            let error = keel_codegen::compile(&module, target)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("multiple return values are not supported"),
                "{target:?} {call}: {error}"
            );
        }
    }
}
