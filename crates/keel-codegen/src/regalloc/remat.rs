//! Immutable recipes for values whose stack homes can be replaced by recomputation.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum RematRecipe {
    Iconst(u64),
    StackAddr(keel_ir::StackSlot),
}

pub(crate) fn remat_recipe(func: &Function, value: Value) -> Option<RematRecipe> {
    if value.index() >= func.num_values() || func.resolve(value) != value {
        return None;
    }
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return None;
    };
    if func.is_removed(inst) {
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
}

#[cfg(feature = "experimental-regalloc")]
impl RegisterAllocator for RematerializingAllocator<'_> {
    fn name(&self) -> &'static str {
        "rematerializing"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let pressure = self.prefer_spilling
            && matches!(self.base, AllocationTier::Fast | AllocationTier::Full);
        let eligible = if pressure {
            backend_scope!(Allocation);
            Some(self.eligible(input))
        } else {
            None
        };
        let mut allocation = if let Some(eligible) = &eligible {
            allocate_tier_remat(input, *self.base, eligible)
        } else {
            self.base.allocate(input)
        };
        // Separate scopes avoid counting the base allocation twice.
        backend_scope!(Allocation);
        profiling::scope!("rematerialization");
        if allocation.spill_slots == 0
            || !allocation.edits.is_empty()
            || (!self.constants && !self.stack_addresses)
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
            for loc in allocation.locs.iter().flatten().flatten() {
                if let Loc::Stack(s) = *loc {
                    used[s as usize] = true;
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

#[cfg(all(test, feature = "experimental-regalloc"))]
mod tests {
    use super::*;
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
