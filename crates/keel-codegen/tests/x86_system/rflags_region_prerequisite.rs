//! Successful non-emitting CFG analysis never grants POPFQ codegen permission.
use keel_ir::{
    Type,
    rflags_region::{Block, Exit, Operation, Plan, analyze},
};

#[test]
fn structural_plan_cannot_unblock_reserved_swap() {
    let plan = Plan {
        captures: vec![Type::I64],
        observations: 1,
        blocks: vec![Block {
            operations: vec![
                Operation::SaveEntry,
                Operation::Swap {
                    capture: 0,
                    observation: 0,
                },
                Operation::ReapplyEntryImage,
            ],
            exit: Exit::Normal,
        }],
    };
    assert_eq!(analyze(&plan).unwrap().unresolved.len(), 5);
    assert!(keel_ir::Intrinsic::ALL.len() >= 731);
    assert_eq!(keel_ir::Intrinsic::X86RflagsSwap as usize, 730);
    let mut m=keel_ir::parse("func @swap(i64)->i64 {\nblock0(v0:i64):\n v1 = intrinsic x86_rflags_swap(v0)\n return v1\n}").unwrap();
    keel_ir::verify_module(&m).unwrap();
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        keel_opt::optimize_with_tier(&mut m, tier).unwrap();
        let error = keel_codegen::x86_64::compile(&m).unwrap_err().to_string();
        assert!(
            error.contains("requires a verified owned-FLAGS region"),
            "{error}"
        );
        assert!(keel_codegen::aarch64::compile(&m).is_err());
    }
}
