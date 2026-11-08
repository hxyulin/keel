//! Lowering shared by the native backends: allocated operand locations,
//! block-argument edge copies, switch case plans and call classification.
//! Each backend keeps its own instruction selection and emission.
use crate::regalloc::{Allocation, Loc, OperandKind, Transfer, ValuePart};
use keel_ir::{BlockCall, Function, Inst, InstData, Intrinsic, Type, Value};

/// Operand locations and edge copies, read from a backend's allocation.
pub(crate) trait EdgeCopies<'a> {
    fn func(&self) -> &'a Function;
    fn allocation(&self) -> &Allocation;
    /// The allocator's index of the instruction being lowered.
    fn allocation_index(&self) -> Option<usize>;
    /// The instruction being lowered; a terminator when edges are lowered.
    fn current_inst(&self) -> Option<Inst>;
    /// A reusable buffer for edge copies.
    fn copy_buffer(&mut self) -> &mut Vec<Transfer>;
    /// Emits `moves`, each `(dst, src)`, as if they all happened at once.
    fn emit_parallel_copy(&mut self, moves: &[Transfer]);

    fn loc(&self, value: Value) -> Loc {
        self.allocation()
            .operand_loc(
                self.allocation_index(),
                ValuePart::low(self.func().resolve(value)),
                OperandKind::Use,
            )
            .expect("operand in reachable code is allocated")
    }
    fn hi(&self, value: Value) -> Loc {
        self.allocation()
            .operand_loc(
                self.allocation_index(),
                ValuePart::high(self.func().resolve(value)),
                OperandKind::Use,
            )
            .expect("operand in reachable code is allocated")
    }
    fn defined(&self, value: Value) -> Option<Loc> {
        self.allocation().operand_loc(
            self.allocation_index(),
            ValuePart::low(value),
            OperandKind::Def,
        )
    }
    fn defined_hi(&self, value: Value) -> Option<Loc> {
        self.allocation().operand_loc(
            self.allocation_index(),
            ValuePart::high(value),
            OperandKind::Def,
        )
    }
    fn edge_moves(&self, successor: usize) -> &[Transfer] {
        self.allocation().edits.edge(
            self.current_inst().expect("edge in a terminator"),
            successor,
        )
    }

    /// Whether an edge can branch directly without executing any copies.
    /// Include allocator transfers and both halves of wide block arguments;
    /// parameters without locations are aliases and need no copy.
    fn block_args_noop(&self, call: &BlockCall, successor: usize) -> bool {
        let alloc = self.allocation();
        self.edge_moves(successor)
            .iter()
            .all(|(dst, src)| dst == src)
            && self
                .func()
                .block_params(call.block)
                .iter()
                .zip(&call.args)
                .all(|(&param, &arg)| {
                    let low = alloc.param_loc(call.block, ValuePart::low(param));
                    let high = alloc.param_loc(call.block, ValuePart::high(param));
                    low.is_none_or(|dst| dst == self.loc(arg))
                        && high.is_none_or(|dst| dst == self.hi(arg))
                })
    }

    /// Copies block-call arguments into the target's parameters, all at
    /// once (`jump block1(v4, v3)` may swap two parameters).
    fn block_args(&mut self, call: &BlockCall, successor: usize) {
        let params = self.func().block_params(call.block);
        let mut moves = std::mem::take(self.copy_buffer());
        moves.extend_from_slice(self.edge_moves(successor));
        for (&param, &arg) in params.iter().zip(&call.args) {
            // A parameter with no location is an alias, never read.
            let alloc = self.allocation();
            let Some(dst) = alloc.param_loc(call.block, ValuePart::low(param)) else {
                continue;
            };
            let hi = alloc.param_loc(call.block, ValuePart::high(param));
            moves.push((dst, self.loc(arg)));
            if let Some(hi) = hi {
                moves.push((hi, self.hi(arg)));
            }
        }
        self.emit_parallel_copy(&moves);
        moves.clear();
        *self.copy_buffer() = moves;
    }
}

/// The case values of a `switch`, as unsigned bits of the switch type.
pub(crate) struct SwitchPlan {
    pub values: Vec<u64>,
    pub min: u64,
    pub span: u64,
}

impl SwitchPlan {
    /// `mask` keeps the bits of the switch type.
    pub(crate) fn new(cases: &[i64], mask: u64) -> Self {
        let values: Vec<u64> = cases.iter().map(|&c| c as u64 & mask).collect();
        let (min, max) = match (values.iter().min(), values.iter().max()) {
            (Some(&min), Some(&max)) => (min, max),
            _ => (0, 0),
        };
        Self {
            values,
            min,
            span: max - min,
        }
    }

    /// Whether the cases are dense enough for a jump table indexed by
    /// `value - min`, where any index above `span` takes the default.
    pub(crate) fn jump_table(&self) -> bool {
        self.values.len() >= 4 && self.span < 3 * self.values.len() as u64 && self.span < 4096
    }

    /// Drop a bounds check only when a proven range fits the table. At most
    /// two extra default entries cost eight bytes, matching the two removed
    /// ARM instructions. Unknown or wide ranges retain the original plan.
    pub(crate) fn bound(&mut self, (lower, upper): (u64, u64)) -> bool {
        if !self.jump_table() {
            return false;
        }
        let min = self.min.min(lower);
        let max = (self.min + self.span).max(upper);
        let span = max - min;
        if span >= 4096 || span - self.span > 2 {
            return false;
        }
        self.min = min;
        self.span = span;
        true
    }

    /// The destination of each table entry from `min` to `min + span`, as an
    /// index into the switch destinations, where 0 is the default.
    pub(crate) fn table(&self) -> impl Iterator<Item = usize> + '_ {
        (0..=self.span).map(|k| {
            self.values
                .iter()
                .position(|&v| v == self.min + k)
                .map_or(0, |i| i + 1)
        })
    }
}

/// Whether a location copy from `src` to `dst` is one register-to-register
/// instruction. The backends count these for the codegen report.
pub(crate) fn is_register_move(dst: Loc, src: Loc) -> bool {
    dst != src
        && matches!(
            (dst, src),
            (Loc::Reg(_) | Loc::FReg(_), Loc::Reg(_) | Loc::FReg(_))
        )
}

/// Instructions that clobber the caller-saved registers on every target:
/// calls, the memory intrinsics, which call libc, and trapping `i128`
/// arithmetic, which calls compiler-rt.
pub(crate) fn is_call(func: &Function, data: &InstData) -> bool {
    match data {
        InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Invoke { .. } => true,
        InstData::Binary { op, args } => {
            op.can_trap() && func.value_type(func.resolve(args[0])) == Type::I128
        }
        InstData::Intrinsic { id, .. } => matches!(
            id,
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset | Intrinsic::Memcmp
        ),
        _ => false,
    }
}

/// Calls that may return a second time, from a `longjmp`. An `invoke` is a
/// call whose unwind edge does not change how its normal return behaves.
pub(crate) fn returns_twice(func: &Function, data: &InstData) -> bool {
    match data {
        InstData::Call { func: callee, .. } | InstData::Invoke { func: callee, .. } => {
            func.callee(*callee).sig.returns_twice
        }
        InstData::CallIndirect { sig, .. } => func.signature(*sig).returns_twice,
        _ => false,
    }
}

/// The callee of a function whose whole body is one `return_call` passing
/// its parameters unchanged. An explicit tail terminator already permits
/// discarding this activation, and every physical argument (including stack
/// arguments and aggregate pieces) is already where the callee expects it,
/// so the function can be a single jump. Callers keep the normal path for
/// debug homes.
pub(crate) fn identity_tail_callee(func: &Function) -> Option<&str> {
    if func.sig.variadic || func.sig.returns_twice || func.personality.is_some() {
        return None;
    }
    let mut blocks = func.blocks();
    let entry = blocks.next()?;
    if blocks.next().is_some() {
        return None;
    }
    let [inst] = func.block_insts(entry) else {
        return None;
    };
    let InstData::ReturnCall { func: callee, args } = func.inst_data(*inst) else {
        return None;
    };
    let decl = func.callee(*callee);
    (same_placement(&decl.sig, &func.sig)
        && !args.has_explicit_abi()
        && args.len() == func.block_params(entry).len()
        && args
            .iter()
            .zip(func.block_params(entry))
            .all(|(&arg, &param)| func.resolve(arg) == func.resolve(param)))
    .then_some(decl.name.as_str())
}

/// Whether arguments placed for `caller` are already placed for `callee`.
/// Equal signatures are. So are fast and C signatures of scalars and
/// pointers, which both conventions place alike, provided the callee needs
/// no extension the caller's own callers did not apply.
fn same_placement(callee: &keel_ir::Signature, caller: &keel_ir::Signature) -> bool {
    use keel_ir::{ArgExt, Purpose};
    let scalar = |p: &keel_ir::AbiParam| {
        p.purpose == Purpose::Normal
            && matches!(
                p.ty,
                Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::F32 | Type::F64 | Type::Ptr
            )
    };
    callee == caller
        || (callee.returns == caller.returns
            && !callee.variadic
            && !callee.returns_twice
            && callee.x86_vector_abi.is_none()
            && caller.x86_vector_abi.is_none()
            && callee.params.len() == caller.params.len()
            && callee.returns.iter().all(scalar)
            && callee.params.iter().zip(&caller.params).all(|(c, p)| {
                scalar(c) && c.ty == p.ty && (c.ext == ArgExt::None || c.ext == p.ext)
            }))
}

/// Whether `data` is an atomic access of an `i128`.
pub(crate) fn wide_atomic(func: &keel_ir::Function, data: &keel_ir::InstData) -> bool {
    use keel_ir::InstData;
    let ty = match *data {
        InstData::AtomicLoad { ty, .. } => ty,
        InstData::AtomicStore { value, .. } => func.value_type(value),
        InstData::AtomicRmw { args, .. } => func.value_type(args[1]),
        InstData::AtomicCas { args, .. } => func.value_type(args[1]),
        _ => return false,
    };
    ty == keel_ir::Type::I128
}

/// One class of result register: general-purpose or floating-point.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum ResultClass {
    Int,
    Float,
}

/// The register class of each result of `sig`, which has more than one.
/// Version 1 returns only integers up to 64 bits, pointers, `f32` and
/// `f64`, with no extension: a struct's fields are not extended.
pub(crate) fn multi_result_classes(sig: &keel_ir::Signature) -> Result<Vec<ResultClass>, String> {
    sig.returns
        .iter()
        .map(|r| {
            if r.ext != keel_ir::ArgExt::None {
                return Err(format!(
                    "{sig}: a function with several results does not extend them"
                ));
            }
            match r.ty {
                Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr => Ok(ResultClass::Int),
                Type::F32 | Type::F64 => Ok(ResultClass::Float),
                ty => Err(format!(
                    "{sig}: a function with several results cannot return {ty} yet"
                )),
            }
        })
        .collect()
}

/// Which 8-byte unit of a C struct of the results, in order and natural
/// layout, holds each result. `None` when the struct is over 16 bytes,
/// which C returns through memory, or when two results share a unit, which
/// C would pack into one register.
pub(crate) fn result_units(returns: &[keel_ir::AbiParam]) -> Option<Vec<u64>> {
    let layout = keel_ir::Layout::Struct(
        returns
            .iter()
            .map(|r| keel_ir::Layout::Scalar(r.ty))
            .collect(),
    );
    if layout.size() > 16 {
        return None;
    }
    let units: Vec<u64> = layout
        .leaves()
        .iter()
        .map(|&(offset, _)| offset / 8)
        .collect();
    units.windows(2).all(|w| w[0] < w[1]).then_some(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(source: &str) -> Function {
        keel_ir::parse(source)
            .expect("test IR parses")
            .functions
            .remove(0)
    }

    fn call_data(func: &Function) -> Vec<&InstData> {
        func.layout_insts()
            .map(|(_, inst)| func.inst_data(inst))
            .filter(|data| is_call(func, data))
            .collect()
    }

    #[test]
    fn invoke_of_a_returns_twice_callee_returns_twice() {
        let func = function(
            "decl @setjmp(ptr) -> i32 cc(c) returns_twice\n\
             decl @plain(ptr) -> i32 cc(c)\n\
             decl @personality()\n\
             func @f(ptr) -> i32 cc(c) personality @personality {\n\
             block0(v0: ptr):\n\
             \x20   v1 = invoke @setjmp(v0), block1, block2\n\
             block1:\n\
             \x20   v2 = invoke @plain(v0), block3, block2\n\
             block3:\n\
             \x20   v3 = call @setjmp(v0)\n\
             \x20   v4 = call @plain(v0)\n\
             \x20   return v1\n\
             block2:\n\
             \x20   v5, v6 = landingpad cleanup\n\
             \x20   return v5\n\
             }\n",
        );
        let calls = call_data(&func);
        assert_eq!(calls.len(), 4);
        let twice: Vec<bool> = calls.iter().map(|d| returns_twice(&func, d)).collect();
        assert_eq!(twice, [true, false, true, false]);
        assert!(matches!(calls[0], InstData::Invoke { .. }));
    }

    #[test]
    fn switch_plan_uses_a_table_only_for_dense_cases() {
        let plan = SwitchPlan::new(&[3, 1, 2, 5], 0xff);
        assert!(plan.jump_table());
        assert_eq!((plan.min, plan.span), (1, 4));
        assert_eq!(plan.table().collect::<Vec<_>>(), [2, 3, 1, 0, 4]);
        assert!(!SwitchPlan::new(&[1, 2, 3], 0xff).jump_table());
        assert!(!SwitchPlan::new(&[0, 10, 20, 30], 0xff).jump_table());
        // Masked to the switch type: -1 is 0xff for an i8 switch.
        assert_eq!(SwitchPlan::new(&[-1], 0xff).values, [0xff]);
    }
}
