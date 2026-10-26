//! Report catalogue record layout and operand sharing on the current target.
use keel_asm_ir::x86::{
    evex::EvexForm,
    isa::{EVEX_FORMS, FORMS, IsaForm, IsaOperand},
};
fn main() {
    println!(
        "IsaOperand={} IsaForm={} EvexForm={} legacy_forms={} evex_forms={}",
        size_of::<IsaOperand>(),
        size_of::<IsaForm>(),
        size_of::<EvexForm>(),
        FORMS.len(),
        EVEX_FORMS.len()
    );
    let mut seen = std::collections::HashSet::new();
    let operand_bytes: usize = FORMS
        .iter()
        .chain(EVEX_FORMS.iter().map(|e| &e.form))
        .filter_map(|f| {
            seen.insert((f.operands.as_ptr(), f.operands.len()))
                .then_some(size_of_val(f.operands))
        })
        .sum();
    println!("distinct_operand_slice_bytes={operand_bytes}");
}
