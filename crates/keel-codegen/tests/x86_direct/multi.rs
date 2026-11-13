//! Shared result classification and actual register-bank execution.
use super::{parse, request};
use keel_codegen::{Request, backend_profile};
#[test]
fn scalar_multiple_results_match_c_and_fast_register_banks() {
    super::integer::execute_count(
        "multi-results",
        include_str!("multi.keel"),
        include_str!("multi_oracle.c"),
        Some(9),
    );
}
#[test]
fn invalid_multi_result_shapes_keep_ordinary_diagnostics() {
    for returns in [
        "i32,i32",
        "i8 sext,i64",
        "i64,i64,i64",
        "i64,i64,i64,i64,i64,i64,i64,i64) cc(fast",
    ] {
        let (types, cc) = returns
            .strip_suffix(") cc(fast")
            .map_or((returns, ""), |t| (t, "cc(fast) local"));
        let types: Vec<_> = types.split(',').collect();
        let params = types
            .iter()
            .map(|t| t.split_whitespace().next().unwrap())
            .collect::<Vec<_>>()
            .join(",");
        let args = (0..types.len())
            .map(|i| format!("v{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let block = types
            .iter()
            .enumerate()
            .map(|(i, t)| format!("v{i}:{}", t.split_whitespace().next().unwrap()))
            .collect::<Vec<_>>()
            .join(",");
        let module = parse(&format!(
            "func @f({params})->({}) {cc} {{\nblock0({block}):\n return {args}\n}}\n",
            types.join(",")
        ));
        let req = request();
        let (result, report) = backend_profile::capture(|| req.program(&module));
        let ordinary = Request::new(req.target, req.pipeline).program(&module);
        assert_eq!(
            result.unwrap_err().to_string(),
            ordinary.unwrap_err().to_string()
        );
        assert_eq!(
            report.counters[backend_profile::Counter::DirectFunctions as usize],
            0
        );
    }
}

#[test]
fn high_result_registers_are_normalized_before_home_stores() {
    use object::{Object, ObjectSection};
    let module = parse(include_str!("multi.keel"));
    let req = request();
    let bytes = keel_asm::assemble_program_for(req.target, &req.program(&module).unwrap()).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let text = file.section_by_name(".text").unwrap().data().unwrap();
    // Independent assembler encodings for each high-register capture and narrow normalization.
    for instructions in ["mov rax,r8\n movzx eax,al", "mov rax,r9\n movzx eax,ax"] {
        let object =
            keel_asm::assemble_source_for(req.target, &format!(".text\n {instructions}\n"))
                .unwrap();
        let oracle = object::File::parse(object.as_slice()).unwrap();
        let want = oracle.section_by_name(".text").unwrap().data().unwrap();
        assert!(
            text.windows(want.len()).any(|window| window == want),
            "missing {instructions}"
        );
    }
}
