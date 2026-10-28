//! LLVM contracts whose guarantees hold at an explicit function/call boundary.
use super::*;
use keel_ir::{CallContract, MemoryAccess, MemorySummary, PointerContract};

/// LLVM 23's IRMemLocation is encoded as two ModRefInfo bits per location:
/// ArgMem=0, InaccessibleMem=1, ErrnoMem=2, Other=3, TargetMem0=4, TargetMem1=5.
/// Ref=1 and Mod=2. See LLVM Support/ModRef.h, MemoryEffectsBase::toIntValue.
/// The integration tests check this encoding against the loaded LLVM parser.
fn decode_memory(bits: u64) -> MemorySummary {
    if bits & !0xfff != 0 {
        return MemorySummary::default();
    }
    fn access(bits: u64) -> MemoryAccess {
        match bits & 3 {
            0 => MemoryAccess::None,
            1 => MemoryAccess::Read,
            2 => MemoryAccess::Write,
            _ => MemoryAccess::ReadWrite,
        }
    }
    // Keel's three regions conservatively combine the more detailed LLVM regions.
    MemorySummary {
        argmem: access(bits),
        other: access((bits >> 4) | (bits >> 6)),
        inaccessible: access((bits >> 2) | (bits >> 8) | (bits >> 10)),
    }
}

impl Translation<'_> {
    fn enum_value(&self, owner: Ref, index: u32, name: &str, call: bool) -> Option<u64> {
        let attribute = self.l.attribute(owner, index, name, call);
        (attribute != 0).then(|| self.l.get_enum_attribute_value(attribute))
    }

    pub(super) fn contract(&self, owner: Ref, fty: Ref, call: bool) -> CallContract {
        let l = self.l;
        let has = |name| l.attribute(owner, u32::MAX, name, call) != 0;
        let mut contract = CallContract {
            memory: self
                .enum_value(owner, u32::MAX, "memory", call)
                .map(decode_memory)
                .unwrap_or_default(),
            nofree: has("nofree"),
            nosync: has("nosync"),
            // LLVM nocallback constrains reentry into the caller's module. It is
            // ineffective on local definitions; retain only known external targets.
            nocallback: has("nocallback")
                && if call {
                    let target = l.get_called_value(owner);
                    l.is_a_function(target) != 0 && l.get_first_basic_block(target) == 0
                } else {
                    l.get_first_basic_block(owner) == 0
                },
            ..Default::default()
        };
        let mut types = vec![l.get_return_type(fty)];
        types.extend(l.params(fty));
        for (index, ty) in types.into_iter().enumerate() {
            if l.get_type_kind(ty) != 12 || l.get_pointer_address_space(ty) != 0 {
                continue;
            }
            // Aggregate ABI transport is deliberately outside this pointer contract profile.
            if l.attribute(owner, index as u32, "sret", call) != 0
                || l.attribute(owner, index as u32, "byval", call) != 0
            {
                continue;
            }
            let index = index as u32;
            let readable = self
                .enum_value(owner, index, "dereferenceable", call)
                .filter(|&n| n > 0 && n <= i64::MAX as u64)
                .unwrap_or(0);
            let defined = readable != 0 || l.attribute(owner, index, "noundef", call) != 0;
            let align = if defined {
                self.enum_value(owner, index, "align", call)
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| n.is_power_of_two())
                    .unwrap_or(1)
            } else {
                1
            };
            let pointer = PointerContract {
                align,
                nonnull: defined
                    && (readable != 0 || l.attribute(owner, index, "nonnull", call) != 0),
                readable_bytes: readable,
                no_capture: index != 0
                    && (self.enum_value(owner, index, "captures", call) == Some(0)
                        || l.attribute(owner, index, "nocapture", call) != 0),
            };
            if pointer == PointerContract::default() {
                continue;
            }
            if index == 0 {
                contract.returns.push((0, pointer));
            } else {
                contract.parameters.push((index - 1, pointer));
            }
        }
        contract
    }
}
