//! Bounded x87 region controls. Stable operation numbers, independent of Intrinsic discriminants.
//! See docs/targets/x86_64/f80/README.md. Existing raw APIs keep their masked-CW preconditions.
use keel_asm_ir::x86::{Opcode, op};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X87RegionOperand {
    None,
    St01,
    St10,
    St10Pop,
    St1,
    Flags,
    Input,
    Output,
    Second,
}
#[derive(Clone, Copy, Debug)]
pub struct X87RegionOp {
    pub name: &'static str,
    pub opcode: Opcode,
    pub loads: u8,
    pub results: u8,
    pub operand: X87RegionOperand,
    pub bits: u16,
}
pub const X87_REGION_OPS: &[X87RegionOp] = &[
    X87RegionOp {
        name: "fsin",
        opcode: op::FSIN,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 0
    X87RegionOp {
        name: "fcos",
        opcode: op::FCOS,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 1
    X87RegionOp {
        name: "f2xm1",
        opcode: op::F2XM1,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 2
    X87RegionOp {
        name: "fpatan",
        opcode: op::FPATAN,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 3
    X87RegionOp {
        name: "fyl2x",
        opcode: op::FYL2X,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 4
    X87RegionOp {
        name: "fyl2xp1",
        opcode: op::FYL2XP1,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 5
    X87RegionOp {
        name: "fscale",
        opcode: op::FSCALE,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 6
    X87RegionOp {
        name: "fprem",
        opcode: op::FPREM,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 7
    X87RegionOp {
        name: "fprem1",
        opcode: op::FPREM1,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 8
    X87RegionOp {
        name: "fsincos",
        opcode: op::FSINCOS,
        loads: 1,
        results: 2,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 9
    X87RegionOp {
        name: "fptan",
        opcode: op::FPTAN,
        loads: 1,
        results: 2,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 10
    X87RegionOp {
        name: "fxtract",
        opcode: op::FXTRACT,
        loads: 1,
        results: 2,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 11
    X87RegionOp {
        name: "fabs",
        opcode: op::FABS,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 12
    X87RegionOp {
        name: "fchs",
        opcode: op::FCHS,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 13
    X87RegionOp {
        name: "fsqrt",
        opcode: op::FSQRT,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 14
    X87RegionOp {
        name: "frndint",
        opcode: op::FRNDINT,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 15
    X87RegionOp {
        name: "ftst",
        opcode: op::FTST,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 16
    X87RegionOp {
        name: "fxam",
        opcode: op::FXAM,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 17
    X87RegionOp {
        name: "fadd",
        opcode: Opcode::Fadd,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St01,
        bits: 0,
    }, // 18
    X87RegionOp {
        name: "fmul",
        opcode: Opcode::Fmul,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St01,
        bits: 0,
    }, // 19
    X87RegionOp {
        name: "fsub",
        opcode: op::FSUB,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St01,
        bits: 0,
    }, // 20
    X87RegionOp {
        name: "fsubr",
        opcode: op::FSUBR,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St01,
        bits: 0,
    }, // 21
    X87RegionOp {
        name: "fdiv",
        opcode: op::FDIV,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St01,
        bits: 0,
    }, // 22
    X87RegionOp {
        name: "fdivr",
        opcode: op::FDIVR,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St01,
        bits: 0,
    }, // 23
    X87RegionOp {
        name: "fadd_st1",
        opcode: Opcode::Fadd,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10,
        bits: 0,
    }, // 24
    X87RegionOp {
        name: "fmul_st1",
        opcode: Opcode::Fmul,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10,
        bits: 0,
    }, // 25
    X87RegionOp {
        name: "fsub_st1",
        opcode: op::FSUB,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10,
        bits: 0,
    }, // 26
    X87RegionOp {
        name: "fsubr_st1",
        opcode: op::FSUBR,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10,
        bits: 0,
    }, // 27
    X87RegionOp {
        name: "fdiv_st1",
        opcode: op::FDIV,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10,
        bits: 0,
    }, // 28
    X87RegionOp {
        name: "fdivr_st1",
        opcode: op::FDIVR,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10,
        bits: 0,
    }, // 29
    X87RegionOp {
        name: "faddp",
        opcode: op::FADDP,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10Pop,
        bits: 0,
    }, // 30
    X87RegionOp {
        name: "fmulp",
        opcode: Opcode::Fmulp,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10Pop,
        bits: 0,
    }, // 31
    X87RegionOp {
        name: "fsubp",
        opcode: Opcode::Fsubp,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10Pop,
        bits: 0,
    }, // 32
    X87RegionOp {
        name: "fsubrp",
        opcode: Opcode::Fsubrp,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10Pop,
        bits: 0,
    }, // 33
    X87RegionOp {
        name: "fdivp",
        opcode: op::FDIVP,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10Pop,
        bits: 0,
    }, // 34
    X87RegionOp {
        name: "fdivrp",
        opcode: op::FDIVRP,
        loads: 2,
        results: 1,
        operand: X87RegionOperand::St10Pop,
        bits: 0,
    }, // 35
    X87RegionOp {
        name: "fcom",
        opcode: op::FCOM,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::St1,
        bits: 0,
    }, // 36
    X87RegionOp {
        name: "fcomp",
        opcode: op::FCOMP,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::St1,
        bits: 0,
    }, // 37
    X87RegionOp {
        name: "fcompp",
        opcode: op::FCOMPP,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 38
    X87RegionOp {
        name: "fucom",
        opcode: op::FUCOM,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::St1,
        bits: 0,
    }, // 39
    X87RegionOp {
        name: "fucomp",
        opcode: op::FUCOMP,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::St1,
        bits: 0,
    }, // 40
    X87RegionOp {
        name: "fucompp",
        opcode: op::FUCOMPP,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 41
    X87RegionOp {
        name: "fcomi",
        opcode: op::FCOMI,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::Flags,
        bits: 0,
    }, // 42
    X87RegionOp {
        name: "fcomip",
        opcode: op::FCOMIP,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::Flags,
        bits: 0,
    }, // 43
    X87RegionOp {
        name: "fucomi",
        opcode: Opcode::Fucomi,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::Flags,
        bits: 0,
    }, // 44
    X87RegionOp {
        name: "fucomip",
        opcode: Opcode::Fucomip,
        loads: 2,
        results: 0,
        operand: X87RegionOperand::Flags,
        bits: 0,
    }, // 45
    X87RegionOp {
        name: "fist16",
        opcode: op::FIST,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 16,
    }, // 46
    X87RegionOp {
        name: "fist32",
        opcode: op::FIST,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 32,
    }, // 47
    X87RegionOp {
        name: "fistp16",
        opcode: Opcode::Fistp,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 16,
    }, // 48
    X87RegionOp {
        name: "fistp32",
        opcode: Opcode::Fistp,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 32,
    }, // 49
    X87RegionOp {
        name: "fistp64",
        opcode: Opcode::Fistp,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 64,
    }, // 50
    X87RegionOp {
        name: "fisttp16",
        opcode: op::FISTTP,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 16,
    }, // 51
    X87RegionOp {
        name: "fisttp32",
        opcode: op::FISTTP,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 32,
    }, // 52
    X87RegionOp {
        name: "fisttp64",
        opcode: op::FISTTP,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 64,
    }, // 53
    X87RegionOp {
        name: "fild16",
        opcode: Opcode::Fild,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::Input,
        bits: 16,
    }, // 54
    X87RegionOp {
        name: "fild32",
        opcode: Opcode::Fild,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::Input,
        bits: 32,
    }, // 55
    X87RegionOp {
        name: "fild64",
        opcode: Opcode::Fild,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::Input,
        bits: 64,
    }, // 56
    X87RegionOp {
        name: "fiadd16",
        opcode: op::FIADD,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 57
    X87RegionOp {
        name: "fiadd32",
        opcode: op::FIADD,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 58
    X87RegionOp {
        name: "fimul16",
        opcode: op::FIMUL,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 59
    X87RegionOp {
        name: "fimul32",
        opcode: op::FIMUL,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 60
    X87RegionOp {
        name: "fisub16",
        opcode: op::FISUB,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 61
    X87RegionOp {
        name: "fisub32",
        opcode: op::FISUB,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 62
    X87RegionOp {
        name: "fisubr16",
        opcode: op::FISUBR,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 63
    X87RegionOp {
        name: "fisubr32",
        opcode: op::FISUBR,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 64
    X87RegionOp {
        name: "fidiv16",
        opcode: op::FIDIV,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 65
    X87RegionOp {
        name: "fidiv32",
        opcode: op::FIDIV,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 66
    X87RegionOp {
        name: "fidivr16",
        opcode: op::FIDIVR,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 67
    X87RegionOp {
        name: "fidivr32",
        opcode: op::FIDIVR,
        loads: 1,
        results: 1,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 68
    X87RegionOp {
        name: "fbld",
        opcode: op::FBLD,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::Input,
        bits: 80,
    }, // 69
    X87RegionOp {
        name: "fbstp",
        opcode: op::FBSTP,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Output,
        bits: 80,
    }, // 70
    X87RegionOp {
        name: "fldpi",
        opcode: op::FLDPI,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 71
    X87RegionOp {
        name: "fldl2e",
        opcode: op::FLDL2E,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 72
    X87RegionOp {
        name: "fldl2t",
        opcode: op::FLDL2T,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 73
    X87RegionOp {
        name: "fldlg2",
        opcode: op::FLDLG2,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 74
    X87RegionOp {
        name: "fldln2",
        opcode: op::FLDLN2,
        loads: 0,
        results: 1,
        operand: X87RegionOperand::None,
        bits: 0,
    }, // 75
    X87RegionOp {
        name: "ficom16",
        opcode: op::FICOM,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 76
    X87RegionOp {
        name: "ficom32",
        opcode: op::FICOM,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 77
    X87RegionOp {
        name: "ficomp16",
        opcode: op::FICOMP,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Second,
        bits: 16,
    }, // 78
    X87RegionOp {
        name: "ficomp32",
        opcode: op::FICOMP,
        loads: 1,
        results: 0,
        operand: X87RegionOperand::Second,
        bits: 32,
    }, // 79
];
/// Low byte: stable operation. Bits 8..9: input F80/F32/F64. Bits 10..11:
/// output F80/F32/F64. Bit 12: discard post-principal FP results. All other bits zero.
pub fn x87_region_control(control: i64) -> Option<(&'static X87RegionOp, u16, u16, bool)> {
    if control < 0 || control & !0x1fff != 0 {
        return None;
    }
    let op = X87_REGION_OPS.get((control & 255) as usize)?;
    let input = [80, 32, 64].get(((control >> 8) & 3) as usize).copied()?;
    let output = [80, 32, 64].get(((control >> 10) & 3) as usize).copied()?;
    let discard = control & 4096 != 0;
    if op.loads == 0 && input != 80
        || op.results == 0 && (output != 80 || discard)
        || discard && output != 80
    {
        return None;
    }
    Some((op, input, output, discard))
}
