//! The instruction loop: frames, calls and one arm per instruction format.

use crate::ops;
use crate::value::{bit_width, byte_size, from_lanes, lanes, mask, signed};
use crate::{Interpreter, Stop, Trap, Val};
use keel_ir::{
    BinaryOp, Block, BlockCall, ConvertOp, EntityRef, Function, Inst, InstData, Intrinsic,
    MemoryFlags, Purpose, RmwOp, Symbol, Type, UnaryOp, Value, VectorBinaryOp, VectorType,
};

/// One activation of a function.
struct Frame<'m> {
    func: &'m Function,
    values: Vec<Option<Val>>,
    block: Block,
    /// Index of the next instruction in `block`.
    pos: usize,
    /// Base address of each stack slot.
    slots: Vec<u64>,
    /// Allocations freed when the activation ends.
    owned: Vec<u64>,
    /// `stack_alloc` blocks and `stack_save` marks, oldest first.
    dynamic: Vec<u64>,
    /// The call instruction waiting for a callee's results.
    call: Option<Inst>,
}

impl Frame<'_> {
    fn get(&self, value: Value) -> Result<Val, Stop> {
        let value = self.func.resolve(value);
        self.values
            .get(value.index())
            .cloned()
            .flatten()
            .ok_or_else(|| Stop::Invalid(format!("{value} used before it is defined")))
    }

    fn get_all(&self, values: &[Value]) -> Result<Vec<Val>, Stop> {
        values.iter().map(|&v| self.get(v)).collect()
    }

    fn set(&mut self, value: Value, val: Val) -> Result<(), Stop> {
        let ty = self.func.value_type(value);
        if val.ty() != ty {
            return Err(Stop::Invalid(format!(
                "{value} has type {ty}, but its definition gives {}",
                val.ty()
            )));
        }
        self.values[value.index()] = Some(val);
        Ok(())
    }

    fn set_results(&mut self, inst: Inst, vals: Vec<Val>) -> Result<(), Stop> {
        let results = self.func.inst_results(inst);
        if results.len() != vals.len() {
            return Err(Stop::Invalid(format!(
                "{inst} names {} results for {} values",
                results.len(),
                vals.len()
            )));
        }
        for (&result, val) in results.iter().zip(vals) {
            self.set(result, val)?;
        }
        Ok(())
    }

    /// Takes the edge to `dest`, copying all block arguments at once.
    fn jump(&mut self, dest: &BlockCall) -> Result<(), Stop> {
        let args = self.get_all(&dest.args)?;
        let params = self.func.block_params(dest.block);
        if params.len() != args.len() {
            return Err(Stop::Invalid(format!(
                "{} takes {} arguments, given {}",
                dest.block,
                params.len(),
                args.len()
            )));
        }
        for (&param, arg) in params.iter().zip(args) {
            // An aliased parameter no longer carries data.
            if self.func.resolve(param) == param {
                self.set(param, arg)?;
            }
        }
        self.block = dest.block;
        self.pos = 0;
        Ok(())
    }
}

/// What the loop does after an instruction.
enum Flow<'m> {
    Next,
    Call(&'m Function, Vec<Val>),
    TailCall(&'m Function, Vec<Val>),
    Return(Vec<Val>),
}

fn invalid<T>(why: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::Invalid(why.into()))
}

fn unsupported<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::Unsupported(what.into()))
}

fn int(val: &Val) -> Result<(Type, u128), Stop> {
    val.int_parts()
        .ok_or_else(|| Stop::Invalid(format!("expected an integer, found {val:?}")))
}

fn float(val: &Val) -> Result<(Type, u64), Stop> {
    val.float_parts()
        .ok_or_else(|| Stop::Invalid(format!("expected f32 or f64, found {val:?}")))
}

fn ptr(val: &Val) -> Result<u64, Stop> {
    match val {
        Val::Ptr(addr) => Ok(*addr),
        _ => invalid(format!("expected a pointer, found {val:?}")),
    }
}

fn vector(val: &Val) -> Result<(VectorType, Vec<u64>), Stop> {
    match val {
        Val::Vector(ty, bytes) => Ok((*ty, lanes(*ty, bytes))),
        _ => invalid(format!("expected a vector, found {val:?}")),
    }
}

fn same_type(a: &Val, b: &Val) -> Result<(), Stop> {
    if a.ty() == b.ty() {
        Ok(())
    } else {
        invalid(format!("operands of types {} and {}", a.ty(), b.ty()))
    }
}

fn flag(b: bool) -> Val {
    Val::I8(u8::from(b))
}

/// Bits of an integer or pointer, for comparisons.
fn comparable(val: &Val) -> Result<(u32, u128), Stop> {
    match val {
        Val::Ptr(addr) => Ok((64, u128::from(*addr))),
        _ => int(val).map(|(ty, bits)| (bit_width(ty), bits)),
    }
}

fn check_align(addr: u64, align: u64) -> Result<(), Stop> {
    if !addr.is_multiple_of(align) {
        return Err(Trap::Misaligned { addr, align }.into());
    }
    Ok(())
}

impl<'m> Interpreter<'m> {
    pub(crate) fn run(&mut self, func: &'m Function, args: Vec<Val>) -> Result<Vec<Val>, Stop> {
        let mut stack = Vec::new();
        let result = self.run_stack(&mut stack, func, args);
        for frame in stack {
            self.leave(frame);
        }
        result
    }

    fn run_stack(
        &mut self,
        stack: &mut Vec<Frame<'m>>,
        func: &'m Function,
        args: Vec<Val>,
    ) -> Result<Vec<Val>, Stop> {
        stack.push(self.enter(func, args)?);
        let mut steps = 0u64;
        loop {
            steps += 1;
            if steps > self.step_limit {
                return Err(Stop::StepLimit);
            }
            let frame = stack.last_mut().unwrap();
            let Some(&inst) = frame.func.block_insts(frame.block).get(frame.pos) else {
                return invalid(format!("{} has no terminator", frame.block));
            };
            frame.pos += 1;
            match self.step(frame, inst)? {
                Flow::Next => {}
                Flow::Call(callee, args) => {
                    if stack.len() >= self.depth_limit {
                        return Err(Trap::StackOverflow.into());
                    }
                    stack.last_mut().unwrap().call = Some(inst);
                    stack.push(self.enter(callee, args)?);
                }
                Flow::TailCall(callee, args) => {
                    // The callee's byval copies are made before the frame goes.
                    let new = self.enter(callee, args)?;
                    let old = stack.pop().unwrap();
                    self.leave(old);
                    stack.push(new);
                }
                Flow::Return(values) => {
                    let old = stack.pop().unwrap();
                    self.leave(old);
                    let Some(caller) = stack.last_mut() else {
                        return Ok(values);
                    };
                    let call = caller.call.take().expect("a waiting call");
                    caller.set_results(call, values)?;
                    if let InstData::Invoke { dests, .. } = caller.func.inst_data(call) {
                        caller.jump(&dests[0])?;
                    }
                }
            }
        }
    }

    fn enter(&mut self, func: &'m Function, mut args: Vec<Val>) -> Result<Frame<'m>, Stop> {
        let Some(entry) = func.entry_block() else {
            return unsupported(format!("call to @{}, which has no body", func.name));
        };
        let mut frame = Frame {
            func,
            values: vec![None; func.num_values()],
            block: entry,
            pos: 0,
            slots: Vec::with_capacity(func.num_stack_slots()),
            owned: Vec::new(),
            dynamic: Vec::new(),
            call: None,
        };
        match self.set_up(&mut frame, &mut args) {
            Ok(()) => Ok(frame),
            Err(stop) => {
                self.leave(frame);
                Err(stop)
            }
        }
    }

    /// Which of `func`'s slots a lifetime marker names, found once per function.
    fn marked_slots(&mut self, func: &'m Function) -> std::rc::Rc<[bool]> {
        self.marked_slots
            .entry(&func.name)
            .or_insert_with(|| {
                let mut marked = vec![false; func.num_stack_slots()];
                for (_, inst) in func.layout_insts() {
                    if let Some(slot) = func.lifetime_marker_slot(inst) {
                        marked[slot.index()] = true;
                    }
                }
                marked.into()
            })
            .clone()
    }

    fn set_up(&mut self, frame: &mut Frame<'m>, args: &mut [Val]) -> Result<(), Stop> {
        let func = frame.func;
        let marked = self.marked_slots(func);
        for (id, slot) in func.stack_slots() {
            let base = self
                .memory
                .try_alloc(u64::from(slot.size), u64::from(slot.align), false)?;
            if marked[id.index()] {
                self.memory.set_live(base, false)?;
            }
            frame.slots.push(base);
            frame.owned.push(base);
        }
        let params = func.block_params(frame.block);
        // Extra variadic arguments are only readable through `va_start`,
        // which the interpreter does not model.
        let extra_ok = func.sig.variadic && args.len() > params.len();
        if params.len() != args.len() && !extra_ok {
            return invalid(format!(
                "@{} takes {} arguments, given {}",
                func.name,
                params.len(),
                args.len()
            ));
        }
        for (abi, arg) in func.sig.params.iter().zip(args.iter_mut()) {
            if let Purpose::ByVal(layout) = &abi.purpose {
                let src = ptr(arg)?;
                let size = layout.size();
                let copy = self.memory.try_alloc(size, layout.align(), false)?;
                frame.owned.push(copy);
                let bytes = self.memory.read(src, size)?.to_vec();
                self.memory.write(copy, &bytes)?;
                *arg = Val::Ptr(copy);
            }
        }
        for (&param, arg) in params.iter().zip(args.iter()) {
            if func.resolve(param) == param {
                frame.set(param, arg.clone())?;
            }
        }
        Ok(())
    }

    fn leave(&mut self, frame: Frame<'m>) {
        for base in frame.owned {
            self.memory.free(base);
        }
    }

    fn callee(&self, name: &str) -> Result<&'m Function, Stop> {
        match self.functions.get(name) {
            Some(&func) => Ok(func),
            None => unsupported(format!("call to external function @{name}")),
        }
    }

    fn callee_at(&self, addr: u64) -> Result<&'m Function, Stop> {
        match self.function_at.get(&addr) {
            Some(name) => self.callee(name),
            None => Err(Trap::BadCallTarget { addr }.into()),
        }
    }

    fn load(&self, ty: Type, addr: u64) -> Result<Val, Stop> {
        let size = byte_size(ty)?;
        let bytes = self.memory.read(addr, size)?;
        Ok(Val::from_bytes(ty, bytes))
    }

    fn atomic_addr(&self, addr: u64, ty: Type) -> Result<u64, Stop> {
        check_align(addr, byte_size(ty)?)?;
        Ok(addr)
    }

    fn step(&mut self, frame: &mut Frame<'m>, inst: Inst) -> Result<Flow<'m>, Stop> {
        let func = frame.func;
        let value = match func.inst_data(inst) {
            InstData::Iconst { ty, imm } => Val::from_int(*ty, *imm as u128 & mask(bit_width(*ty))),
            InstData::Fconst { ty, bits } => match ty {
                Type::F32 | Type::F64 => Val::from_float(*ty, *bits),
                _ => return invalid(format!("fconst.{ty}")),
            },
            InstData::F80const { .. } => return unsupported("f80 values"),
            InstData::Unary { op, arg } => unary(*op, &frame.get(*arg)?)?,
            InstData::Binary { op, args } => {
                binary(*op, &frame.get(args[0])?, &frame.get(args[1])?)?
            }
            InstData::Overflow { op, args } => {
                let (a, b) = (frame.get(args[0])?, frame.get(args[1])?);
                same_type(&a, &b)?;
                let ((ty, x), (_, y)) = (int(&a)?, int(&b)?);
                let (result, overflowed) = ops::overflow(*op, bit_width(ty), x, y);
                frame.set_results(inst, vec![Val::from_int(ty, result), flag(overflowed)])?;
                return Ok(Flow::Next);
            }
            InstData::Fma { args } => {
                let vals = frame.get_all(args)?;
                same_type(&vals[0], &vals[1])?;
                same_type(&vals[0], &vals[2])?;
                if let Val::Vector(ty, _) = &vals[0] {
                    let ty = *ty;
                    let lane = ty.lane_type();
                    let (_, a) = vector(&vals[0])?;
                    let (_, b) = vector(&vals[1])?;
                    let (_, c) = vector(&vals[2])?;
                    let out = (0..a.len())
                        .map(|i| ops::fma(lane, a[i], b[i], c[i]))
                        .collect::<Result<Vec<_>, _>>()?;
                    from_lanes(ty, &out)
                } else {
                    let (ty, a) = float(&vals[0])?;
                    let (b, c) = (float(&vals[1])?.1, float(&vals[2])?.1);
                    Val::from_float(ty, ops::fma(ty, a, b, c)?)
                }
            }
            InstData::Select { args } => {
                let (_, cond) = int(&frame.get(args[0])?)?;
                frame.get(args[if cond != 0 { 1 } else { 2 }])?
            }
            InstData::Convert { op, ty, arg } => convert(*op, *ty, &frame.get(*arg)?)?,
            InstData::Icmp { cond, args } => {
                let (a, b) = (frame.get(args[0])?, frame.get(args[1])?);
                same_type(&a, &b)?;
                let ((width, x), (_, y)) = (comparable(&a)?, comparable(&b)?);
                flag(ops::icmp(*cond, width, x, y))
            }
            InstData::Fcmp { cond, args } => {
                let (a, b) = (frame.get(args[0])?, frame.get(args[1])?);
                same_type(&a, &b)?;
                let ((ty, x), (_, y)) = (float(&a)?, float(&b)?);
                flag(ops::fcmp(*cond, ty, x, y)?)
            }
            InstData::VectorSplat { ty, arg } => {
                let Type::Vector(vt) = *ty else {
                    return invalid(format!("vsplat.{ty}"));
                };
                let scalar = frame.get(*arg)?;
                if scalar.ty() != vt.lane_type() {
                    return invalid(format!("vsplat.{vt} of {}", scalar.ty()));
                }
                let bits = scalar_lane_bits(&scalar)?;
                from_lanes(vt, &vec![bits; usize::from(vt.lanes())])
            }
            InstData::VectorUnary { op, arg } => {
                let (ty, a) = vector(&frame.get(*arg)?)?;
                let lane = ty.lane_type();
                let out = a
                    .iter()
                    .map(|&x| {
                        if lane.is_float() {
                            ops::float_unary(*op, lane, x)
                        } else {
                            ops::int_unary(*op, bit_width(lane), x.into()).map(|r| r as u64)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                from_lanes(ty, &out)
            }
            InstData::VectorBinary { op, args } => {
                let (a, b) = (frame.get(args[0])?, frame.get(args[1])?);
                same_type(&a, &b)?;
                vector_binary(*op, &a, &b)?
            }
            InstData::VectorReduce { op, arg } => {
                let (ty, a) = vector(&frame.get(*arg)?)?;
                let lane = ty.lane_type();
                if !lane.is_int() {
                    return invalid(format!("{op} on {ty}"));
                }
                let width = bit_width(lane);
                let mut acc = u128::from(a[0]);
                for &x in &a[1..] {
                    acc = ops::int_lane(op.binary(), width, acc, x.into())?;
                }
                Val::from_int(lane, acc)
            }
            InstData::VectorExtract { arg, lane } => {
                let (ty, a) = vector(&frame.get(*arg)?)?;
                let Some(&bits) = a.get(usize::from(*lane)) else {
                    return invalid(format!("lane {lane} of {ty}"));
                };
                lane_value(ty.lane_type(), bits)
            }
            InstData::VectorInsert { args, lane } => {
                let (ty, mut a) = vector(&frame.get(args[0])?)?;
                let scalar = frame.get(args[1])?;
                if scalar.ty() != ty.lane_type() || usize::from(*lane) >= a.len() {
                    return invalid(format!(
                        "vinsert of {} into lane {lane} of {ty}",
                        scalar.ty()
                    ));
                }
                a[usize::from(*lane)] = scalar_lane_bits(&scalar)?;
                from_lanes(ty, &a)
            }
            InstData::VectorShuffle { args, lanes } => {
                let (a, b) = (frame.get(args[0])?, frame.get(args[1])?);
                same_type(&a, &b)?;
                let ((ty, mut concat), (_, b)) = (vector(&a)?, vector(&b)?);
                concat.extend(b);
                if lanes.len() != usize::from(ty.lanes()) {
                    return invalid(format!("vshuffle of {ty} with {} indices", lanes.len()));
                }
                let out = lanes
                    .iter()
                    .map(|&i| concat.get(usize::from(i)).copied())
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| Stop::Invalid("vshuffle index out of range".into()))?;
                from_lanes(ty, &out)
            }
            InstData::VectorIcmp { cond, args } => {
                let (a, b) = (frame.get(args[0])?, frame.get(args[1])?);
                same_type(&a, &b)?;
                let ((ty, x), (_, y)) = (vector(&a)?, vector(&b)?);
                if !ty.lane_type().is_int() {
                    return invalid(format!("vicmp on {ty}"));
                }
                let width = bit_width(ty.lane_type());
                let out: Vec<_> = (0..x.len())
                    .map(|i| lane_mask(ops::icmp(*cond, width, x[i].into(), y[i].into()), width))
                    .collect();
                from_lanes(ty, &out)
            }
            InstData::VectorFcmp { cond, args } => {
                let (a, b) = (frame.get(args[0])?, frame.get(args[1])?);
                same_type(&a, &b)?;
                let ((ty, x), (_, y)) = (vector(&a)?, vector(&b)?);
                let lane = ty.lane_type();
                let width = bit_width(lane);
                let out = (0..x.len())
                    .map(|i| ops::fcmp(*cond, lane, x[i], y[i]).map(|t| lane_mask(t, width)))
                    .collect::<Result<Vec<_>, _>>()?;
                from_lanes(ty.mask(), &out)
            }
            InstData::VectorSelect { args } => {
                let (m, a, b) = (
                    frame.get(args[0])?,
                    frame.get(args[1])?,
                    frame.get(args[2])?,
                );
                same_type(&a, &b)?;
                let ((mask_ty, m), (ty, x), (_, y)) = (vector(&m)?, vector(&a)?, vector(&b)?);
                if mask_ty != ty.mask() {
                    return invalid(format!("vselect of {ty} with a {mask_ty} mask"));
                }
                let out: Vec<_> = (0..x.len())
                    .map(|i| if m[i] != 0 { x[i] } else { y[i] })
                    .collect();
                from_lanes(ty, &out)
            }
            InstData::Neon { op, .. } => {
                return unsupported(format!("NEON intrinsic {}", op.name()));
            }
            InstData::Load { flags, ty, addr } => {
                let addr = self.access_addr(&frame.get(*addr)?, flags)?;
                if let Some(trace) = &mut self.load_trace {
                    trace.push(addr);
                }
                self.load(*ty, addr)?
            }
            InstData::Store { flags, value, addr } => {
                let addr = self.access_addr(&frame.get(*addr)?, flags)?;
                let value = frame.get(*value)?;
                byte_size(value.ty())?;
                self.memory.write(addr, &value.to_bytes())?;
                return Ok(Flow::Next);
            }
            InstData::AtomicLoad { ty, addr, .. } => {
                let addr = self.atomic_addr(ptr(&frame.get(*addr)?)?, *ty)?;
                self.load(*ty, addr)?
            }
            InstData::AtomicStore { value, addr, .. } => {
                let value = frame.get(*value)?;
                let addr = self.atomic_addr(ptr(&frame.get(*addr)?)?, value.ty())?;
                self.memory.write(addr, &value.to_bytes())?;
                return Ok(Flow::Next);
            }
            InstData::AtomicRmw { op, args, .. } => {
                let operand = frame.get(args[1])?;
                let ty = operand.ty();
                let addr = self.atomic_addr(ptr(&frame.get(args[0])?)?, ty)?;
                let old = self.load(ty, addr)?;
                let new = rmw(*op, &old, &operand)?;
                self.memory.write(addr, &new.to_bytes())?;
                old
            }
            // The interpreter has one thread, so a weak compare-and-swap
            // never fails spuriously, which is a legal execution.
            InstData::AtomicCas { args, .. } => {
                let (expected, new) = (frame.get(args[1])?, frame.get(args[2])?);
                same_type(&expected, &new)?;
                let addr = self.atomic_addr(ptr(&frame.get(args[0])?)?, new.ty())?;
                let old = self.load(new.ty(), addr)?;
                let stored = old == expected;
                if stored {
                    self.memory.write(addr, &new.to_bytes())?;
                }
                frame.set_results(inst, vec![old, flag(stored)])?;
                return Ok(Flow::Next);
            }
            InstData::Fence { .. } => return Ok(Flow::Next),
            InstData::VaArg { .. } | InstData::AggregateVaArg { .. } => {
                return unsupported("variadic arguments");
            }
            InstData::Call { func: callee, args } => {
                let callee = self.callee(&func.callee(*callee).name)?;
                return Ok(Flow::Call(callee, frame.get_all(args)?));
            }
            InstData::CallIndirect { callee, args, .. } => {
                let callee = self.callee_at(ptr(&frame.get(*callee)?)?)?;
                return Ok(Flow::Call(callee, frame.get_all(args)?));
            }
            InstData::Invoke {
                func: callee, args, ..
            } => {
                let callee = self.callee(&func.callee(*callee).name)?;
                return Ok(Flow::Call(callee, frame.get_all(args)?));
            }
            InstData::ReturnCall { func: callee, args } => {
                let callee = self.callee(&func.callee(*callee).name)?;
                return Ok(Flow::TailCall(callee, frame.get_all(args)?));
            }
            InstData::ReturnCallIndirect { callee, args, .. } => {
                let callee = self.callee_at(ptr(&frame.get(*callee)?)?)?;
                return Ok(Flow::TailCall(callee, frame.get_all(args)?));
            }
            InstData::Intrinsic { id, args } => {
                let args = frame.get_all(args)?;
                return self.intrinsic(frame, inst, *id, &args);
            }
            InstData::X86RflagsRegion { .. } => return unsupported("x86 FLAGS regions"),
            InstData::InlineAsm { .. } => return unsupported("inline assembly"),
            InstData::LandingPad { .. } | InstData::EhTypeid { .. } => {
                return unsupported("exception handling");
            }
            InstData::StackAddr { slot } => match frame.slots.get(slot.index()) {
                Some(&base) => Val::Ptr(base),
                None => return invalid(format!("{slot} is not declared")),
            },
            InstData::SymbolAddr { symbol } => {
                let name = func.symbol_name(*symbol);
                match self.symbols.get(name) {
                    Some(Ok(addr)) => Val::Ptr(*addr),
                    Some(Err(why)) => return unsupported(why.clone()),
                    None => {
                        let kind = match symbol {
                            Symbol::Func(_) => "function",
                            Symbol::Data(_) => "data",
                        };
                        return unsupported(format!("address of unknown {kind} @{name}"));
                    }
                }
            }
            InstData::Jump { dest } => {
                frame.jump(dest)?;
                return Ok(Flow::Next);
            }
            InstData::Brif { cond, dests } => {
                let (_, cond) = int(&frame.get(*cond)?)?;
                frame.jump(&dests[usize::from(cond == 0)])?;
                return Ok(Flow::Next);
            }
            InstData::Switch { arg, dests, cases } => {
                let (ty, x) = int(&frame.get(*arg)?)?;
                let m = mask(bit_width(ty));
                let index = cases
                    .iter()
                    .position(|&case| i128::from(case) as u128 & m == x)
                    .map_or(0, |i| i + 1);
                let Some(dest) = dests.get(index) else {
                    return invalid("switch without a destination for a case");
                };
                frame.jump(dest)?;
                return Ok(Flow::Next);
            }
            InstData::Return { args } => {
                let values = frame.get_all(args)?;
                let expected = func.sig.return_types();
                let actual: Vec<_> = values.iter().map(Val::ty).collect();
                if expected != actual {
                    return invalid(format!(
                        "@{} returns {actual:?}, but its signature says {expected:?}",
                        func.name
                    ));
                }
                return Ok(Flow::Return(values));
            }
            InstData::Trap => return Err(Trap::Unreachable.into()),
        };
        // The formats that reach here have exactly one result.
        frame.set(func.first_result(inst), value)?;
        Ok(Flow::Next)
    }

    fn access_addr(&self, addr: &Val, flags: &MemoryFlags) -> Result<u64, Stop> {
        let addr = ptr(addr)?;
        check_align(addr, u64::from(flags.align.max(1)))?;
        Ok(addr)
    }

    fn intrinsic(
        &mut self,
        frame: &mut Frame<'m>,
        inst: Inst,
        id: Intrinsic,
        args: &[Val],
    ) -> Result<Flow<'m>, Stop> {
        let info = id.info();
        if args.len() != info.params.len() {
            return invalid(format!("{id} takes {} arguments", info.params.len()));
        }
        let length = |val: &Val| int(val).map(|(_, n)| n as u64);
        let results = match id {
            Intrinsic::Memcpy | Intrinsic::Memmove => {
                let (dst, src, n) = (ptr(&args[0])?, ptr(&args[1])?, length(&args[2])?);
                if n != 0 {
                    let bytes = self.memory.read(src, n)?.to_vec();
                    self.memory.write(dst, &bytes)?;
                }
                vec![]
            }
            Intrinsic::Memset => {
                let (dst, (_, byte), n) = (ptr(&args[0])?, int(&args[1])?, length(&args[2])?);
                if n != 0 {
                    // Bounds first, so that a huge length traps instead of allocating.
                    self.memory.read(dst, n)?;
                    self.memory.write(dst, &vec![byte as u8; n as usize])?;
                }
                vec![]
            }
            Intrinsic::Memcmp => {
                let (a, b, n) = (ptr(&args[0])?, ptr(&args[1])?, length(&args[2])?);
                let mut order = 0i32;
                if n != 0 {
                    let x = self.memory.read(a, n)?;
                    let y = self.memory.read(b, n)?;
                    if let Some((p, q)) = x.iter().zip(y).find(|(p, q)| p != q) {
                        order = i32::from(*p) - i32::from(*q);
                    }
                }
                vec![Val::I32(order as u32)]
            }
            Intrinsic::StackAlloc => {
                let base = self.memory.try_alloc(length(&args[0])?, 16, false)?;
                frame.owned.push(base);
                frame.dynamic.push(base);
                vec![Val::Ptr(base)]
            }
            Intrinsic::StackSave => {
                let mark = self.memory.try_alloc(0, 16, false)?;
                frame.owned.push(mark);
                frame.dynamic.push(mark);
                vec![Val::Ptr(mark)]
            }
            Intrinsic::StackRestore => {
                let mark = ptr(&args[0])?;
                let Some(at) = frame.dynamic.iter().position(|&m| m == mark) else {
                    return invalid("stack_restore of a mark this frame did not save");
                };
                for base in frame.dynamic.drain(at + 1..) {
                    self.memory.free(base);
                }
                vec![]
            }
            Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd => {
                let live = id == Intrinsic::LifetimeStart;
                self.memory.set_live(ptr(&args[0])?, live)?;
                vec![]
            }
            _ => return unsupported(format!("intrinsic {id}")),
        };
        frame.set_results(inst, results)?;
        Ok(Flow::Next)
    }
}

/// The bits of a scalar as one vector lane.
fn scalar_lane_bits(val: &Val) -> Result<u64, Stop> {
    match val.float_parts() {
        Some((_, bits)) => Ok(bits),
        None => int(val).map(|(_, bits)| bits as u64),
    }
}

fn lane_value(lane: Type, bits: u64) -> Val {
    if lane.is_float() {
        Val::from_float(lane, bits)
    } else {
        Val::from_int(lane, bits.into())
    }
}

fn lane_mask(t: bool, width: u32) -> u64 {
    if t { mask(width) as u64 } else { 0 }
}

fn unary(op: UnaryOp, arg: &Val) -> Result<Val, Stop> {
    if let Some((ty, bits)) = arg.float_parts() {
        return Ok(Val::from_float(ty, ops::float_unary(op, ty, bits)?));
    }
    let (ty, bits) = int(arg)?;
    Ok(Val::from_int(ty, ops::int_unary(op, bit_width(ty), bits)?))
}

fn binary(op: BinaryOp, a: &Val, b: &Val) -> Result<Val, Stop> {
    if op == BinaryOp::Ptradd {
        let (base, (ty, offset)) = (ptr(a)?, int(b)?);
        if ty != Type::I64 {
            return invalid(format!("ptradd by {ty}"));
        }
        return Ok(Val::Ptr(base.wrapping_add(offset as u64)));
    }
    if op.is_float() {
        same_type(a, b)?;
        let ((ty, x), (_, y)) = (float(a)?, float(b)?);
        return Ok(Val::from_float(ty, ops::float_binary(op, ty, x, y)?));
    }
    let ((ty, x), (_, y)) = (int(a)?, int(b)?);
    let shift = matches!(
        op,
        BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr | BinaryOp::Rotl | BinaryOp::Rotr
    );
    if shift {
        // The amount may have any integer type and is taken modulo the width.
        let width = u128::from(bit_width(ty));
        return Ok(Val::from_int(
            ty,
            ops::int_binary(op, bit_width(ty), x, y % width)?,
        ));
    }
    same_type(a, b)?;
    Ok(Val::from_int(ty, ops::int_binary(op, bit_width(ty), x, y)?))
}

fn vector_binary(op: VectorBinaryOp, a: &Val, b: &Val) -> Result<Val, Stop> {
    let ((ty, x), (_, y)) = (vector(a)?, vector(b)?);
    let lane = ty.lane_type();
    let width = bit_width(lane);
    let apply = |op: VectorBinaryOp, p: u64, q: u64| -> Result<u64, Stop> {
        match ops::vector_float_op(op) {
            Some(_) if !lane.is_float() => invalid(format!("{op} on {ty}")),
            Some(scalar) => ops::float_binary(scalar, lane, p, q),
            None if lane.is_float() => invalid(format!("{op} on {ty}")),
            None => ops::int_lane(op, width, p.into(), q.into()).map(|r| r as u64),
        }
    };
    let out = if let Some(pair_op) = op.pairwise_binary() {
        let concat: Vec<u64> = x.iter().chain(&y).copied().collect();
        concat
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[p, q]| apply(pair_op, p, q))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        (0..x.len())
            .map(|i| apply(op, x[i], y[i]))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(from_lanes(ty, &out))
}

fn rmw(op: RmwOp, old: &Val, operand: &Val) -> Result<Val, Stop> {
    if op == RmwOp::Xchg {
        return Ok(operand.clone());
    }
    let ((ty, x), (_, y)) = (int(old)?, int(operand)?);
    let width = bit_width(ty);
    let (sx, sy) = (signed(x, width), signed(y, width));
    let bits = match op {
        RmwOp::Add => x.wrapping_add(y),
        RmwOp::Sub => x.wrapping_sub(y),
        RmwOp::And => x & y,
        RmwOp::Or => x | y,
        RmwOp::Xor => x ^ y,
        RmwOp::Xchg => y,
        RmwOp::Umin => x.min(y),
        RmwOp::Umax => x.max(y),
        RmwOp::Smin => sx.min(sy) as u128,
        RmwOp::Smax => sx.max(sy) as u128,
    };
    Ok(Val::from_int(ty, bits & mask(width)))
}

fn convert(op: ConvertOp, to: Type, arg: &Val) -> Result<Val, Stop> {
    let from = arg.ty();
    if from == Type::F80 || to == Type::F80 {
        return unsupported("f80 values");
    }
    Ok(match op {
        ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc => {
            let (ty, bits) = int(arg)?;
            if !to.is_int() {
                return invalid(format!("{op} to {to}"));
            }
            let bits = if op == ConvertOp::Sext {
                signed(bits, bit_width(ty)) as u128
            } else {
                bits
            };
            Val::from_int(to, bits & mask(bit_width(to)))
        }
        ConvertOp::PtrToInt => Val::I64(ptr(arg)?),
        ConvertOp::IntToPtr => match arg {
            Val::I64(addr) => Val::Ptr(*addr),
            _ => return invalid(format!("inttoptr of {from}")),
        },
        ConvertOp::Fpext | ConvertOp::Fptrunc => {
            let bits = match (arg, to) {
                (Val::F32(bits), Type::F64) => f64::from(f32::from_bits(*bits)).to_bits(),
                (Val::F64(bits), Type::F32) => (f64::from_bits(*bits) as f32).to_bits().into(),
                _ => return invalid(format!("{op} of {from} to {to}")),
            };
            Val::from_float(to, ops::canonical_nan(to, bits))
        }
        ConvertOp::Fptosi | ConvertOp::Fptoui => {
            let x = arg
                .as_f64()
                .ok_or_else(|| Stop::Invalid(format!("{op} of {from}")))?;
            // Rust's casts saturate and give 0 for NaN, as the IR specifies.
            // Widening f32 to f64 first is exact.
            match (op, to) {
                (ConvertOp::Fptosi, Type::I32) => Val::I32(x as i32 as u32),
                (ConvertOp::Fptosi, Type::I64) => Val::I64(x as i64 as u64),
                (ConvertOp::Fptoui, Type::I32) => Val::I32(x as u32),
                (ConvertOp::Fptoui, Type::I64) => Val::I64(x as u64),
                _ => return invalid(format!("{op} to {to}")),
            }
        }
        ConvertOp::Sitofp | ConvertOp::Uitofp => {
            let (ty, bits) = int(arg)?;
            if ty == Type::I128 {
                return invalid(format!("{op} of i128"));
            }
            let signed_value = signed(bits, bit_width(ty)) as i64;
            let unsigned_value = bits as u64;
            let signed_op = op == ConvertOp::Sitofp;
            match to {
                Type::F32 if signed_op => Val::f32(signed_value as f32),
                Type::F32 => Val::f32(unsigned_value as f32),
                Type::F64 if signed_op => Val::f64(signed_value as f64),
                Type::F64 => Val::f64(unsigned_value as f64),
                _ => return invalid(format!("{op} to {to}")),
            }
        }
        ConvertOp::Bitcast => {
            if from == Type::Ptr || to == Type::Ptr || byte_size(from)? != byte_size(to)? {
                return invalid(format!("bitcast of {from} to {to}"));
            }
            Val::from_bytes(to, &arg.to_bytes())
        }
        ConvertOp::SwidenLow
        | ConvertOp::UwidenLow
        | ConvertOp::SwidenHigh
        | ConvertOp::UwidenHigh
        | ConvertOp::NarrowLow
        | ConvertOp::SnarrowSatLow
        | ConvertOp::UnarrowSatLow
        | ConvertOp::SunarrowSatLow
        | ConvertOp::SpairwiseAddLong
        | ConvertOp::UpairwiseAddLong => vector_convert(op, to, arg)?,
    })
}

fn vector_convert(op: ConvertOp, to: Type, arg: &Val) -> Result<Val, Stop> {
    let (from, x) = vector(arg)?;
    let Type::Vector(to) = to else {
        return invalid(format!("{op} to {to}"));
    };
    let (from_lane, to_lane) = (from.lane_type(), to.lane_type());
    if !from_lane.is_int() || !to_lane.is_int() {
        return invalid(format!("{op} of {from} to {to}"));
    }
    let (fw, tw) = (bit_width(from_lane), bit_width(to_lane));
    let n = usize::from(to.lanes());
    let sext = |bits: u64| signed(bits.into(), fw);
    let out: Vec<u64> = match op {
        ConvertOp::SwidenLow
        | ConvertOp::UwidenLow
        | ConvertOp::SwidenHigh
        | ConvertOp::UwidenHigh => {
            if fw >= tw || x.len() < n {
                return invalid(format!("{op} of {from} to {to}"));
            }
            let start = match op {
                ConvertOp::SwidenHigh | ConvertOp::UwidenHigh => x.len() - n,
                _ => 0,
            };
            let signed_op = matches!(op, ConvertOp::SwidenLow | ConvertOp::SwidenHigh);
            x[start..start + n]
                .iter()
                .map(|&b| {
                    let wide = if signed_op { sext(b) as u128 } else { b.into() };
                    (wide & mask(tw)) as u64
                })
                .collect()
        }
        ConvertOp::NarrowLow
        | ConvertOp::SnarrowSatLow
        | ConvertOp::UnarrowSatLow
        | ConvertOp::SunarrowSatLow => {
            if fw <= tw {
                return invalid(format!("{op} of {from} to {to}"));
            }
            let (smin, smax) = (-(1i128 << (tw - 1)), (1i128 << (tw - 1)) - 1);
            let umax = mask(tw) as i128;
            let mut out: Vec<u64> = x
                .iter()
                .take(n)
                .map(|&b| {
                    let narrowed = match op {
                        ConvertOp::SnarrowSatLow => sext(b).clamp(smin, smax),
                        ConvertOp::UnarrowSatLow => i128::from(b).min(umax),
                        ConvertOp::SunarrowSatLow => sext(b).clamp(0, umax),
                        _ => i128::from(b),
                    };
                    (narrowed as u128 & mask(tw)) as u64
                })
                .collect();
            out.resize(n, 0);
            out
        }
        ConvertOp::SpairwiseAddLong | ConvertOp::UpairwiseAddLong => {
            if tw != 2 * fw || x.len() < 2 * n {
                return invalid(format!("{op} of {from} to {to}"));
            }
            let signed_op = op == ConvertOp::SpairwiseAddLong;
            (0..n)
                .map(|i| {
                    let (p, q) = (x[2 * i], x[2 * i + 1]);
                    let sum = if signed_op {
                        (sext(p) + sext(q)) as u128
                    } else {
                        u128::from(p) + u128::from(q)
                    };
                    (sum & mask(tw)) as u64
                })
                .collect()
        }
        _ => unreachable!("vector_convert of {op}"),
    };
    Ok(from_lanes(to, &out))
}
