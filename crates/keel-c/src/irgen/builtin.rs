//! Builtin lowering, CPU feature queries and atomics.

use super::*;

// CAS IR has one ordering: the success order also supplies a legally stronger
// failure order. Runtime orders conservatively select seq_cst.
fn atomic_order(order: i64, load: bool, store: bool) -> Ordering {
    match order {
        0 => Ordering::Relaxed,
        1 | 2 if store => Ordering::Relaxed,
        1 | 2 => Ordering::Acquire,
        3 if load => Ordering::Relaxed,
        3 => Ordering::Release,
        4 if load => Ordering::Acquire,
        4 if store => Ordering::Release,
        4 => Ordering::AcqRel,
        _ => Ordering::SeqCst,
    }
}

impl<'g, 'p, 'f> FnGen<'g, 'p, 'f> {
    fn cpu_cpuid(&mut self, leaf: u32) -> Value {
        let leaf = self.b.ins().iconst(Type::I32, i64::from(leaf));
        let subleaf = self.b.ins().iconst(Type::I32, 0);
        let inst = self
            .b
            .ins()
            .intrinsic(Intrinsic::X86Cpuid, &[leaf, subleaf]);
        self.b.function().inst_results(inst)[0]
    }

    fn cpu_register(&mut self, cpuid: Value, shift: i64) -> Value {
        let shift = self.b.ins().iconst(Type::I128, shift);
        let shifted = self.b.ins().ushr(cpuid, shift);
        self.b.ins().trunc(Type::I32, shifted)
    }

    fn cpu_mask(&mut self, register: Value, mask: u32) -> Value {
        let mask = self.b.ins().iconst(Type::I32, i64::from(mask));
        let bits = self.b.ins().and(register, mask);
        self.b.ins().icmp(IntCC::Eq, bits, mask)
    }

    fn cpu_supports(&mut self, feature: &crate::x86_cpu::Feature) -> Value {
        let maximum = self.cpu_cpuid(if feature.leaf >= 0x8000_0000 {
            0x8000_0000
        } else {
            0
        });
        let maximum = self.b.ins().trunc(Type::I32, maximum);
        let leaf = self.b.ins().iconst(Type::I32, i64::from(feature.leaf));
        let exists = self.b.ins().icmp(IntCC::Uge, maximum, leaf);
        let available = self.new_block();
        let done = self.new_block();
        let result = self.b.append_block_param(done, Type::I32);
        let absent = self.b.ins().iconst(Type::I32, 0);
        self.b.ins().brif(exists, available, &[], done, &[absent]);
        self.b.seal_block(available);
        self.switch_to(available);
        let queried = self.cpu_cpuid(feature.leaf);
        let register = self.cpu_register(queried, feature.register.shift());
        let mut supported = self.cpu_mask(register, feature.mask);
        if let Some(state_mask) = feature.state.xcr0_mask() {
            let leaf1 = if feature.leaf == 1 {
                queried
            } else {
                self.cpu_cpuid(1)
            };
            let ecx = self.cpu_register(leaf1, 64);
            let avx = self.cpu_mask(ecx, crate::x86_cpu::AVX_CPUID);
            // Keep XGETBV behind all CPU prerequisites, even though the query
            // intrinsic independently checks CPU/OS XSAVE permission.
            let executable = self.b.ins().and(supported, avx);
            let state = self.new_block();
            self.b.ins().brif(executable, state, &[], done, &[absent]);
            self.b.seal_block(state);
            self.switch_to(state);
            let xcr0_inst = self.b.ins().intrinsic(Intrinsic::X86CpuXcr0, &[]);
            let xcr0 = self.b.function().inst_results(xcr0_inst)[0];
            let mask = self.b.ins().iconst(Type::I64, state_mask as i64);
            let enabled = self.b.ins().and(xcr0, mask);
            let enabled = self.b.ins().icmp(IntCC::Eq, enabled, mask);
            supported = self.b.ins().and(supported, avx);
            supported = self.b.ins().and(supported, enabled);
        }
        let supported = self.b.ins().uext(Type::I32, supported);
        self.b.ins().jump(done, &[supported]);
        self.b.seal_block(done);
        self.switch_to(done);
        result
    }

    pub(super) fn builtin(
        &mut self,
        op: Builtin,
        args: &[Expr<'p>],
        e: &Expr<'p>,
    ) -> R<Option<Value>> {
        if let Builtin::X86CpuSupports(feature) = op {
            return Ok(Some(self.cpu_supports(feature)));
        }
        if op == Builtin::X86CpuInit {
            return Ok(None);
        }
        // The expectation is usually a literal, with nothing to evaluate.
        if op == Builtin::Hint {
            let v = self.val(&args[0])?;
            for a in &args[1..] {
                self.discard(a)?;
            }
            return Ok(Some(v));
        }
        if op == Builtin::Prefetch {
            // A prefetch is advisory and must not become a load from the
            // supplied address. Keep calls and volatile reads that compute it.
            self.discard(&args[0])?;
            return Ok(None);
        }
        // C lvalue conversions and compound operators synthesize these builtins
        // without passing through the explicit builtin parser's alignment gate.
        if matches!(
            op,
            Builtin::AtomicLoad
                | Builtin::AtomicStore
                | Builtin::AtomicRmw { .. }
                | Builtin::AtomicFloatRmw { .. }
        ) && let ExprKind::Addr(obj) = args[0].kind
        {
            let align = match obj.kind {
                ExprKind::Local(id) | ExprKind::CompoundLit(id, _) => self.def.locals[id].align,
                ExprKind::Global(id) => self.g.p.globals[id].align,
                ExprKind::Member { align, .. } => align,
                _ => self.g.p.type_alignment(&obj.ty, &obj.sugar),
            };
            if self
                .g
                .types()
                .atomic_size(obj.ty.unatomic(), align)
                .is_none()
            {
                return Err(Error::new(e.loc, "atomic object is not naturally aligned"));
            }
        }
        let mut vals = Vec::new();
        for a in args.iter() {
            vals.push(self.val(a)?);
        }
        if let Builtin::Memory(id) = op
            && args.len() == 4
        {
            return self.checked_memory_builtin(id, args, &vals, e).map(Some);
        }
        if let Builtin::AtomicGeneric { kind, size } = op {
            let ty = match size {
                1 => Type::I8,
                2 => Type::I16,
                4 => Type::I32,
                _ => Type::I64,
            };
            let order_index = if kind == GenericAtomicOp::CompareExchange {
                4
            } else {
                args.len() - 1
            };
            let order = if kind == GenericAtomicOp::CompareExchange
                && crate::eval::eval_int(&args[5]).is_none()
            {
                5
            } else {
                crate::eval::eval_int(&args[order_index]).unwrap_or(5)
            };
            let ordering = atomic_order(
                order,
                kind == GenericAtomicOp::Load,
                kind == GenericAtomicOp::Store,
            );
            let flags = |i: usize| MemoryFlags {
                volatile: self.g.object_volatile(
                    args[i].ty.pointee().unwrap(),
                    &args[i].sugar.pointee(self.g.p.typedefs),
                ),
                ..MemoryFlags::default()
            };
            let expected_flags = flags(1);
            let desired_flags = flags(if kind == GenericAtomicOp::CompareExchange {
                2
            } else {
                1
            });
            match kind {
                GenericAtomicOp::Load => {
                    let value = self.b.ins().atomic_load(ty, ordering, vals[0]);
                    self.b
                        .ins()
                        .store_with_flags(value, vals[1], expected_flags);
                }
                GenericAtomicOp::Store => {
                    let value = self.b.ins().load_with_flags(ty, vals[1], desired_flags);
                    self.b.ins().atomic_store(ordering, value, vals[0]);
                }
                GenericAtomicOp::Exchange => {
                    let value = self.b.ins().load_with_flags(ty, vals[1], desired_flags);
                    let old = self
                        .b
                        .ins()
                        .atomic_rmw(RmwOp::Xchg, ordering, vals[0], value);
                    let flags = MemoryFlags {
                        volatile: self.g.object_volatile(
                            args[2].ty.pointee().unwrap(),
                            &args[2].sugar.pointee(self.g.p.typedefs),
                        ),
                        ..MemoryFlags::default()
                    };
                    self.b.ins().store_with_flags(old, vals[2], flags);
                }
                GenericAtomicOp::CompareExchange => {
                    let expected = self.b.ins().load_with_flags(ty, vals[1], expected_flags);
                    let desired = self.b.ins().load_with_flags(ty, vals[2], desired_flags);
                    let old = self
                        .b
                        .ins()
                        .atomic_cas(ordering, vals[0], expected, desired);
                    return Ok(Some(self.atomic_expected(
                        old,
                        expected,
                        vals[1],
                        expected_flags,
                    )));
                }
            }
            return Ok(None);
        }
        if let Builtin::AtomicFloatRmw { op, new } = op {
            let ty = e.ty;
            let float = ty.is_float();
            let bits = if !float {
                ty.ir_with_layout(self.g.p.c_layout)
            } else if ty == Ty::Float {
                Type::I32
            } else {
                Type::I64
            };
            let first = self.b.ins().atomic_load(bits, Ordering::SeqCst, vals[0]);
            let retry = self.new_block();
            let done = self.new_block();
            let expected = self.b.append_block_param(retry, bits);
            self.b.ins().jump(retry, &[first]);
            self.switch_to(retry);
            let old = if float {
                self.b.ins().convert(
                    ConvertOp::Bitcast,
                    ty.ir_with_layout(self.g.p.c_layout),
                    expected,
                )
            } else {
                expected
            };
            let x = self.convert(old, &ty, &args[1].ty, e.loc)?;
            let value = self.arith(op, &args[1].ty, x, vals[1], e.loc)?;
            let value = self.convert(value, &args[1].ty, &ty, e.loc)?;
            let desired = if float {
                self.b.ins().convert(ConvertOp::Bitcast, bits, value)
            } else {
                value
            };
            let found = self
                .b
                .ins()
                .atomic_cas(Ordering::SeqCst, vals[0], expected, desired);
            let success = self.b.ins().icmp(IntCC::Eq, found, expected);
            self.b.ins().brif(success, done, &[], retry, &[found]);
            self.b.seal_block(retry);
            self.b.seal_block(done);
            self.switch_to(done);
            return Ok(Some(if new { value } else { old }));
        }
        let ins = self.b.ins();
        Ok(Some(match op {
            Builtin::AtomicGeneric { .. }
            | Builtin::AtomicFloatRmw { .. }
            | Builtin::X86CpuSupports(_)
            | Builtin::X86CpuInit => unreachable!(),
            Builtin::Memory(id) => {
                // All arguments were evaluated once above, including at
                // length zero. Copy/fill intrinsics return dst in C; a
                // comparison builtin returns the intrinsic's integer result.
                if id == Intrinsic::Memset {
                    vals[1] = ins.trunc(Type::I8, vals[1]);
                }
                let inst = self.b.ins().intrinsic(id, &vals);
                if id.info().returns.is_empty() {
                    vals[0]
                } else {
                    self.b.function().inst_results(inst)[0]
                }
            }
            Builtin::Pclmulqdq(id, control) => {
                let mode = ins.iconst(Type::I32, i64::from(control));
                let inst = self.b.ins().intrinsic(id, &[vals[0], vals[1], mode]);
                self.b.function().inst_results(inst)[0]
            }
            Builtin::X86Aes(id)
            | Builtin::X86PackedShift(id)
            | Builtin::X86Saturating(id)
            | Builtin::X86PackedConversion(id)
            | Builtin::X86Value(id) => {
                let inst = ins.intrinsic(id, &vals);
                if let Some(&result) = self.b.function().inst_results(inst).first() {
                    result
                } else {
                    // Void expressions carry a placeholder that no C value can use.
                    self.b.ins().iconst(Type::I32, 0)
                }
            }
            Builtin::VectorFloatCompare(cc) => {
                let mask = ins.vfcmp(cc, vals[0], vals[1]);
                self.b.ins().convert(
                    ConvertOp::Bitcast,
                    e.ty.ir_with_layout(self.g.p.c_layout),
                    mask,
                )
            }
            Builtin::VectorReduce(op) => ins.vector_reduce(op, vals[0]),
            Builtin::VectorSplat => ins.vsplat(e.ty.ir_with_layout(self.g.p.c_layout), vals[0]),
            Builtin::VectorShuffle => ins.vshuffle(
                vals[0],
                vals[1],
                args[2..]
                    .iter()
                    .map(|e| crate::eval::eval_int(e).unwrap() as u8)
                    .collect(),
            ),
            Builtin::Complex => {
                let Ty::Complex(elem) = e.ty else {
                    unreachable!()
                };
                self.complex_pair(vals[0], vals[1], elem)
            }
            Builtin::Bswap => ins.bswap(vals[0]),
            Builtin::Fabs => ins.fabs(vals[0]),
            Builtin::Overflow(op) => {
                let (value, overflow) = ins.overflow(op, vals[0], vals[1]);
                let flags = self.object_flags(
                    args[2].ty.pointee().unwrap(),
                    &args[2].sugar.pointee(self.g.p.typedefs),
                );
                self.b.ins().store_with_flags(value, vals[2], flags);
                overflow
            }
            Builtin::GenericOverflow(op) => self.generic_overflow(op, args, &vals, e.loc)?,
            Builtin::Hint => vals[0],
            Builtin::Prefetch => unreachable!("prefetch lowered before value operands"),
            Builtin::AssumeAligned(alignment) => {
                // Keep the contract local to this expression's SSA result.
                // Reusing vals[0] would leak a branch-local assumption to
                // unrelated uses of the original pointer.
                let zero = ins.iconst(Type::I64, 0);
                let result = self.b.ins().ptradd(vals[0], zero);
                let offset = args.get(1).and_then(crate::eval::eval_int);
                let align = match (args.len(), offset) {
                    (1, _) => alignment,
                    (_, Some(offset)) => {
                        alignment.min(1u64 << (offset as u64).trailing_zeros().min(63))
                    }
                    _ => 1,
                };
                self.alignments.insert(result, align.min(1 << 31) as u32);
                result
            }
            Builtin::Signbit => {
                let (int, bits) = match args[0].ty {
                    Ty::Float => (Type::I32, 31),
                    _ => (Type::I64, 63),
                };
                let x = ins.convert(ConvertOp::Bitcast, int, vals[0]);
                let shift = self.b.ins().iconst(int, bits);
                let sign = self.b.ins().binary(BinaryOp::Ushr, x, shift);
                if int == Type::I64 {
                    self.b.ins().trunc(Type::I32, sign)
                } else {
                    sign
                }
            }
            Builtin::Clz | Builtin::Ctz | Builtin::Popcount => {
                let r = match op {
                    Builtin::Clz => ins.clz(vals[0]),
                    Builtin::Ctz => ins.ctz(vals[0]),
                    _ => ins.popcnt(vals[0]),
                };
                if self.b.function().value_type(r) == Type::I64 {
                    self.b.ins().trunc(Type::I32, r)
                } else {
                    r
                }
            }
            Builtin::Ffs => {
                let ty = self.b.function().value_type(vals[0]);
                let zero = self.b.ins().iconst(ty, 0);
                let nonzero = self.b.ins().icmp(IntCC::Ne, vals[0], zero);
                // IR ctz is defined at zero (returns its bit width), so both
                // select arms are safe to evaluate without introducing a branch.
                let count = self.b.ins().ctz(vals[0]);
                let count = if ty == Type::I64 {
                    self.b.ins().trunc(Type::I32, count)
                } else {
                    count
                };
                let one = self.b.ins().iconst(Type::I32, 1);
                let position = self.b.ins().binary(BinaryOp::Iadd, count, one);
                let zero = self.b.ins().iconst(Type::I32, 0);
                self.b.ins().select(nonzero, position, zero)
            }
            Builtin::Alloca => {
                if self.def.has_vm {
                    return Err(Error::new(
                        self.def.loc,
                        "alloca in a function containing VLAs is not supported yet",
                    ));
                }
                let inst = ins.intrinsic(Intrinsic::StackAlloc, &vals);
                self.b.function().inst_results(inst)[0]
            }
            Builtin::AsmAggregateBits => {
                // Aggregate values are addresses of their complete byte
                // representation. Their evaluation already preserves volatile
                // aggregate reads and materializes returned values once.
                self.b
                    .ins()
                    .load(e.ty.ir_with_layout(self.g.p.c_layout), vals[0])
            }
            Builtin::StackPointer => {
                let inst = ins.intrinsic(Intrinsic::StackSave, &[]);
                let stack = self.b.function().inst_results(inst)[0];
                self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, stack)
            }
            Builtin::ReturnAddress(depth) => {
                let inst = ins.intrinsic(Intrinsic::FrameAddress, &[]);
                let mut frame = self.b.function().inst_results(inst)[0];
                for _ in 0..depth {
                    frame = self.b.ins().load(Type::Ptr, frame);
                }
                // The x86 frame intrinsic prevents frameless emission. RBP
                // names the saved caller RBP; the return slot follows it.
                let slot = self.offset_by(frame, 8);
                self.b.ins().load(Type::Ptr, slot)
            }
            Builtin::FrameAddress(depth) => {
                let inst = ins.intrinsic(Intrinsic::FrameAddress, &[]);
                let mut frame = self.b.function().inst_results(inst)[0];
                // Native frame records begin with the caller's saved RBP/x29.
                // Nonzero depths require readable caller frame records; do not
                // fabricate a frame or replace a missing chain with null.
                for _ in 0..depth {
                    frame = self.b.ins().load(Type::Ptr, frame);
                }
                frame
            }
            Builtin::AtomicLoad
            | Builtin::AtomicStore
            | Builtin::AtomicRmw { .. }
            | Builtin::AtomicCas
            | Builtin::AtomicCasBool
            | Builtin::AtomicCompareExchange
            | Builtin::Fence => {
                // `__ATOMIC_RELAXED` 0 ... `__ATOMIC_SEQ_CST` 5; consume
                // (1) is acquire. A load cannot release nor a store
                // acquire, so those orders weaken to what applies.
                let order_arg = if op == Builtin::AtomicCompareExchange {
                    &args[3]
                } else {
                    args.last().unwrap()
                };
                let order = if op == Builtin::AtomicCompareExchange
                    && crate::eval::eval_int(&args[4]).is_none()
                {
                    5
                } else {
                    crate::eval::eval_int(order_arg).unwrap_or(5)
                };
                let load = matches!(op, Builtin::AtomicLoad);
                let store = matches!(op, Builtin::AtomicStore);
                let ordering = atomic_order(order, load, store);
                let value_ty = args[0]
                    .ty
                    .pointee()
                    .map(|t| *t.unatomic())
                    .unwrap_or(Ty::Void);
                let float = value_ty.is_float();
                let repr = if value_ty == Ty::Float {
                    Type::I32
                } else {
                    Type::I64
                };
                // Floating operands enter integer atomic IR by bitcast only.
                if float {
                    let range = match op {
                        Builtin::AtomicStore | Builtin::AtomicRmw { .. } => 1..2,
                        Builtin::AtomicCas | Builtin::AtomicCasBool => 1..3,
                        Builtin::AtomicCompareExchange => 2..3,
                        _ => 0..0,
                    };
                    for i in range {
                        vals[i] = self.b.ins().convert(ConvertOp::Bitcast, repr, vals[i]);
                    }
                }
                let ins = self.b.ins();
                match op {
                    Builtin::AtomicLoad => {
                        let v = ins.atomic_load(
                            if float {
                                repr
                            } else {
                                e.ty.ir_with_layout(self.g.p.c_layout)
                            },
                            ordering,
                            vals[0],
                        );
                        if float {
                            self.b.ins().convert(
                                ConvertOp::Bitcast,
                                e.ty.ir_with_layout(self.g.p.c_layout),
                                v,
                            )
                        } else {
                            v
                        }
                    }
                    Builtin::AtomicStore => {
                        ins.atomic_store(ordering, vals[1], vals[0]);
                        if e.ty == Ty::Void {
                            return Ok(None);
                        }
                        if float {
                            self.b.ins().convert(
                                ConvertOp::Bitcast,
                                value_ty.ir_with_layout(self.g.p.c_layout),
                                vals[1],
                            )
                        } else {
                            vals[1]
                        }
                    }
                    Builtin::AtomicRmw { op, new } => {
                        let old = ins.atomic_rmw(op, ordering, vals[0], vals[1]);
                        if !new {
                            return Ok(Some(if float {
                                self.b.ins().convert(
                                    ConvertOp::Bitcast,
                                    value_ty.ir_with_layout(self.g.p.c_layout),
                                    old,
                                )
                            } else {
                                old
                            }));
                        }
                        let bin = match op {
                            RmwOp::Add => BinaryOp::Iadd,
                            RmwOp::Sub => BinaryOp::Isub,
                            RmwOp::And => BinaryOp::And,
                            RmwOp::Or => BinaryOp::Or,
                            RmwOp::Xor => BinaryOp::Xor,
                            _ => unreachable!("the parser only asks for these"),
                        };
                        self.b.ins().binary(bin, old, vals[1])
                    }
                    Builtin::AtomicCas => {
                        let v = ins.atomic_cas(ordering, vals[0], vals[1], vals[2]);
                        if float {
                            self.b.ins().convert(
                                ConvertOp::Bitcast,
                                value_ty.ir_with_layout(self.g.p.c_layout),
                                v,
                            )
                        } else {
                            v
                        }
                    }
                    Builtin::AtomicCasBool => {
                        let old = ins.atomic_cas(ordering, vals[0], vals[1], vals[2]);
                        self.b.ins().icmp(IntCC::Eq, old, vals[1])
                    }
                    Builtin::AtomicCompareExchange => {
                        let ty = if float {
                            repr
                        } else {
                            args[2].ty.ir_with_layout(self.g.p.c_layout)
                        };
                        let flags = MemoryFlags {
                            volatile: args[1]
                                .sugar
                                .pointee(self.g.p.typedefs)
                                .is_volatile(self.g.p.typedefs),
                            ..MemoryFlags::default()
                        };
                        let expected = ins.load_with_flags(ty, vals[1], flags);
                        let old = self
                            .b
                            .ins()
                            .atomic_cas(ordering, vals[0], expected, vals[2]);
                        self.atomic_expected(old, expected, vals[1], flags)
                    }
                    _ => {
                        if order != 0 {
                            ins.fence(ordering);
                        }
                        return Ok(None);
                    }
                }
            }
            Builtin::Debugtrap => {
                ins.intrinsic(Intrinsic::Debugtrap, &[]);
                return Ok(None);
            }
            Builtin::Trap => {
                ins.trap();
                self.terminated = true;
                self.ensure_block();
                return Ok(None);
            }
        }))
    }

    /// The expected buffer is written only on failure (observable for volatile).
    fn atomic_expected(
        &mut self,
        old: Value,
        expected: Value,
        ptr: Value,
        flags: MemoryFlags,
    ) -> Value {
        let success = self.b.ins().icmp(IntCC::Eq, old, expected);
        let fail = self.new_block();
        let done = self.new_block();
        self.b.ins().brif(success, done, &[], fail, &[]);
        self.b.seal_block(fail);
        self.switch_to(fail);
        self.b.ins().store_with_flags(old, ptr, flags);
        self.jump_to(done);
        self.b.seal_block(done);
        self.switch_to(done);
        success
    }

    // ---- memory ----
}
