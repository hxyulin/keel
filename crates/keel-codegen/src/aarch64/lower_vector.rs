//! Lowering of NEON intrinsics and the legalized vector instructions
//! that no cover claimed.
use super::*;

impl FunctionCompiler<'_> {
    /// NEON intrinsics, and vector compares, selects, arithmetic, shuffles,
    /// lanes and reductions.
    pub(super) fn vector_inst(&mut self, inst: Inst, result: Option<Value>) {
        let func = self.func;
        match *func.inst_data(inst) {
            InstData::Neon { op, ty, ref args } => {
                if op.memory().is_some() {
                    self.neon_memory(op, ty, args, func.inst_results(inst));
                } else {
                    self.neon(op, ty, args, result);
                }
            }
            InstData::VectorIcmp { cond, args } => self.vector_compare(cond, args, result),
            InstData::VectorSelect { args } => self.vector_select(args, result),
            InstData::VectorBinary { op, args } => self.vector_binary(op, args, result),
            InstData::VectorReduce { op, arg } => self.vector_reduce(op, arg, result),
            InstData::VectorUnary { op, arg } => self.vector_unary(op, arg, result),
            InstData::VectorSplat { ty, arg } => self.vector_splat(ty, arg, result),
            InstData::VectorShuffle { args, ref lanes } => {
                self.vector_shuffle_window(args, lanes, result)
            }
            InstData::VectorExtract { arg, lane } => self.vector_extract(arg, lane, result),
            InstData::VectorInsert { args, lane } => self.vector_insert(args, lane, result),
            InstData::VectorFcmp { .. } => {
                unreachable!("legalized vector operation")
            }
            _ => unreachable!("not a vector instruction"),
        }
    }
}
