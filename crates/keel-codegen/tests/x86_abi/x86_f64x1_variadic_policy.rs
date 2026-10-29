//! Explicit producer variadics and placement-compatible scalar tails.
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};

fn both_routes(src: &str, accepted: bool) {
    let m = keel_ir::parse(src).unwrap();
    let text = keel_codegen::x86_64::compile(&m);
    let typed = keel_codegen::x86_64::compile_program_with_pipeline(
        &m,
        &keel_codegen::x86_64::REGISTERS,
        &CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Fast,
        },
    );
    assert_eq!(text.is_ok(), accepted, "{src}: {text:?}");
    assert_eq!(typed.is_ok(), accepted, "{src}: {typed:?}");
}

#[test]
fn tagged_local_reader_and_ordinary_variadic_adapters() {
    for producer in ["gnu", "clang"] {
        let tag = format!("x86_vector_abi({producer})");
        let src = format!(
            "decl @callee(i64,...)->f64x1 {tag}\n\
             func @reader(i64,f64x1,...)->f64x1 {tag} {{\n\
              ss0 = stack_slot 24, align 8\n\
             entry(v0:i64,v1:f64x1):\n\
              v2 = stack_addr ss0\n\
              intrinsic va_start(v2)\n\
              v3 = va_arg.f64x1 v2 {tag}\n\
              return v3\n}}\n\
             func @adapter(ptr,f64x1)->f64x1 {tag} {{\n\
             entry(v0:ptr,v1:f64x1):\n\
              v2 = iconst.i64 1\n\
              v3 = call @callee(v2,v1)\n\
              v4 = call_indirect v0(v2,v3) : (i64,...)->f64x1 {tag}\n\
              return v4\n}}"
        );
        keel_ir::verify_module(&keel_ir::parse(&src).unwrap()).unwrap();
        both_routes(&src, true);
        // A pure lane operation between reads may acquire private local spill
        // stores during legalization. Those stores cannot alias the va_list.
        let interleaved = format!(
            "func @lanes(i64x2,...)->f64x1 {tag} {{\n ss0 = stack_slot 24, align 8\nentry(v0:i64x2):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n v2 = va_arg.f64x1 v1 {tag}\n v3 = vextract v0,0\n v4 = va_arg.f64x1 v1 {tag}\n v5 = bitcast.i64 v4\n v6 = xor v3,v5\n v7 = bitcast.f64x1 v6\n return v7\n}}"
        );
        keel_ir::verify_module(&keel_ir::parse(&interleaved).unwrap()).unwrap();
        both_routes(&interleaved, true);
    }
}

#[test]
fn unnamed_f64x1_tails_check_capacity_and_unspecified_calls_remain_closed() {
    for producer in ["gnu", "clang"] {
        let tag = format!("x86_vector_abi({producer})");
        // Neither endpoint signature contains f64x1: only the actual unnamed
        // operand exposes it. Check each route independently.
        for tail in [
            "return_call @g(v9,v8)".to_string(),
            format!("return_call_indirect v0(v9,v8) : (i64,...)->i64 {tag}"),
        ] {
            both_routes(
                &format!(
                    "decl @g(i64,...)->i64 {tag}\nfunc @bad(ptr,i64,i64,i64,i64,i64,i64,i64)->i64 {tag} {{\nentry(v0:ptr,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64,v7:i64):\n v8 = load.f64x1 v0\n v9 = iconst.i64 1\n {tail}\n}}"
                ),
                true,
            );
            let growing = tail.replace("v9,v8", "v9,v8,v8,v8");
            both_routes(
                &format!(
                    "decl @g(i64,...)->i64 {tag}\nfunc @bad(ptr,i64,i64,i64,i64,i64,i64,i64)->i64 {tag} {{\nentry(v0:ptr,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64,v7:i64):\n v8 = load.f64x1 v0\n v9 = iconst.i64 1\n {growing}\n}}"
                ),
                false,
            );
        }
        both_routes(
            &format!(
                "decl @g(i64,...)\nfunc @bad(f64x1) {tag} {{\nentry(v0:f64x1):\n v1 = iconst.i64 1\n call @g(v1,v0)\n return\n}}"
            ),
            false,
        );
    }
}
