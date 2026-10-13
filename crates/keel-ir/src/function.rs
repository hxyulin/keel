//! A function: arenas of blocks, instructions and values, plus block order.
//!
//! All mutation goes through methods so invariants (result values, aliases)
//! stay in one place.

use crate::inst::{BlockCall, InstData};
use crate::{Arena, Block, FuncRef, Inst, StackSlot, Type, Value};
use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CallConv {
    /// The target's C calling convention.
    C,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Signature {
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
    pub cc: CallConv,
}

impl fmt::Display for Signature {
    /// `(i32, ptr) -> i64 cc(c)`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        write_list(f, &self.params)?;
        f.write_str(")")?;
        match self.returns.as_slice() {
            [] => {}
            [ty] => write!(f, " -> {ty}")?,
            tys => {
                f.write_str(" -> (")?;
                write_list(f, tys)?;
                f.write_str(")")?;
            }
        }
        match self.cc {
            CallConv::C => f.write_str(" cc(c)"),
        }
    }
}

/// A function known by name and signature: a module's `decl`, or an entry in
/// a function's table of callees.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FuncDecl {
    pub name: String,
    pub sig: Signature,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ValueDef {
    /// The `n`th result of an instruction.
    Result(Inst, u16),
    /// The `n`th parameter of a block.
    Param(Block, u16),
    /// Replaced by another value; see [`Function::replace_all_uses`].
    Alias(Value),
}

#[derive(Clone, Debug)]
pub struct ValueData {
    pub ty: Type,
    pub def: ValueDef,
}

/// A stack slot: `size` bytes aligned to `align` (a power of two, at most 16).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct StackSlotData {
    pub size: u32,
    pub align: u32,
}

#[derive(Clone, Debug, Default)]
struct BlockData {
    params: Vec<Value>,
    insts: Vec<Inst>,
}

#[derive(Clone, Debug)]
struct InstNode {
    data: InstData,
    results: Vec<Value>,
    block: Block,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub sig: Signature,
    blocks: Arena<Block, BlockData>,
    insts: Arena<Inst, InstNode>,
    values: Arena<Value, ValueData>,
    stack_slots: Arena<StackSlot, StackSlotData>,
    /// Functions this one calls. Keeping their signatures here makes a
    /// function self-contained: result types and verification of calls need
    /// no module.
    callees: Arena<FuncRef, FuncDecl>,
    /// Block order. The first block is the entry.
    layout: Vec<Block>,
}

impl Function {
    pub fn new(name: impl Into<String>, sig: Signature) -> Self {
        Self {
            name: name.into(),
            sig,
            blocks: Arena::new(),
            insts: Arena::new(),
            values: Arena::new(),
            stack_slots: Arena::new(),
            callees: Arena::new(),
            layout: Vec::new(),
        }
    }

    pub fn create_stack_slot(&mut self, size: u32, align: u32) -> StackSlot {
        self.stack_slots.push(StackSlotData { size, align })
    }

    pub fn stack_slots(&self) -> impl Iterator<Item = (StackSlot, StackSlotData)> + '_ {
        self.stack_slots.iter().map(|(ss, data)| (ss, *data))
    }

    pub fn num_stack_slots(&self) -> usize {
        self.stack_slots.len()
    }

    /// Returns a reference for calling `name`, reusing an existing one.
    pub fn import_function(&mut self, name: &str, sig: Signature) -> FuncRef {
        if let Some((func_ref, _)) = self.callees.iter().find(|(_, d)| d.name == name) {
            return func_ref;
        }
        self.callees.push(FuncDecl {
            name: name.to_string(),
            sig,
        })
    }

    pub fn callee(&self, func_ref: FuncRef) -> &FuncDecl {
        &self.callees[func_ref]
    }

    pub fn callees(&self) -> impl Iterator<Item = (FuncRef, &FuncDecl)> {
        self.callees.iter()
    }

    /// Creates a block at the end of the layout.
    pub fn create_block(&mut self) -> Block {
        let block = self.blocks.push(BlockData::default());
        self.layout.push(block);
        block
    }

    pub fn append_block_param(&mut self, block: Block, ty: Type) -> Value {
        let n = self.blocks[block].params.len() as u16;
        let value = self.values.push(ValueData {
            ty,
            def: ValueDef::Param(block, n),
        });
        self.blocks[block].params.push(value);
        value
    }

    /// Appends an instruction to `block` and creates its result values.
    pub fn append_inst(&mut self, block: Block, data: InstData) -> Inst {
        let result_tys = match &data {
            InstData::Iconst { ty, .. }
            | InstData::Convert { ty, .. }
            | InstData::Load { ty, .. } => vec![*ty],
            InstData::StackAddr { .. } => vec![Type::Ptr],
            InstData::Unary { arg, .. } => vec![self.value_type(*arg)],
            InstData::Binary { args, .. } => vec![self.value_type(args[0])],
            InstData::Select { args } => vec![self.value_type(args[1])],
            InstData::Icmp { .. } => vec![Type::I8],
            InstData::Call { func, .. } => self.callees[*func].sig.returns.clone(),
            InstData::Intrinsic { id, .. } => id.info().returns.to_vec(),
            InstData::Store { .. }
            | InstData::Jump { .. }
            | InstData::Brif { .. }
            | InstData::Return { .. }
            | InstData::Trap => vec![],
        };
        self.append_inst_with_results(block, data, &result_tys)
    }

    /// Like [`Function::append_inst`] with the result types supplied, for the
    /// parser, whose operands may not exist yet.
    pub(crate) fn append_inst_with_results(
        &mut self,
        block: Block,
        data: InstData,
        result_tys: &[Type],
    ) -> Inst {
        let inst = self.insts.push(InstNode {
            data,
            results: Vec::new(),
            block,
        });
        for (n, &ty) in result_tys.iter().enumerate() {
            let value = self.values.push(ValueData {
                ty,
                def: ValueDef::Result(inst, n as u16),
            });
            self.insts[inst].results.push(value);
        }
        self.blocks[block].insts.push(inst);
        inst
    }

    /// Makes every use of `old` read `new` instead. `old`'s defining
    /// instruction is left in place; dead code removal deletes it.
    pub fn replace_all_uses(&mut self, old: Value, new: Value) {
        assert_ne!(self.resolve(new), old, "alias cycle");
        assert_eq!(self.value_type(old), self.value_type(new), "type mismatch");
        self.values[old].def = ValueDef::Alias(new);
    }

    /// Follows aliases to the value that is actually defined.
    pub fn resolve(&self, mut value: Value) -> Value {
        while let ValueDef::Alias(next) = self.values[value].def {
            value = next;
        }
        value
    }

    /// Number of values ever created; value indices are below this.
    pub fn num_values(&self) -> usize {
        self.values.len()
    }

    /// Number of blocks ever created; block indices are below this.
    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    pub fn entry_block(&self) -> Option<Block> {
        self.layout.first().copied()
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block> + '_ {
        self.layout.iter().copied()
    }

    pub fn block_params(&self, block: Block) -> &[Value] {
        &self.blocks[block].params
    }

    pub fn block_insts(&self, block: Block) -> &[Inst] {
        &self.blocks[block].insts
    }

    pub fn inst_data(&self, inst: Inst) -> &InstData {
        &self.insts[inst].data
    }

    pub fn inst_results(&self, inst: Inst) -> &[Value] {
        &self.insts[inst].results
    }

    pub fn inst_block(&self, inst: Inst) -> Block {
        self.insts[inst].block
    }

    /// The single result of `inst`. Panics if it has none.
    pub fn first_result(&self, inst: Inst) -> Value {
        self.insts[inst].results[0]
    }

    pub fn value_type(&self, value: Value) -> Type {
        self.values[value].ty
    }

    pub fn value_def(&self, value: Value) -> ValueDef {
        self.values[value].def
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "func @{}{} {{", self.name, self.sig)?;
        for (ss, data) in self.stack_slots() {
            writeln!(
                f,
                "    {ss} = stack_slot {}, align {}",
                data.size, data.align
            )?;
        }

        for block in self.blocks() {
            write!(f, "{block}")?;
            let params = self.block_params(block);
            if !params.is_empty() {
                f.write_str("(")?;
                for (i, &param) in params.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{param}: {}", self.value_type(param))?;
                }
                f.write_str(")")?;
            }
            f.write_str(":\n")?;
            for &inst in self.block_insts(block) {
                f.write_str("    ")?;
                self.write_inst(f, inst)?;
                f.write_str("\n")?;
            }
        }
        f.write_str("}\n")
    }
}

impl Function {
    /// Formats one instruction the way the printer does, e.g. `v3 = iadd v1, v2`.
    pub fn display_inst(&self, inst: Inst) -> impl fmt::Display + '_ {
        struct DisplayInst<'a>(&'a Function, Inst);
        impl fmt::Display for DisplayInst<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.write_inst(f, self.1)
            }
        }
        DisplayInst(self, inst)
    }

    fn write_inst(&self, f: &mut fmt::Formatter<'_>, inst: Inst) -> fmt::Result {
        let results = self.inst_results(inst);
        if !results.is_empty() {
            write_list(f, results)?;
            f.write_str(" = ")?;
        }
        // Operands are printed resolved, so aliases never show up in text.
        let v = |value: Value| self.resolve(value);
        match self.inst_data(inst) {
            InstData::Iconst { ty, imm } => write!(f, "iconst.{ty} {imm}"),
            InstData::StackAddr { slot } => write!(f, "stack_addr {slot}"),
            InstData::Call { func, args } => {
                write!(f, "call @{}(", self.callees[*func].name)?;
                write_list(f, args.iter().map(|&a| v(a)))?;
                f.write_str(")")
            }
            InstData::Intrinsic { id, args } => {
                write!(f, "intrinsic {id}(")?;
                write_list(f, args.iter().map(|&a| v(a)))?;
                f.write_str(")")
            }
            InstData::Trap => f.write_str("trap"),
            InstData::Unary { op, arg } => write!(f, "{op} {}", v(*arg)),
            InstData::Binary { op, args } => write!(f, "{op} {}, {}", v(args[0]), v(args[1])),
            InstData::Select { args } => {
                write!(f, "select {}, {}, {}", v(args[0]), v(args[1]), v(args[2]))
            }
            InstData::Convert { op, ty, arg } => write!(f, "{op}.{ty} {}", v(*arg)),
            InstData::Icmp { cond, args } => {
                write!(f, "icmp.{cond} {}, {}", v(args[0]), v(args[1]))
            }
            InstData::Load { ty, addr } => write!(f, "load.{ty} {}", v(*addr)),
            InstData::Store { value, addr } => write!(f, "store {}, {}", v(*value), v(*addr)),
            InstData::Jump { dest } => {
                f.write_str("jump ")?;
                self.write_block_call(f, dest)
            }
            InstData::Brif { cond, dests } => {
                write!(f, "brif {}, ", v(*cond))?;
                self.write_block_call(f, &dests[0])?;
                f.write_str(", ")?;
                self.write_block_call(f, &dests[1])
            }
            InstData::Return { args } => {
                f.write_str("return")?;
                if !args.is_empty() {
                    f.write_str(" ")?;
                    write_list(f, args.iter().map(|&a| v(a)))?;
                }
                Ok(())
            }
        }
    }

    fn write_block_call(&self, f: &mut fmt::Formatter<'_>, call: &BlockCall) -> fmt::Result {
        write!(f, "{}", call.block)?;
        if !call.args.is_empty() {
            f.write_str("(")?;
            write_list(f, call.args.iter().map(|&a| self.resolve(a)))?;
            f.write_str(")")?;
        }
        Ok(())
    }
}

fn write_list<T: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    items: impl IntoIterator<Item = T>,
) -> fmt::Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityRef;
    use crate::inst::{BinaryOp, BlockCall, ConvertOp, IntCC};

    fn binary(func: &mut Function, block: Block, op: BinaryOp, a: Value, b: Value) -> Value {
        let inst = func.append_inst(block, InstData::Binary { op, args: [a, b] });
        func.first_result(inst)
    }

    fn iconst(func: &mut Function, block: Block, ty: Type, imm: i64) -> Value {
        let inst = func.append_inst(block, InstData::Iconst { ty, imm });
        func.first_result(inst)
    }

    /// `fn sum(p: *const i32, n: i64) -> i64`: sums `n` sign-extended i32s.
    fn build_sum() -> Function {
        let sig = Signature {
            params: vec![Type::Ptr, Type::I64],
            returns: vec![Type::I64],
            cc: CallConv::C,
        };
        let mut func = Function::new("sum", sig);
        let entry = func.create_block();
        let header = func.create_block();
        let body = func.create_block();
        let exit = func.create_block();

        let p = func.append_block_param(entry, Type::Ptr);
        let n = func.append_block_param(entry, Type::I64);
        let zero = iconst(&mut func, entry, Type::I64, 0);
        func.append_inst(
            entry,
            InstData::Jump {
                dest: BlockCall {
                    block: header,
                    args: vec![zero, zero],
                },
            },
        );

        let i = func.append_block_param(header, Type::I64);
        let acc = func.append_block_param(header, Type::I64);
        let cond = func.append_inst(
            header,
            InstData::Icmp {
                cond: IntCC::Ult,
                args: [i, n],
            },
        );
        let cond = func.first_result(cond);
        func.append_inst(
            header,
            InstData::Brif {
                cond,
                dests: [
                    BlockCall {
                        block: body,
                        args: vec![],
                    },
                    BlockCall {
                        block: exit,
                        args: vec![acc],
                    },
                ],
            },
        );

        let four = iconst(&mut func, body, Type::I64, 4);
        let offset = binary(&mut func, body, BinaryOp::Imul, i, four);
        let addr = binary(&mut func, body, BinaryOp::Ptradd, p, offset);
        let elem = func.append_inst(
            body,
            InstData::Load {
                ty: Type::I32,
                addr,
            },
        );
        let elem = func.first_result(elem);
        let wide = func.append_inst(
            body,
            InstData::Convert {
                op: ConvertOp::Sext,
                ty: Type::I64,
                arg: elem,
            },
        );
        let wide = func.first_result(wide);
        let acc2 = binary(&mut func, body, BinaryOp::Iadd, acc, wide);
        let one = iconst(&mut func, body, Type::I64, 1);
        let i2 = binary(&mut func, body, BinaryOp::Iadd, i, one);
        func.append_inst(
            body,
            InstData::Jump {
                dest: BlockCall {
                    block: header,
                    args: vec![i2, acc2],
                },
            },
        );

        let result = func.append_block_param(exit, Type::I64);
        func.append_inst(exit, InstData::Return { args: vec![result] });
        func
    }

    #[test]
    fn prints_sum_loop() {
        let expected = "\
func @sum(ptr, i64) -> i64 cc(c) {
block0(v0: ptr, v1: i64):
    v2 = iconst.i64 0
    jump block1(v2, v2)
block1(v3: i64, v4: i64):
    v5 = icmp.ult v3, v1
    brif v5, block2, block3(v4)
block2:
    v6 = iconst.i64 4
    v7 = imul v3, v6
    v8 = ptradd v0, v7
    v9 = load.i32 v8
    v10 = sext.i64 v9
    v11 = iadd v4, v10
    v12 = iconst.i64 1
    v13 = iadd v3, v12
    jump block1(v13, v11)
block3(v14: i64):
    return v14
}
";
        assert_eq!(build_sum().to_string(), expected);
    }

    #[test]
    fn replace_all_uses_prints_resolved_operands() {
        let mut func = build_sum();
        let (v4, v11) = (Value::new(4), Value::new(11));
        // Pretend a pass proved `v11` equals the incoming accumulator.
        func.replace_all_uses(v11, v4);
        assert_eq!(func.resolve(v11), v4);
        assert!(func.to_string().contains("jump block1(v13, v4)"));
    }

    #[test]
    fn pinned_and_floating() {
        let func = build_sum();
        let pinned: Vec<String> = func
            .blocks()
            .flat_map(|b| func.block_insts(b).to_vec())
            .filter(|&inst| func.inst_data(inst).is_pinned())
            .map(|inst| inst.to_string())
            .collect();
        // jump, brif, load, jump, return
        assert_eq!(pinned, ["inst1", "inst3", "inst7", "inst12", "inst13"]);
    }
}
