//! Target-independent vector lane legalization, before allocation.
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, MemoryFlags, Type, Value,
    VectorBinaryOp as V, VectorType,
};
use std::borrow::Cow;

fn needs_lowering(
    func: &Function,
    data: &InstData,
    keep: &impl Fn(&Function, &InstData) -> bool,
) -> bool {
    if keep(func, data) {
        return false;
    }
    match data {
        InstData::Convert {
            op: ConvertOp::Bitcast | ConvertOp::SwidenLow | ConvertOp::UwidenLow,
            ty,
            arg,
        } => ty.is_vector() || func.value_type(*arg).is_vector(),
        d => d.is_vector_instruction(),
    }
}

pub(crate) fn lower(
    func: &Function,
    keep: impl Fn(&Function, &InstData) -> bool,
) -> Cow<'_, Function> {
    if !func
        .layout_insts()
        .any(|(_, i)| needs_lowering(func, func.inst_data(i), &keep))
    {
        return Cow::Borrowed(func);
    }
    let mut f = func.clone();
    // Inputs are captured before any scalar result is produced. Reuse these
    // buffers across straight-line expansions; no optimizer runs after this pass.
    let mut slots: [Option<keel_ir::StackSlot>; 4] = [None; 4];
    let work: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    for inst in work {
        let data = f.inst_data(inst).clone();
        if !needs_lowering(&f, &data, &keep) {
            continue;
        }
        let used = match data {
            InstData::Convert {
                op: ConvertOp::SwidenLow | ConvertOp::UwidenLow,
                ..
            } => [true, false, false, true],
            InstData::Convert { .. } | InstData::VectorExtract { .. } => {
                [true, false, false, false]
            }
            InstData::VectorSplat { .. } => [false, false, false, true],
            InstData::VectorUnary { .. } | InstData::VectorInsert { .. } => {
                [true, false, false, true]
            }
            InstData::VectorSelect { .. } => [true; 4],
            _ => [true, true, false, true],
        };
        for i in 0..4 {
            if used[i] && slots[i].is_none() {
                slots[i] = Some(f.create_stack_slot(16, 16));
            }
        }
        let mut l = LaneLowering {
            func: &mut f,
            at: inst,
        };
        let first = used.iter().position(|u| *u).unwrap();
        let pointer = l.emit(InstData::StackAddr {
            slot: slots[first].unwrap(),
        });
        let mut pointers = [pointer; 4];
        for i in first + 1..4 {
            if used[i] {
                pointers[i] = l.emit(InstData::StackAddr {
                    slot: slots[i].unwrap(),
                });
            }
        }
        let result = match data {
            InstData::Convert {
                op: op @ (ConvertOp::SwidenLow | ConvertOp::UwidenLow),
                ty,
                arg,
            } => {
                let source = l.ty(arg).vector().unwrap();
                let dest = ty.vector().unwrap();
                l.store(pointers[0], arg);
                for i in 0..dest.lanes() {
                    let value = l.lane(pointers[0], i, source);
                    let value = l.emit(InstData::Convert {
                        op: if op == ConvertOp::SwidenLow {
                            ConvertOp::Sext
                        } else {
                            ConvertOp::Uext
                        },
                        ty: dest.lane_type(),
                        arg: value,
                    });
                    let at = l.address(pointers[3], i, dest);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::Convert { ty, arg, .. } => {
                l.store(pointers[0], arg);
                l.load(pointers[0], ty)
            }
            InstData::VectorSplat { ty, arg } => {
                let shape = ty.vector().unwrap();
                for i in 0..shape.lanes() {
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, arg);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorExtract { arg, lane } => {
                let shape = l.ty(arg).vector().unwrap();
                l.store(pointers[0], arg);
                l.lane(pointers[0], lane, shape)
            }
            InstData::VectorInsert { args, lane } => {
                let ty = l.ty(args[0]);
                let shape = ty.vector().unwrap();
                l.store(pointers[0], args[0]);
                for i in 0..shape.lanes() {
                    let value = if i == lane {
                        args[1]
                    } else {
                        l.lane(pointers[0], i, shape)
                    };
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorShuffle { args, lanes } => {
                let ty = l.ty(args[0]);
                let shape = ty.vector().unwrap();
                l.store(pointers[0], args[0]);
                l.store(pointers[1], args[1]);
                for (i, index) in lanes.into_iter().enumerate() {
                    let value = l.lane(
                        pointers[usize::from(index / shape.lanes())],
                        index % shape.lanes(),
                        shape,
                    );
                    let at = l.address(pointers[3], i as u8, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorUnary { op, arg } => {
                let ty = l.ty(arg);
                let shape = ty.vector().unwrap();
                l.store(pointers[0], arg);
                for i in 0..shape.lanes() {
                    let a = l.lane(pointers[0], i, shape);
                    let value = l.emit(InstData::Unary { op, arg: a });
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorSelect { args } => {
                let ty = l.ty(args[1]);
                let shape = ty.vector().unwrap();
                for i in 0..3 {
                    l.store(pointers[i], args[i]);
                }
                for i in 0..shape.lanes() {
                    let mask = l.lane(pointers[0], i, shape.mask());
                    let a = l.lane(pointers[1], i, shape);
                    let b = l.lane(pointers[2], i, shape);
                    let value = l.emit(InstData::Select { args: [mask, a, b] });
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorBinary { args, .. }
            | InstData::VectorIcmp { args, .. }
            | InstData::VectorFcmp { args, .. } => {
                let ty = l.ty(args[0]);
                let shape = ty.vector().unwrap();
                let compare = matches!(
                    data,
                    InstData::VectorIcmp { .. } | InstData::VectorFcmp { .. }
                );
                let out_shape = if compare { shape.mask() } else { shape };
                l.store(pointers[0], args[0]);
                l.store(pointers[1], args[1]);
                for i in 0..shape.lanes() {
                    let a = l.lane(pointers[0], i, shape);
                    let b = l.lane(pointers[1], i, shape);
                    let scalar = match data {
                        InstData::VectorBinary { op, .. } => InstData::Binary {
                            op: scalar_op(op),
                            args: [a, b],
                        },
                        InstData::VectorIcmp { cond, .. } => InstData::Icmp { cond, args: [a, b] },
                        InstData::VectorFcmp { cond, .. } => InstData::Fcmp { cond, args: [a, b] },
                        _ => unreachable!(),
                    };
                    let mut value = l.emit(scalar);
                    if compare {
                        let ty = out_shape.lane_type();
                        let yes = l.emit(InstData::Iconst { ty, imm: -1 });
                        let no = l.emit(InstData::Iconst { ty, imm: 0 });
                        value = l.emit(InstData::Select {
                            args: [value, yes, no],
                        });
                    }
                    let at = l.address(pointers[3], i, out_shape);
                    l.store(at, value);
                }
                l.load(pointers[3], Type::Vector(out_shape))
            }
            _ => unreachable!("unhandled vector legalization: {data:?}"),
        };
        let old = f.first_result(inst);
        f.replace_all_uses(old, result);
        f.remove_inst(inst);
    }
    debug_assert!(keel_ir::verify(&f).is_ok());
    Cow::Owned(f)
}
struct LaneLowering<'a> {
    func: &'a mut Function,
    at: Inst,
}
impl LaneLowering<'_> {
    fn emit(&mut self, data: InstData) -> Value {
        let loc = self.func.inst_loc(self.at);
        let i = self.func.insert_inst(InsertPoint::Before(self.at), data);
        self.func.set_inst_loc(i, loc);
        self.func.first_result(i)
    }
    fn ty(&self, v: Value) -> Type {
        self.func.value_type(v)
    }
    fn store(&mut self, addr: Value, value: Value) {
        let flags = MemoryFlags {
            notrap: true,
            align: self.ty(value).bytes().unwrap(),
            ..MemoryFlags::default()
        };
        let loc = self.func.inst_loc(self.at);
        let i = self.func.insert_inst(
            InsertPoint::Before(self.at),
            InstData::Store { flags, value, addr },
        );
        self.func.set_inst_loc(i, loc);
    }
    fn load(&mut self, addr: Value, ty: Type) -> Value {
        self.emit(InstData::Load {
            flags: MemoryFlags {
                notrap: true,
                align: ty.bytes().unwrap(),
                ..MemoryFlags::default()
            },
            ty,
            addr,
        })
    }
    fn address(&mut self, p: Value, lane: u8, shape: VectorType) -> Value {
        if lane == 0 {
            return p;
        }
        let offset = self.emit(InstData::Iconst {
            ty: Type::I64,
            imm: i64::from(lane) * i64::from(shape.lane_type().bytes().unwrap()),
        });
        self.emit(InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [p, offset],
        })
    }
    fn lane(&mut self, p: Value, lane: u8, shape: VectorType) -> Value {
        let at = self.address(p, lane, shape);
        self.load(at, shape.lane_type())
    }
}
fn scalar_op(op: V) -> BinaryOp {
    match op {
        V::Iadd => BinaryOp::Iadd,
        V::Isub => BinaryOp::Isub,
        V::Imul => BinaryOp::Imul,
        V::And => BinaryOp::And,
        V::Or => BinaryOp::Or,
        V::Xor => BinaryOp::Xor,
        V::Shl => BinaryOp::Shl,
        V::Ushr => BinaryOp::Ushr,
        V::Sshr => BinaryOp::Sshr,
        V::Fadd => BinaryOp::Fadd,
        V::Fsub => BinaryOp::Fsub,
        V::Fmul => BinaryOp::Fmul,
        V::Fdiv => BinaryOp::Fdiv,
        V::Fmin => BinaryOp::Fmin,
        V::Fmax => BinaryOp::Fmax,
        V::Fcopysign => BinaryOp::Fcopysign,
    }
}
