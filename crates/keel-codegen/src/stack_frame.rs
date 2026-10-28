//! Storage for declared IR stack objects, separate from allocator spill homes.
use keel_ir::{EntityRef, Function, InstData};
use std::ops::Index;

/// Indexed by the original SlotId, including holes with no frame storage.
/// Address emission must never turn an omitted home into a valid offset.
pub(crate) struct SlotOffsets(Vec<Option<usize>>);

impl SlotOffsets {
    pub(crate) fn layout(func: &Function, mut offset: usize, debug: bool) -> (Self, usize) {
        // Inspect IR before target folding: folded loads/stores and rematerialized
        // addresses still need the object. Keeping every surviving StackAddr is
        // conservative, including escapes, aliases and unused address results.
        // Debug emission indexes slots independently of executing instructions;
        // retain all declared homes whenever this module emits debug information.
        let mut used = vec![debug; func.num_stack_slots()];
        for block in func.blocks() {
            for &inst in func.block_insts(block) {
                match *func.inst_data(inst) {
                    InstData::StackAddr { slot } | InstData::AggregateVaArg { home: slot, .. } => {
                        used[slot.index()] = true
                    }
                    _ => {}
                }
            }
        }
        let mut offsets = vec![None; used.len()];
        for (id, slot) in func.stack_slots() {
            if used[id.index()] {
                offset = offset.next_multiple_of(slot.align as usize);
                offsets[id.index()] = Some(offset);
                offset += slot.size as usize;
            }
        }
        (Self(offsets), offset)
    }

    /// Only debug frames have an observable home for every declared slot.
    pub(crate) fn debug_offsets(&self) -> impl Iterator<Item = usize> + '_ {
        self.0
            .iter()
            .map(|offset| offset.expect("missing debug stack home"))
    }
}

impl Index<usize> for SlotOffsets {
    type Output = usize;

    fn index(&self, index: usize) -> &usize {
        self.0[index]
            .as_ref()
            .expect("address of omitted stack slot")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_slots_keep_identity_alignment_and_debug_homes() {
        let module = keel_ir::parse(
            "func @f() -> ptr {
                ss0 = stack_slot 4096, align 16
                ss1 = stack_slot 3, align 1
                ss2 = stack_slot 65536, align 16
                ss3 = stack_slot 16, align 16
            block0:
                v0 = stack_addr ss1
                v1 = stack_addr ss3
                return v1
            }",
        )
        .unwrap();
        let f = &module.functions[0];
        let (slots, end) = SlotOffsets::layout(f, 8, false);
        assert_eq!(slots.0, [None, Some(8), None, Some(16)]);
        assert_eq!(end, 32);
        // Even an address with no uses is retained; address escape analysis is
        // deliberately outside frame layout's responsibility.
        assert_eq!(slots[1], 8);
        assert_eq!(slots[3], 16);
        let (debug, end) = SlotOffsets::layout(f, 8, true);
        assert_eq!(
            debug.debug_offsets().collect::<Vec<_>>(),
            [16, 4112, 4128, 69664]
        );
        assert_eq!(end, 69680);
    }

    #[test]
    fn removed_address_alias_keeps_only_its_surviving_home() {
        let mut module = keel_ir::parse(
            "func @f() -> ptr {
                ss0 = stack_slot 65536, align 16
                ss1 = stack_slot 8, align 8
            block0:
                v0 = stack_addr ss0
                v1 = stack_addr ss1
                return v0
            }",
        )
        .unwrap();
        let f = &mut module.functions[0];
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        f.replace_all_uses(f.first_result(insts[0]), f.first_result(insts[1]));
        f.remove_inst(insts[0]);
        let (slots, end) = SlotOffsets::layout(f, 0, false);
        assert_eq!(slots.0, [None, Some(0)]);
        assert_eq!(end, 8);
        // Each emitter resolves aliases, including when folding an address.
        for target in crate::Target::NATIVE {
            crate::compile_with_options(&module, target, crate::SelectionOptions::ALL).unwrap();
        }
    }

    #[test]
    #[should_panic(expected = "address of omitted stack slot")]
    fn omitted_storage_cannot_be_used_as_an_address() {
        let mut f = Function::new("f", keel_ir::Signature::new(&[], &[]));
        f.create_stack_slot(8, 8);
        let (slots, _) = SlotOffsets::layout(&f, 0, false);
        let _ = slots[0];
    }
}
