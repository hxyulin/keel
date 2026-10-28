//! Which kinds of instruction a function contains, found in one walk. Most
//! target covers and frame checks look for rare instructions; testing this
//! summary first skips their own whole-function scans.
use std::cell::OnceCell;

use keel_ir::{BinaryOp, ConvertOp, EntityRef, Function, InstData, OverflowOp};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InstKinds {
    pub inline_asm: bool,
    pub landing_pad: bool,
    /// `call`, `call_indirect` or `invoke`.
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
    /// The kinds, and in the same walk the machine-relevant IR use counts of
    /// [`crate::fold::use_counts`].
    pub(crate) fn with_uses(func: &Function) -> (Self, Vec<u8>) {
        let mut kinds = Self::default();
        let mut uses = vec![0u8; func.num_values()];
        for (_, inst) in func.layout_insts() {
            let data = func.inst_data(inst);
            if data.is_lifetime_marker() {
                continue;
            }
            data.for_each_operand(|v| {
                let count = &mut uses[func.resolve(v).index()];
                *count = (*count + 1).min(2);
            });
            match data {
                InstData::InlineAsm { .. } => kinds.inline_asm = true,
                InstData::LandingPad { .. } => kinds.landing_pad = true,
                InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Invoke { .. } => {
                    kinds.call = true
                }
                InstData::ReturnCall { .. } | InstData::ReturnCallIndirect { .. } => {
                    kinds.return_call = true
                }
                InstData::Intrinsic { .. } | InstData::X86RflagsRegion { .. } => {
                    kinds.intrinsic = true
                }
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
        (kinds, uses)
    }
}

/// Immutable facts shared by the target selectors for one function.
/// Machine uses omit lifetime markers and saturate at two; raw uses
/// include every operand and saturate at 255. Raw counts are only prepared
/// when a vector-cover candidate requests them.
pub(crate) struct FunctionFacts<'a> {
    pub func: &'a Function,
    pub kinds: InstKinds,
    pub machine_uses: Vec<u8>,
    raw_uses: OnceCell<Vec<u8>>,
}

impl<'a> FunctionFacts<'a> {
    pub fn new(func: &'a Function) -> Self {
        let (kinds, machine_uses) = InstKinds::with_uses(func);
        Self {
            func,
            kinds,
            machine_uses,
            raw_uses: OnceCell::new(),
        }
    }

    pub fn raw_uses(&self) -> &[u8] {
        self.raw_uses.get_or_init(|| {
            let mut uses = vec![0u8; self.func.num_values()];
            for (_, inst) in self.func.layout_insts() {
                self.func.inst_data(inst).for_each_operand(|v| {
                    let count = &mut uses[self.func.resolve(v).index()];
                    *count = count.saturating_add(1);
                });
            }
            uses
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operand_policies_preserve_aliases_lifetimes_multiplicity_and_saturation() {
        let arguments = vec!["v5"; 260].join(", ");
        let source = format!(
            "decl @sink(i64, ...)
func @f(ptr, ptr, i64) -> i64 {{
block0(v0: ptr, v1: ptr, v2: i64):
 v3 = ptradd v0, v2
 intrinsic lifetime_start(v3)
 v4 = load.i64 v3
 v5 = iadd v4, v4
 intrinsic lifetime_end(v3)
 call @sink({arguments})
 return v5
}}"
        );
        let mut module = keel_ir::parse(&source).unwrap();
        let f = &mut module.functions[0];
        let entry = f.entry_block().unwrap();
        let alias = f.block_insts(entry)[0];
        let old = f.first_result(alias);
        let target = f.block_params(entry)[1];
        f.replace_all_uses(old, target);
        let facts = FunctionFacts::new(f);
        assert!(facts.raw_uses.get().is_none());
        assert_eq!(facts.machine_uses[target.index()], 1);
        assert_eq!(facts.machine_uses[old.index()], 0);
        // The same operand appearing twice is two machine uses.
        let load = f.block_insts(entry)[2];
        let add = f.block_insts(entry)[3];
        assert_eq!(facts.machine_uses[f.first_result(load).index()], 2);
        assert_eq!(facts.machine_uses[f.first_result(add).index()], 2);
        let raw = facts.raw_uses();
        assert_eq!(raw[target.index()], 3);
        assert_eq!(raw[old.index()], 0);
        assert_eq!(raw[f.first_result(load).index()], 2);
        assert_eq!(raw[f.first_result(add).index()], 255);
        assert!(std::ptr::eq(raw.as_ptr(), facts.raw_uses().as_ptr()));
    }

    #[test]
    fn duplicate_branch_arguments_count_as_separate_occurrences() {
        let m = keel_ir::parse(
            "func @f(i64) -> i64 {
block0(v0: i64):
 jump block1(v0, v0)
block1(v1: i64, v2: i64):
 return v1
}",
        )
        .unwrap();
        keel_ir::verify_module(&m).unwrap();
        let facts = FunctionFacts::new(&m.functions[0]);
        assert_eq!(facts.machine_uses, [2, 1, 0]);
        assert_eq!(facts.raw_uses(), [2, 1, 0]);
    }

    #[test]
    fn noncandidate_vector_covers_leave_raw_counts_unprepared() {
        let m = keel_ir::parse(
            "func @f() {
block0:
 return
}",
        )
        .unwrap();
        let facts = FunctionFacts::new(&m.functions[0]);
        let mut selection = crate::select::Selection::default();
        assert!(crate::packed_absolute::select(&facts, &mut selection).is_empty());
        assert!(facts.raw_uses.get().is_none());
    }
}
