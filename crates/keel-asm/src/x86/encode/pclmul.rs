//! Legacy 66 0F 3A 44 /r ib only; memory source is always m128.
use super::*;
pub(super) fn encode<'a>(e: &mut Encoding<'a>, i: &'a Instruction) -> Result<(), String> {
    let [a, b, control] = i.operands.as_slice() else {
        return Err("pclmulqdq requires xmm, xmm/m128, imm8".into());
    };
    if !xmm(a) {
        return Err("pclmulqdq destination must be xmm".into());
    }
    check_xmm(b, 128)?;
    let n = constant(immediate(control)?)
        .filter(|n| (0..=255).contains(n))
        .ok_or("pclmulqdq control requires constant imm8 in0..=255")?;
    e.rm(0x66, &[0x0f, 0x3a, 0x44], 32, reg(a)?.number, b, None)?;
    e.number(n, 1);
    Ok(())
}
