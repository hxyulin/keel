//! ARM64 compact-unwind selection from CFI, independently of directive order.
//! See LLVM libunwind's mach-o/compact_unwind_encoding.h and CompactUnwinder.hpp.
use std::collections::BTreeMap;

pub(super) enum Rule {
    Cfa(u64, u64),
    CfaOffset(u64),
    Offset(u64, i64),
    Other,
}
pub(super) fn encoding(rules: &[Rule], personality: Option<&str>) -> Option<u32> {
    // Darwin's compact personality table has only three slots. Match the
    // system assembler's policy; custom personalities retain their own CIE.
    if personality.is_some_and(|p| !matches!(p, "___gxx_personality_v0" | "___objc_personality_v0"))
    {
        return None;
    }
    let mut frame = false;
    let mut stack = None;
    let mut offsets = BTreeMap::new();
    for rule in rules {
        match *rule {
            Rule::Cfa(29, 16) if !frame && stack.is_none() => frame = true,
            Rule::CfaOffset(n) if !frame && stack.is_none() => stack = Some(n),
            Rule::Offset(r, n) if !offsets.contains_key(&r) => {
                offsets.insert(r, n);
            }
            _ => return None,
        }
    }
    if !frame {
        let stack = stack.unwrap_or(0);
        return (offsets.is_empty() && stack <= 65520 && stack % 16 == 0)
            .then_some(0x0200_0000 | ((stack / 16) as u32) << 12);
    }
    if offsets.remove(&29) != Some(-16) || offsets.remove(&30) != Some(-8) {
        return None;
    }
    let mut encoding = 0x0400_0000;
    let mut offset = -24;
    // Register pairs are packed immediately below FP/LR, X pairs before D
    // pairs, with the lower-numbered register at the higher address.
    for (low, bit) in [
        (19, 0),
        (21, 1),
        (23, 2),
        (25, 3),
        (27, 4),
        (72, 8),
        (74, 9),
        (76, 10),
        (78, 11),
    ] {
        match (offsets.remove(&low), offsets.remove(&(low + 1))) {
            (None, None) => {}
            (Some(a), Some(b)) if a == offset && b == offset - 8 => {
                encoding |= 1 << bit;
                offset -= 16;
            }
            _ => return None,
        }
    }
    offsets.is_empty().then_some(encoding)
}
