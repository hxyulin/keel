//! Which kinds of instruction a function contains, found in one walk. Most
//! target covers and frame checks look for rare instructions; testing this
//! summary first skips their own whole-function scans.
use keel_ir::{BinaryOp, ConvertOp, Function, InstData, OverflowOp};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InstKinds {
    pub inline_asm: bool,
    pub landing_pad: bool,
    /// `call` or `call_indirect`.
    pub call: bool,
    /// `return_call` or `return_call_indirect`.
    pub return_call: bool,
    pub intrinsic: bool,
    pub symbol_addr: bool,
    /// `umulhi`/`smulhi` or overflowing `umul`/`smul`.
    pub high_multiply: bool,
    pub select: bool,
    pub ptradd: bool,
    /// `icmp` or `fcmp`.
    pub compare: bool,
    pub brif: bool,
    pub vector_splat: bool,
    pub vector_icmp: bool,
    pub vector_select: bool,
    /// `swiden_low` or `uwiden_low`.
    pub widen_low: bool,
}

impl InstKinds {
    pub(crate) fn of(func: &Function) -> Self {
        let mut kinds = Self::default();
        for (_, inst) in func.layout_insts() {
            match func.inst_data(inst) {
                InstData::InlineAsm { .. } => kinds.inline_asm = true,
                InstData::LandingPad { .. } => kinds.landing_pad = true,
                InstData::Call { .. } | InstData::CallIndirect { .. } => kinds.call = true,
                InstData::ReturnCall { .. } | InstData::ReturnCallIndirect { .. } => {
                    kinds.return_call = true
                }
                InstData::Intrinsic { .. } => kinds.intrinsic = true,
                InstData::SymbolAddr { .. } => kinds.symbol_addr = true,
                InstData::Binary {
                    op: BinaryOp::Umulhi | BinaryOp::Smulhi,
                    ..
                }
                | InstData::Overflow {
                    op: OverflowOp::Umul | OverflowOp::Smul,
                    ..
                } => kinds.high_multiply = true,
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    ..
                } => kinds.ptradd = true,
                InstData::Select { .. } => kinds.select = true,
                InstData::Icmp { .. } | InstData::Fcmp { .. } => kinds.compare = true,
                InstData::Brif { .. } => kinds.brif = true,
                InstData::VectorSplat { .. } => kinds.vector_splat = true,
                InstData::VectorIcmp { .. } => kinds.vector_icmp = true,
                InstData::VectorSelect { .. } => kinds.vector_select = true,
                InstData::Convert {
                    op: ConvertOp::SwidenLow | ConvertOp::UwidenLow,
                    ..
                } => kinds.widen_low = true,
                _ => {}
            }
        }
        kinds
    }
}
