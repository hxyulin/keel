//! Immutable recipes for values whose stack homes can be replaced by recomputation.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum RematRecipe {
    Iconst(u64),
    NullPtr,
    StackAddr(keel_ir::StackSlot),
}

// This helper deliberately never calls Function::resolve: malformed input
// aliases must terminate here, before consumers that assume valid IR.
fn bounded_i64_input(
    mut value: Value,
    count: usize,
    mut data: impl FnMut(Value) -> (Type, ValueDef),
) -> Option<Value> {
    let mut seen = [None; 9];
    for link in 0..=8 {
        if value.index() >= count || seen[..link].contains(&Some(value)) {
            return None;
        }
        seen[link] = Some(value);
        let (ty, def) = data(value);
        if ty != Type::I64 {
            return None;
        }
        match def {
            ValueDef::Alias(next) => value = next,
            _ => return Some(value),
        }
    }
    None
}

pub(crate) fn remat_recipe(func: &Function, value: Value) -> Option<RematRecipe> {
    if value.index() >= func.num_values() {
        return None;
    }
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return None;
    };
    if inst.index() >= func.num_insts()
        || func.is_removed(inst)
        || func.inst_results(inst).first() != Some(&value)
    {
        return None;
    }
    match *func.inst_data(inst) {
        InstData::Iconst { ty, imm }
            if matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr) =>
        {
            let width = ty.bytes()? * 8;
            let bits = if width == 64 {
                imm as u64
            } else {
                (imm as u64) & ((1u64 << width) - 1)
            };
            if imm < 0 || bits != imm as u64 {
                return None;
            }
            Some(RematRecipe::Iconst(bits))
        }
        InstData::Convert {
            op: keel_ir::ConvertOp::IntToPtr,
            ty: Type::Ptr,
            arg,
        } if func.value_type(value) == Type::Ptr => {
            let arg = bounded_i64_input(arg, func.num_values(), |v| {
                (func.value_type(v), func.value_def(v))
            })?;
            let ValueDef::Result(producer, 0) = func.value_def(arg) else {
                return None;
            };
            // IntToPtr requires I64 on both supported 64-bit targets, so a
            // width-normalized zero is exactly the literal zero here.
            if producer.index() >= func.num_insts()
                || func.is_removed(producer)
                || func.inst_results(producer).first() != Some(&arg)
                || !matches!(
                    func.inst_data(producer),
                    InstData::Iconst {
                        ty: Type::I64,
                        imm: 0
                    }
                )
            {
                return None;
            }
            Some(RematRecipe::NullPtr)
        }
        InstData::StackAddr { slot } => Some(RematRecipe::StackAddr(slot)),
        _ => None,
    }
}

/// Experimental recomputation of cheap scalar spills. Existing sparse edits
/// are left unchanged because they can reference the original homes.
///
/// `prefer_spilling` additionally changes Fast/Full victim selection under
/// pressure. Both policies are opt-in; pressure preference is deliberately
/// independent of the recipe-only experiment.
#[cfg(feature = "experimental-regalloc")]
pub struct RematerializingAllocator<'a> {
    pub base: &'a AllocationTier,
    /// Prefer eligible recipes as victims when a Fast/Full pool is exhausted.
    pub prefer_spilling: bool,
    pub constants: bool,
    pub stack_addresses: bool,
    /// Null-only, postspill substitution on Fast/Full. Mixed recipe/pressure
    /// modes conservatively return the original base allocation.
    pub null_pointers: bool,
}

#[cfg(feature = "experimental-regalloc")]
impl RegisterAllocator for RematerializingAllocator<'_> {
    fn name(&self) -> &'static str {
        "rematerializing"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        if self.null_pointers
            && (self.constants
                || self.stack_addresses
                || self.prefer_spilling
                || !matches!(self.base, AllocationTier::Fast | AllocationTier::Full))
        {
            return self.base.allocate(input);
        }
        let pressure = self.prefer_spilling
            && matches!(self.base, AllocationTier::Fast | AllocationTier::Full);
        let eligible = if pressure {
            backend_scope!(Allocation);
            Some(self.eligible(input))
        } else {
            None
        };
        let allocation = if let Some(eligible) = &eligible {
            allocate_tier_remat(input, *self.base, eligible)
        } else {
            self.base.allocate(input)
        };
        self.replace_spills(input, allocation, eligible)
    }
}

#[cfg(feature = "experimental-regalloc")]
impl RematerializingAllocator<'_> {
    fn replace_spills(
        &self,
        input: &AllocationInput<'_>,
        mut allocation: Allocation,
        eligible: Option<Vec<bool>>,
    ) -> Allocation {
        // Separate scopes avoid counting the base allocation twice.
        backend_scope!(Allocation);
        profiling::scope!("rematerialization");
        if allocation.spill_slots == 0
            || !allocation.edits.is_empty()
            || (!self.constants && !self.stack_addresses && !self.null_pointers)
        {
            return allocation;
        }
        let eligible = eligible.unwrap_or_else(|| self.eligible(input));
        let mut changed = false;
        for (i, loc) in allocation.locs[0].iter_mut().enumerate() {
            if matches!(loc, Some(Loc::Stack(_))) && eligible[i] {
                let value = Value::new(i);
                *loc = Some(Loc::Remat(value));
                changed = true;
            }
        }
        if changed
            && !input
                .selection
                .constraints()
                .values()
                .any(|c| matches!(c, Constraint::Fixed(Loc::Stack(_))))
        {
            // Remove only wholly unused homes. Compressing surviving indices
            // preserves all original lifetime-sharing decisions.
            let mut used = vec![false; allocation.spill_slots as usize];
            for half in &allocation.locs {
                for (i, loc) in half.iter().enumerate() {
                    if let Some(Loc::Stack(s)) = *loc {
                        used[s as usize] = true;
                        if input.func.value_type(Value::new(i)).is_vector() {
                            // Vector homes occupy two adjacent eight-byte units.
                            // Keep both while removing unused scalar recipe homes.
                            used[s as usize + 1] = true;
                        }
                    }
                }
            }
            let mut next = 0;
            let map: Vec<_> = used
                .into_iter()
                .map(|used| {
                    let index = next;
                    next += u32::from(used);
                    index
                })
                .collect();
            for loc in allocation.locs.iter_mut().flatten().flatten() {
                if let Loc::Stack(s) = loc {
                    *s = map[*s as usize];
                }
            }
            allocation.spill_slots = next;
        }
        allocation
    }
}

#[cfg(feature = "experimental-regalloc")]
impl RematerializingAllocator<'_> {
    fn eligible(&self, input: &AllocationInput<'_>) -> Vec<bool> {
        let mut eligible: Vec<_> = (0..input.func.num_values())
            .map(|i| match remat_recipe(input.func, Value::new(i)) {
                Some(RematRecipe::Iconst(bits)) => self.constants && bits <= u16::MAX as u64,
                Some(RematRecipe::StackAddr(_)) => self.stack_addresses,
                Some(RematRecipe::NullPtr) => {
                    self.null_pointers && input.func.debug_vars.is_empty()
                }
                None => false,
            })
            .collect();
        // Preserve both sides of boundary reuse requirements.
        for (&(_, part, _), constraint) in input.selection.constraints() {
            if *constraint != Constraint::Any {
                eligible[part.value.index()] = false;
                if let Constraint::Reuse(other) = constraint {
                    eligible[other.value.index()] = false;
                }
            }
        }
        eligible
    }
}

#[cfg(test)]
mod recognition_tests {
    use super::*;

    #[test]
    fn bounded_aliases_reject_malformed_graphs_without_resolve_or_verifier() {
        for links in [0, 1, 8, 9] {
            let result = bounded_i64_input(Value::new(0), links + 1, |v| {
                (
                    Type::I64,
                    if v.index() < links {
                        ValueDef::Alias(Value::new(v.index() + 1))
                    } else {
                        ValueDef::Result(keel_ir::Inst::new(0), 0)
                    },
                )
            });
            assert_eq!(result, (links <= 8).then(|| Value::new(links)));
        }
        for defs in [
            vec![ValueDef::Alias(Value::new(0))],
            vec![
                ValueDef::Alias(Value::new(1)),
                ValueDef::Alias(Value::new(0)),
            ],
            vec![ValueDef::Alias(Value::new(99))],
        ] {
            assert!(
                bounded_i64_input(Value::new(0), defs.len(), |v| (Type::I64, defs[v.index()]))
                    .is_none()
            );
        }
        assert!(
            bounded_i64_input(Value::new(0), 2, |v| {
                (
                    if v.index() == 0 { Type::I64 } else { Type::I32 },
                    ValueDef::Alias(Value::new(1)),
                )
            })
            .is_none()
        );
        assert!(bounded_i64_input(Value::new(2), 0, |_| panic!("out of range lookup")).is_none());
    }

    #[test]
    fn exact_recipe_accepts_eight_input_aliases_and_rejects_nine() {
        for links in [0, 1, 8, 9] {
            let mut src = String::from("func @f() -> ptr {\nblock0:\n");
            for i in 0..=links {
                src.push_str(&format!(" v{i} = iconst.i64 0\n"));
            }
            let ptr = Value::new(links + 1);
            src.push_str(&format!(" {ptr} = inttoptr.ptr v0\n return {ptr}\n}}"));
            let mut module = keel_ir::parse(&src).unwrap();
            let f = &mut module.functions[0];
            for i in 0..links {
                f.replace_all_uses(Value::new(i), Value::new(i + 1));
            }
            assert_eq!(
                matches!(remat_recipe(f, ptr), Some(RematRecipe::NullPtr)),
                links <= 8
            );
        }
    }

    #[test]
    fn secondary_results_and_conversion_expressions_are_not_null_recipes() {
        for src in [
            "func @f() {\nblock0:\n v0, v1 = landingpad cleanup\n return\n}",
            "func @f() -> ptr {\nblock0:\n v0 = iconst.i32 0\n v1 = uext.i64 v0\n v2 = inttoptr.ptr v1\n return v2\n}",
            "func @f() -> ptr {\nblock0:\n v0 = iconst.i64 0\n v1 = inttoptr.ptr v0\n v2 = ptrtoint.i64 v1\n v3 = inttoptr.ptr v2\n return v3\n}",
        ] {
            let module = keel_ir::parse(src).unwrap();
            let f = &module.functions[0];
            assert!(remat_recipe(f, Value::new(f.num_values() - 1)).is_none());
        }
    }

    #[test]
    fn only_live_exact_zero_conversions_are_null_recipes() {
        for (producer, yes) in [
            ("iconst.i64 0", true),
            ("iconst.i64 1", false),
            ("iconst.i64 -9223372036854775808", false),
            ("iconst.i64 4294967296", false),
            ("iconst.i32 0", false),
            ("iconst.i8 0", false),
            ("iconst.i16 0", false),
            ("iadd v0, v0", false),
            ("load.i64 v1", false),
            ("ptrtoint.i64 v1", false),
        ] {
            // Wrong-width cases intentionally bypass verification: recognition
            // must reject them on its own.
            let src = format!(
                "func @f(i64, ptr) -> ptr {{\nblock0(v0: i64, v1: ptr):\n v2 = {producer}\n v3 = inttoptr.ptr v2\n return v3\n}}"
            );
            let mut module = keel_ir::parse(&src).unwrap();
            let f = &mut module.functions[0];
            assert_eq!(
                matches!(remat_recipe(f, Value::new(3)), Some(RematRecipe::NullPtr)),
                yes,
                "{producer}"
            );
            assert!(remat_recipe(f, Value::new(0)).is_none());
            assert!(remat_recipe(f, Value::new(f.num_values())).is_none());
            if yes {
                let ValueDef::Result(inst, _) = f.value_def(Value::new(2)) else {
                    unreachable!()
                };
                f.remove_inst(inst);
                assert!(remat_recipe(f, Value::new(3)).is_none());
            }
        }
        let mut module = keel_ir::parse("func @f() -> ptr {\nblock0:\n v0 = iconst.i64 0\n v1 = iconst.i64 0\n v2 = inttoptr.ptr v1\n v3 = inttoptr.ptr v0\n return v2\n}").unwrap();
        let f = &mut module.functions[0];
        f.replace_all_uses(Value::new(1), Value::new(0));
        assert!(matches!(
            remat_recipe(f, Value::new(2)),
            Some(RematRecipe::NullPtr)
        ));
        f.replace_all_uses(Value::new(2), Value::new(3));
        assert!(remat_recipe(f, Value::new(2)).is_none());
        let ValueDef::Result(inst, _) = f.value_def(Value::new(3)) else {
            unreachable!()
        };
        f.remove_inst(inst);
        assert!(remat_recipe(f, Value::new(3)).is_none());
    }
}

#[cfg(all(test, feature = "experimental-regalloc"))]
mod tests {
    use super::*;
    const NULL_SOURCE: &str = "func @f(ptr) -> ptr {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = iconst.i64 0
 v2 = inttoptr.ptr v1
 v3 = stack_addr ss0
 v4 = load.i32x4 v0
 store v4, v0
 return v2
}";
    const EMPTY: Pools = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    fn null_allocator(base: &AllocationTier) -> RematerializingAllocator<'_> {
        RematerializingAllocator {
            base,
            prefer_spilling: false,
            constants: false,
            stack_addresses: false,
            null_pointers: true,
        }
    }
    fn same_allocation(input: &AllocationInput<'_>, a: &Allocation, b: &Allocation) {
        assert_eq!(a.locs, b.locs);
        assert_eq!(a.spill_slots, b.spill_slots);
        assert_eq!(a.order, b.order);
        assert_eq!(a.callee_saved_used, b.callee_saved_used);
        assert_eq!(a.float_callee_saved_used, b.float_callee_saved_used);
        // Null fallback must preserve sparse bindings and transfers too.
        assert_eq!(a.edits.is_empty(), b.edits.is_empty());
        for block in input.func.blocks() {
            for &v in input.func.block_params(block) {
                assert_eq!(
                    a.param_loc(block, ValuePart::low(v)),
                    b.param_loc(block, ValuePart::low(v))
                );
            }
            for &inst in input.func.block_insts(block) {
                let edit = |alloc: &Allocation| {
                    alloc
                        .edits
                        .instruction_index(inst)
                        .map(|i| format!("{:?}", alloc.edits.instruction(i)))
                };
                assert_eq!(edit(a), edit(b));
                for successor in 0..input.func.inst_data(inst).branch_dests().len() {
                    assert_eq!(a.edits.edge(inst, successor), b.edits.edge(inst, successor));
                }
            }
        }
    }

    #[test]
    fn null_only_replaces_existing_spills_and_mixed_modes_return_base() {
        let module = keel_ir::parse(NULL_SOURCE).unwrap();
        let selection = crate::select::Selection::default();
        for base in [
            AllocationTier::Fast,
            AllocationTier::Full,
            AllocationTier::Local,
            AllocationTier::Split,
            AllocationTier::Weighted,
        ] {
            for pools in [&EMPTY, &crate::x86_64::REGISTERS] {
                let input = AllocationInput {
                    func: &module.functions[0],
                    pools,
                    selection: &selection,
                    is_call: &|_| false,
                    returns_twice: &|_| false,
                };
                let original = base.allocate(&input);
                let mut allocator = null_allocator(&base);
                let result = allocator.allocate(&input);
                if matches!(base, AllocationTier::Fast | AllocationTier::Full) {
                    match original.loc(Value::new(2)) {
                        Some(Loc::Stack(_)) => {
                            assert_eq!(result.loc(Value::new(2)), Some(Loc::Remat(Value::new(2))))
                        }
                        loc => assert_eq!(result.loc(Value::new(2)), loc),
                    }
                    for v in [1, 3] {
                        assert!(!matches!(result.loc(Value::new(v)), Some(Loc::Remat(_))));
                    }
                } else {
                    same_allocation(&input, &original, &result);
                }
                for mode in 1..8 {
                    allocator.constants = mode & 1 != 0;
                    allocator.stack_addresses = mode & 2 != 0;
                    allocator.prefer_spilling = mode & 4 != 0;
                    same_allocation(&input, &original, &allocator.allocate(&input));
                }
            }
        }
    }

    #[test]
    fn legacy_recipe_modes_do_not_enable_null_or_prefer_it_as_a_victim() {
        let module = keel_ir::parse(NULL_SOURCE).unwrap();
        let selection = crate::select::Selection::default();
        let input = AllocationInput {
            func: &module.functions[0],
            pools: &EMPTY,
            selection: &selection,
            is_call: &|_| false,
            returns_twice: &|_| false,
        };
        for base in [AllocationTier::Fast, AllocationTier::Full] {
            for mode in [1, 2, 3, 7] {
                let allocator = RematerializingAllocator {
                    base: &base,
                    constants: mode & 1 != 0,
                    stack_addresses: mode & 2 != 0,
                    prefer_spilling: mode & 4 != 0,
                    null_pointers: false,
                };
                assert!(!allocator.eligible(&input)[2]);
                assert!(matches!(
                    allocator.allocate(&input).loc(Value::new(2)),
                    Some(Loc::Stack(_))
                ));
            }
        }
    }

    #[test]
    fn constraints_debug_and_sparse_edits_keep_original_null_homes() {
        let mut module = keel_ir::parse(NULL_SOURCE).unwrap();
        for base in [AllocationTier::Fast, AllocationTier::Full] {
            let func = &module.functions[0];
            let inst = func.block_insts(func.entry_block().unwrap())[1];
            for (part, constraint) in [
                (2, Constraint::Register),
                (2, Constraint::Fixed(Loc::Reg(6))),
                (2, Constraint::Fixed(Loc::Stack(0))),
                (2, Constraint::Reuse(ValuePart::low(Value::new(1)))),
                (1, Constraint::Reuse(ValuePart::low(Value::new(2)))),
            ] {
                let mut selection = crate::select::Selection::default();
                selection.set_constraint(
                    inst,
                    ValuePart::low(Value::new(part)),
                    OperandKind::Def,
                    constraint,
                );
                let input = AllocationInput {
                    func,
                    pools: &EMPTY,
                    selection: &selection,
                    is_call: &|_| false,
                    returns_twice: &|_| false,
                };
                let original = base.allocate(&input);
                let result = null_allocator(&base).allocate(&input);
                same_allocation(&input, &original, &result);
            }
            let selection = crate::select::Selection::default();
            let input = AllocationInput {
                func,
                pools: &EMPTY,
                selection: &selection,
                is_call: &|_| false,
                returns_twice: &|_| false,
            };
            let mut original = base.allocate(&input);
            original.edits.instruction_mut(inst);
            let expected = format!("{:?}", original.edits);
            let locs = original.locs.clone();
            let result = null_allocator(&base).replace_spills(&input, original, None);
            assert_eq!(result.locs, locs);
            assert_eq!(format!("{:?}", result.edits), expected);
        }
        let var = keel_ir::DebugVar {
            name: "p".into(),
            ty: keel_ir::DebugType::new(0),
            slot: keel_ir::StackSlot::new(0),
            indirect: false,
            param: false,
            scope: None,
        };
        module.functions[0].debug_vars.push(var.clone());
        for base in [AllocationTier::Fast, AllocationTier::Full] {
            let selection = crate::select::Selection::default();
            let input = AllocationInput {
                func: &module.functions[0],
                pools: &EMPTY,
                selection: &selection,
                is_call: &|_| false,
                returns_twice: &|_| false,
            };
            same_allocation(
                &input,
                &base.allocate(&input),
                &null_allocator(&base).allocate(&input),
            );
        }
        assert_eq!(module.functions[0].debug_vars, vec![var]);
    }

    #[test]
    fn null_compaction_preserves_shared_vector_homes_and_fixed_stack_indices() {
        let module = keel_ir::parse(NULL_SOURCE).unwrap();
        let func = &module.functions[0];
        for fixed in [false, true] {
            let mut selection = crate::select::Selection::default();
            if fixed {
                selection.set_constraint(
                    func.block_insts(func.entry_block().unwrap())[2],
                    ValuePart::low(Value::new(3)),
                    OperandKind::Def,
                    Constraint::Fixed(Loc::Stack(5)),
                );
            }
            let input = AllocationInput {
                func,
                pools: &EMPTY,
                selection: &selection,
                is_call: &|_| false,
                returns_twice: &|_| false,
            };
            let mut original = AllocationTier::Full.allocate(&input);
            // Synthetic post-allocation layout isolates the home-compaction
            // contract from allocator heuristics. v1 shares the null's home;
            // the vector occupies two units after a wholly unused unit.
            original.locs[0] = vec![
                Some(Loc::Stack(0)),
                Some(Loc::Stack(2)),
                Some(Loc::Stack(2)),
                Some(Loc::Stack(5)),
                Some(Loc::Stack(3)),
            ];
            original.spill_slots = 6;
            let result =
                null_allocator(&AllocationTier::Full).replace_spills(&input, original, None);
            assert_eq!(result.loc(Value::new(2)), Some(Loc::Remat(Value::new(2))));
            assert_eq!(result.spill_slots, if fixed { 6 } else { 5 });
            assert_eq!(
                result.loc(Value::new(1)),
                Some(Loc::Stack(if fixed { 2 } else { 1 }))
            );
            assert_eq!(
                result.loc(Value::new(4)),
                Some(Loc::Stack(if fixed { 3 } else { 2 }))
            );
            assert_eq!(
                result.loc(Value::new(3)),
                Some(Loc::Stack(if fixed { 5 } else { 4 }))
            );
        }
    }

    #[test]
    fn compacting_scalar_recipes_preserves_both_vector_home_units() {
        let module = keel_ir::parse(
            "func @f(ptr) {\nblock0(v0: ptr):\n v1 = iconst.i64 7\n              v2 = load.i32x4 v0\n v3 = vsplat.i64x2 v1\n store v2, v0\n              store v3, v0\n return\n}",
        ).unwrap();
        let func = &module.functions[0];
        let pools = Pools {
            caller_saved: &[],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        let selection = crate::select::Selection::default();
        let input = AllocationInput {
            func,
            pools: &pools,
            selection: &selection,
            is_call: &|_| false,
            returns_twice: &|_| false,
        };
        for base in [AllocationTier::Fast, AllocationTier::Full] {
            let allocation = RematerializingAllocator {
                base: &base,
                prefer_spilling: true,
                constants: true,
                stack_addresses: true,
                null_pointers: false,
            }
            .allocate(&input);
            assert_eq!(
                allocation.loc(Value::new(1)),
                Some(Loc::Remat(Value::new(1)))
            );
            let homes: Vec<_> = [Value::new(2), Value::new(3)]
                .into_iter()
                .map(|v| match allocation.loc(v).unwrap() {
                    Loc::Stack(slot) => slot,
                    loc => panic!("vector should spill with empty pools: {loc:?}"),
                })
                .collect();
            assert!(homes[0].abs_diff(homes[1]) >= 2);
            assert!(homes.iter().all(|s| s + 2 <= allocation.spill_slots));
        }
    }

    #[test]
    fn recipes_replace_only_cheap_unconstrained_scalar_spills() {
        let module = keel_ir::parse(
            "func @f(i64) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: i64):
 v1 = iconst.i64 0
 v2 = iconst.i64 65535
 v3 = iconst.i64 65536
 v4 = iconst.i8 -1
 v5 = stack_addr ss0
 v6 = iadd v2, v0
 return v6
}",
        )
        .unwrap();
        let func = &module.functions[0];
        let pools = Pools {
            caller_saved: &[],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        let mut selection = crate::select::Selection::default();
        let inst = func.block_insts(func.entry_block().unwrap())[5];
        selection.set_constraint(
            inst,
            ValuePart::low(Value::new(6)),
            OperandKind::Def,
            Constraint::Reuse(ValuePart::low(Value::new(2))),
        );
        let input = AllocationInput {
            func,
            pools: &pools,
            selection: &selection,
            is_call: &|_| false,
            returns_twice: &|_| false,
        };
        let alloc = RematerializingAllocator {
            base: &AllocationTier::Full,
            prefer_spilling: false,
            constants: true,
            stack_addresses: true,
            null_pointers: false,
        }
        .allocate(&input);
        assert_eq!(alloc.loc(Value::new(1)), Some(Loc::Remat(Value::new(1))));
        for i in [0, 2, 3, 4] {
            assert!(matches!(alloc.loc(Value::new(i)), Some(Loc::Stack(_))));
        }
        assert_eq!(alloc.loc(Value::new(5)), Some(Loc::Remat(Value::new(5))));
        assert!(alloc.locs.iter().flatten().flatten().all(|&l| match l {
            Loc::Stack(s) => s < alloc.spill_slots,
            _ => true,
        }));
    }
}
