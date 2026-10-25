//! Keep two-word aggregate wrappers in SSA after proving identical SysV
//! placements. This is backend-local: public signatures and binding stay intact.
//! Only private virtual homes may be accessed; no pointer substitution, capture
//! assumption, external memory access, or movement across a trapping read occurs.
use super::sysv::{self, ArgLoc, RegPiece, Slot};
use crate::regalloc::Loc;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{
    AbiParam, BinaryOp, CallConv, ConvertOp, Function, InstData, Layout, Purpose, Signature, Type,
    Value,
};

fn pair(layout: &Layout) -> bool {
    matches!(layout, Layout::Array(t, 2) if **t == Layout::Scalar(Type::I64))
        || matches!(layout, Layout::Struct(fs) if fs.as_slice() == [Layout::Scalar(Type::I64), Layout::Scalar(Type::I64)])
}

fn signature(old: &Signature) -> Option<Signature> {
    if old.cc != CallConv::C
        || old.variadic
        || old.returns_twice
        || old.x86_vector_abi.is_some()
        || !old.returns.is_empty()
        || old.params.len() > 16
        || !matches!(&old.params.first()?.purpose, Purpose::Sret(l) if pair(l))
    {
        return None;
    }
    let mut new = old.clone();
    new.params = old.params[1..]
        .iter()
        .map(|p| match &p.purpose {
            Purpose::ByVal(l) if pair(l) => Some(AbiParam::new(Type::I128)),
            Purpose::Normal
                if matches!(
                    p.ty,
                    Type::Ptr | Type::I8 | Type::I16 | Type::I32 | Type::I64
                ) =>
            {
                Some(p.clone())
            }
            _ => None,
        })
        .collect::<Option<_>>()?;
    new.returns = vec![AbiParam::new(Type::I128)];
    let placement = |s: &Signature| {
        sysv::signature_arg_locs(
            s,
            &s.params
                .iter()
                .map(|p| sysv::signature_param_kind(s, p))
                .collect::<Vec<_>>(),
        )
    };
    let before = placement(old);
    let after = placement(&new);
    if before.stack != after.stack
        || before.stack_align != after.stack_align
        || before.ints != after.ints
        || before.floats != after.floats
        || !matches!(&before.locs[0], ArgLoc::SretRegs(r) if r.as_slice() == [RegPiece { offset: 0, loc: Loc::Reg(sysv::RAX) }, RegPiece { offset: 8, loc: Loc::Reg(sysv::RDX) }])
    {
        return None;
    }
    for (a, b) in before.locs[1..].iter().zip(&after.locs) {
        let same = match (a, b) {
            (ArgLoc::InRegs(r), ArgLoc::Value(Slot::Regs(lo, Some(hi)))) => {
                r.as_slice()
                    == [
                        RegPiece {
                            offset: 0,
                            loc: *lo,
                        },
                        RegPiece {
                            offset: 8,
                            loc: *hi,
                        },
                    ]
            }
            (ArgLoc::OnStack(a), ArgLoc::Value(Slot::Stack(b))) => a == b,
            _ => a == b,
        };
        if !same {
            return None;
        }
    }
    Some(new)
}

struct Builder {
    f: Function,
    block: keel_ir::Block,
    values: FxHashMap<Value, Value>,
    addresses: FxHashMap<Value, (usize, usize)>,
    objects: Vec<Vec<Option<Value>>>,
    pairs: FxHashMap<[Value; 2], Value>,
    constants: FxHashMap<Value, i64>,
}
impl Builder {
    fn emit(&mut self, d: InstData) -> Value {
        let i = self.f.append_inst(self.block, d);
        self.f.first_result(i)
    }
    fn object(&mut self, words: usize) -> usize {
        let id = self.objects.len();
        self.objects.push(vec![None; words]);
        id
    }
    fn split(&mut self, v: Value) -> [Value; 2] {
        let lo = self.emit(InstData::Convert {
            op: ConvertOp::Trunc,
            ty: Type::I64,
            arg: v,
        });
        let shift = self.emit(InstData::Iconst {
            ty: Type::I128,
            imm: 64,
        });
        let high = self.emit(InstData::Binary {
            op: BinaryOp::Ushr,
            args: [v, shift],
        });
        let hi = self.emit(InstData::Convert {
            op: ConvertOp::Trunc,
            ty: Type::I64,
            arg: high,
        });
        self.pairs.insert([lo, hi], v);
        [lo, hi]
    }
    fn packed(&mut self, id: usize, offset: usize) -> Option<Value> {
        let words = self.objects.get(id)?;
        let pair = [(*words.get(offset / 8)?)?, (*words.get(offset / 8 + 1)?)?];
        if let Some(&v) = self.pairs.get(&pair) {
            return Some(v);
        }
        let low_zero = self.constants.get(&pair[0]) == Some(&0);
        let lo = (!low_zero).then(|| {
            self.emit(InstData::Convert {
                op: ConvertOp::Uext,
                ty: Type::I128,
                arg: pair[0],
            })
        });
        let hi = self.emit(InstData::Convert {
            op: ConvertOp::Uext,
            ty: Type::I128,
            arg: pair[1],
        });
        let shift = self.emit(InstData::Iconst {
            ty: Type::I128,
            imm: 64,
        });
        let hi = self.emit(InstData::Binary {
            op: BinaryOp::Shl,
            args: [hi, shift],
        });
        let result = if let Some(lo) = lo {
            self.emit(InstData::Binary {
                op: BinaryOp::Or,
                args: [lo, hi],
            })
        } else {
            hi
        };
        self.pairs.insert(pair, result);
        Some(result)
    }
}

pub(super) fn lower(old: &Function) -> Option<Function> {
    if old.blocks().count() != 1
        || old.layout_insts().count() > 256
        || old.num_values() > 1024
        || old.num_stack_slots() > 32
        || old.personality.is_some()
        || !old.restrict_params.is_empty()
        || old.debug_return.is_some()
        || old.debug_loc.is_some()
        || !old.debug_vars.is_empty()
        || !old.debug_scopes.is_empty()
    {
        return None;
    }
    if old
        .layout_insts()
        .any(|(_, i)| old.inst_results(i).iter().any(|&v| old.resolve(v) != v))
    {
        return None;
    }
    // Do not reinterpret API aliases on entry arguments as distinct ABI homes.
    if old
        .block_params(old.entry_block()?)
        .iter()
        .any(|&v| old.resolve(v) != v)
    {
        return None;
    }
    let sig = signature(&old.sig)?;
    let mut f = Function::new(&old.name, sig);
    f.attrs = old.attrs.clone();
    f.local = old.local;
    f.hidden = old.hidden;
    f.weak_odr = old.weak_odr;
    let block = f.create_block();
    let mut b = Builder {
        f,
        block,
        values: FxHashMap::default(),
        addresses: FxHashMap::default(),
        objects: Vec::new(),
        pairs: FxHashMap::default(),
        constants: FxHashMap::default(),
    };
    let entry = old.entry_block()?;
    let params = old.block_params(entry);
    let result_home = b.object(2);
    b.addresses.insert(old.resolve(params[0]), (result_home, 0));
    for (&v, p) in params[1..].iter().zip(&old.sig.params[1..]) {
        let ty = if matches!(p.purpose, Purpose::ByVal(_)) {
            Type::I128
        } else {
            p.ty
        };
        let new = b.f.append_block_param(block, ty);
        if matches!(p.purpose, Purpose::ByVal(_)) {
            let words = b.split(new);
            let id = b.object(2);
            b.objects[id] = words.map(Some).to_vec();
            b.addresses.insert(old.resolve(v), (id, 0));
        } else {
            b.values.insert(old.resolve(v), new);
        }
    }
    let mut slots = FxHashMap::default();
    let mut calls = 0;
    let mut returned = false;
    for (_, i) in old.layout_insts() {
        if returned {
            return None;
        }
        let d = old.inst_data(i);
        let scalar = |v: Value, b: &Builder| b.values.get(&old.resolve(v)).copied();
        let address = |v: Value, b: &Builder| b.addresses.get(&old.resolve(v)).copied();
        let mut produced = None;
        match d {
            InstData::StackAddr { slot } => {
                let size = old.stack_slots().find(|(s, _)| s == slot)?.1.size as usize;
                if size == 0 || size > 64 || size % 8 != 0 {
                    return None;
                }
                let id = *slots.entry(*slot).or_insert_with(|| b.object(size / 8));
                b.addresses
                    .insert(old.resolve(old.first_result(i)), (id, 0));
            }
            InstData::Iconst { ty, imm } => {
                let new = b.emit(InstData::Iconst { ty: *ty, imm: *imm });
                b.constants.insert(new, *imm);
                produced = Some(new);
            }
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } if address(args[0], &b).is_some() => {
                let (id, off) = address(args[0], &b)?;
                let n = usize::try_from(*b.constants.get(&scalar(args[1], &b)?)?).ok()?;
                let off = off.checked_add(n)?;
                if off > b.objects[id].len() * 8 {
                    return None;
                }
                b.addresses
                    .insert(old.resolve(old.first_result(i)), (id, off));
            }
            InstData::Load {
                ty: Type::I64,
                addr,
                flags,
            } if !flags.volatile && flags.align <= 8 => {
                let (id, off) = address(*addr, &b)?;
                if off % 8 != 0 {
                    return None;
                }
                produced = Some((*b.objects[id].get(off / 8)?)?);
            }
            InstData::Store { addr, value, flags } if !flags.volatile && flags.align <= 8 => {
                let (id, off) = address(*addr, &b)?;
                let v = scalar(*value, &b)?;
                if off % 8 != 0 || b.f.value_type(v) != Type::I64 {
                    return None;
                }
                *b.objects[id].get_mut(off / 8)? = Some(v);
            }
            InstData::Convert {
                op: ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext,
                ..
            }
            | InstData::Binary {
                op: BinaryOp::Iadd | BinaryOp::Isub | BinaryOp::And | BinaryOp::Or | BinaryOp::Xor,
                ..
            } => {
                let mut new = d.clone();
                let mut valid = true;
                new.for_each_operand_mut(|v| {
                    if let Some(n) = scalar(*v, &b) {
                        *v = n;
                    } else {
                        valid = false;
                    }
                });
                if !valid {
                    return None;
                }
                produced = Some(b.emit(new));
            }
            InstData::Call { func, args } if !args.has_explicit_abi() => {
                calls += 1;
                if calls != 1 {
                    return None;
                }
                let decl = old.callee(*func);
                let sig = signature(&decl.sig)?;
                let (id, off) = address(args[0], &b)?;
                if off % 8 != 0 || off + 16 > b.objects[id].len() * 8 {
                    return None;
                }
                let mut values = Vec::new();
                for (&v, p) in args[1..].iter().zip(&decl.sig.params[1..]) {
                    values.push(match p.purpose {
                        Purpose::ByVal(_) => {
                            let (id, off) = address(v, &b)?;
                            if off % 8 != 0 {
                                return None;
                            }
                            b.packed(id, off)?
                        }
                        _ => scalar(v, &b)?,
                    });
                }
                let callee = b.f.import_function(&decl.name, sig);
                b.f.callee_mut(callee).attrs = decl.attrs.clone();
                let result = b.emit(InstData::Call {
                    func: callee,
                    args: values.into(),
                });
                let words = b.split(result);
                b.objects[id][off / 8] = Some(words[0]);
                b.objects[id][off / 8 + 1] = Some(words[1]);
            }
            InstData::Return { args } if args.is_empty() => {
                if calls != 1 {
                    return None;
                }
                let result = b.packed(result_home, 0)?;
                b.f.append_inst(
                    block,
                    InstData::Return {
                        args: vec![result].into(),
                    },
                );
                returned = true;
            }
            _ => return None,
        }
        if let Some(new) = produced {
            b.values.insert(old.resolve(old.first_result(i)), new);
        }
    }
    if !returned {
        return None;
    }
    // Discard unused unpacking operations without making an opt -> codegen cycle.
    loop {
        let mut used = keel_ir::fxhash::FxHashSet::default();
        for (_, i) in b.f.layout_insts() {
            b.f.inst_data(i).for_each_operand(|v| {
                used.insert(b.f.resolve(v));
            });
        }
        let dead: Vec<_> =
            b.f.layout_insts()
                .filter_map(|(_, i)| {
                    let results = b.f.inst_results(i);
                    (!results.is_empty()
                        && !matches!(b.f.inst_data(i), InstData::Call { .. })
                        && results.iter().all(|v| !used.contains(v)))
                    .then_some(i)
                })
                .collect();
        if dead.is_empty() {
            break;
        }
        for i in dead {
            b.f.remove_inst(i);
        }
    }
    Some(b.f)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = include_str!("../../tests/fixtures/aggregate-call-wrapper.keel");
    fn source(s: &str) -> Function {
        let m = keel_ir::parse(s).unwrap();
        keel_ir::verify_module(&m).unwrap();
        m.functions[0].clone()
    }
    #[test]
    fn wrapper_removes_transport_without_changing_the_symbol_or_binding() {
        let original = source(SOURCE);
        let f = lower(&original).unwrap();
        keel_ir::verify(&f).unwrap();
        assert_eq!(f.name, "JS_Call");
        assert!(!f.local);
        assert_eq!(f.sig.returns[0].ty, Type::I128);
        assert_eq!(f.num_stack_slots(), 0);
        assert!(!f.layout_insts().any(|(_, i)| matches!(
            f.inst_data(i),
            InstData::Load { .. } | InstData::Store { .. }
        )));
        assert_eq!(
            f.layout_insts()
                .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
                .count(),
            1
        );
        assert_eq!(original.to_string(), source(SOURCE).to_string());
    }
    #[test]
    fn rejects_observable_or_uninitialized_memory_and_nonlocal_continuations() {
        for s in [
            SOURCE.replace("load.i64 v2", "load.i64 volatile v2"),
            SOURCE.replace("store align 8 v8, v7", "store volatile align 8 v8, v7"),
            SOURCE.replace("    store align 8 v13, v12\n", ""),
            SOURCE.replace("load.i64 v2", "load.i64 v1"),
            SOURCE.replace("v4, v5, v35)", "v4, v7, v35)"),
            SOURCE.replace("v4, v5, v35)", "v4, v6, v35)"),
            SOURCE.replace(
                "    call @JS_CallInternal",
                "    v98 = icmp.eq v7, v6\n    call @JS_CallInternal",
            ),
            SOURCE.replace(
                "    return",
                "    v99 = intrinsic frame_address()\n    return",
            ),
            SOURCE.replace(" cc(c) {", " cc(c) returns_twice {"),
        ] {
            assert!(lower(&source(&s)).is_none(), "{s}");
        }
    }
    #[test]
    fn aliased_entry_parameters_keep_the_original_lowering() {
        let mut f = source(SOURCE);
        let params = f.block_params(f.entry_block().unwrap()).to_vec();
        f.replace_all_uses(params[2], params[3]);
        assert!(lower(&f).is_none());
    }
    #[test]
    fn placement_check_rejects_stack_alignment_changes_and_non_integer_shapes() {
        let sig = source(SOURCE).sig;
        let mut changed = sig.clone();
        changed.params = vec![sig.params[0].clone()];
        changed.params.extend(vec![AbiParam::new(Type::I64); 7]);
        changed.params.push(sig.params[2].clone());
        // Seven integers place a pair at +8 in the original ABI, +16 as i128.
        assert!(signature(&changed).is_none());
        let mut aligned = changed.clone();
        aligned.params.remove(1);
        assert!(signature(&aligned).is_some());
        let mut debug = source(SOURCE);
        debug.debug_scopes.push(None);
        assert!(lower(&debug).is_none());
        let mut changed = sig.clone();
        changed.params[2].purpose =
            Purpose::ByVal(Layout::Array(Box::new(Layout::Scalar(Type::F64)), 2));
        assert!(signature(&changed).is_none());
        let mut changed = sig;
        changed.variadic = true;
        assert!(signature(&changed).is_none());
    }
}
