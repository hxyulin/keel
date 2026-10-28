//! LLVM contracts whose guarantees hold at an explicit function/call boundary.
use super::*;
use keel_ir::{
    AddressCapture, CallContract, CaptureComponents, CaptureContract, IntegerRange, MemoryAccess,
    MemorySummary, PointerContract, ProvenanceCapture,
};

// LLVM CaptureInfo::toIntValue uses the upper nibble for other locations,
// lower nibble for the returned copy. Component bits encode nested capabilities.
fn decode_capture(bits: u64) -> CaptureContract {
    fn components(bits: u64) -> Option<CaptureComponents> {
        let address = match bits & 3 {
            0 => AddressCapture::None,
            1 => AddressCapture::Nullness,
            3 => AddressCapture::Address,
            _ => return None,
        };
        let provenance = match bits & 12 {
            0 => ProvenanceCapture::None,
            4 => ProvenanceCapture::Read,
            12 => ProvenanceCapture::ReadWrite,
            _ => return None,
        };
        Some(CaptureComponents {
            address,
            provenance,
        })
    }
    if bits > 255 {
        return CaptureContract::default();
    }
    match (components(bits >> 4), components(bits & 15)) {
        (Some(other), Some(returned)) => CaptureContract { other, returned },
        _ => CaptureContract::default(),
    }
}

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
    /// Only explicit load definedness establishes the non-poison result facts
    /// in this first metadata profile. Multi-interval ranges are not narrowed.
    pub(super) fn load_facts(&self, inst: Ref) -> keel_ir::ValueFacts {
        let l = self.l;
        let mut facts = keel_ir::ValueFacts::default();
        if l.metadata(inst, "noundef").is_none() {
            return facts;
        }
        let ty = l.type_of(inst);
        let width = self.width(inst);
        if l.get_type_kind(ty) == 8 && matches!(width, 8 | 16 | 32 | 64) {
            if let Some(operands) = l.metadata(inst, "range")
                && let [lower, upper] = operands.as_slice()
                && *lower != 0
                && *upper != 0
                && l.is_a_constant_int(*lower) != 0
                && l.is_a_constant_int(*upper) != 0
                && l.type_of(*lower) == ty
                && l.type_of(*upper) == ty
            {
                let range = IntegerRange {
                    lower: l.const_int_get_z_ext_value(*lower),
                    upper: l.const_int_get_z_ext_value(*upper),
                };
                if range.lower != range.upper {
                    facts.range = Some(range);
                }
            }
        } else if l.get_type_kind(ty) == 12 && l.get_pointer_address_space(ty) == 0 {
            let extent = |name| -> Option<u64> {
                let operands = l.metadata(inst, name)?;
                let [value] = operands.as_slice() else {
                    return None;
                };
                if *value == 0 || l.is_a_constant_int(*value) == 0 || self.width(*value) != 64 {
                    return None;
                }
                Some(l.const_int_get_z_ext_value(*value))
            };
            facts.align = extent("align")
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| n.is_power_of_two())
                .unwrap_or(1);
            facts.readable_bytes = extent("dereferenceable")
                .filter(|&n| n <= i64::MAX as u64)
                .unwrap_or(0);
            facts.readable_or_null_bytes = extent("dereferenceable_or_null")
                .filter(|&n| n <= i64::MAX as u64)
                .unwrap_or(0);
            facts.nonnull = facts.readable_bytes != 0 || l.metadata(inst, "nonnull").is_some();
        }
        facts
    }

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
        let mut printed_ranges = None;
        for (index, ty) in types.into_iter().enumerate() {
            if l.get_type_kind(ty) == 8
                && matches!(l.get_int_type_width(ty), 8 | 16 | 32 | 64)
                && l.attribute(owner, index as u32, "noundef", call) != 0
                && l.attribute(owner, index as u32, "range", call) != 0
            {
                // LLVM's C API exposes construction but no getter for ConstantRange
                // attributes. Read only this position in the verified canonical
                // signature. Never use LLVMGetEnumAttributeValue on a range.
                let ranges =
                    printed_ranges.get_or_insert_with(|| boundary_ranges(&l.printed(owner)));
                if let Some(Some(range)) = ranges.get(index) {
                    let positions = if index == 0 {
                        &mut contract.return_ranges
                    } else {
                        &mut contract.parameter_ranges
                    };
                    positions.push((if index == 0 { 0 } else { index as u32 - 1 }, *range));
                }
            }
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
                readable_or_null_bytes: if defined {
                    self.enum_value(owner, index, "dereferenceable_or_null", call)
                        .filter(|&n| n <= i64::MAX as u64)
                        .unwrap_or(0)
                } else {
                    0
                },
                capture: if index == 0 {
                    CaptureContract::default()
                } else if let Some(bits) = self.enum_value(owner, index, "captures", call) {
                    decode_capture(bits)
                } else if l.attribute(owner, index, "nocapture", call) != 0 {
                    CaptureContract::NONE
                } else {
                    CaptureContract::default()
                },
                return_noalias: index == 0 && l.attribute(owner, index, "noalias", call) != 0,
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

/// Return and argument ranges from one verified, canonically printed signature.
/// Missing/unsupported syntax is unknown; body text and quoted names are ignored.
fn boundary_ranges(text: &str) -> Vec<Option<IntegerRange>> {
    fn quoted_end(bytes: &[u8], mut i: usize) -> usize {
        i += 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => return i + 1,
                _ => i += 1,
            }
        }
        bytes.len()
    }
    fn range(text: &str) -> Option<IntegerRange> {
        static RANGE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\brange\(i(8|16|32|64) (-?[0-9]+), (-?[0-9]+)\)").unwrap()
        });
        let mut unquoted = text.as_bytes().to_vec();
        let mut i = 0;
        while i < unquoted.len() {
            if unquoted[i] == b'"' {
                let end = quoted_end(&unquoted, i);
                unquoted[i..end].fill(b' ');
                i = end;
            } else {
                i += 1;
            }
        }
        let unquoted = std::str::from_utf8(&unquoted).ok()?;
        let matched = RANGE.captures(unquoted)?;
        let width: u32 = matched[1].parse().ok()?;
        let mask = u64::MAX >> (64 - width);
        let lower = matched[2].parse::<i128>().ok()? as u64 & mask;
        let upper = matched[3].parse::<i128>().ok()? as u64 & mask;
        (lower != upper).then_some(IntegerRange { lower, upper })
    }
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i = quoted_end(bytes, i);
            continue;
        }
        if !matches!(bytes[i], b'@' | b'%') {
            i += 1;
            continue;
        }
        let symbol = i;
        i += 1;
        if bytes.get(i) == Some(&b'"') {
            i = quoted_end(bytes, i);
        } else {
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || b"-._$".contains(&bytes[i]))
            {
                i += 1;
            }
        }
        if bytes.get(i) != Some(&b'(') {
            continue;
        }
        let mut out = vec![range(&text[..symbol])];
        i += 1;
        let mut start = i;
        let mut depth = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'"' => {
                    i = quoted_end(bytes, i);
                    continue;
                }
                b')' if depth == 0 => {
                    if !text[start..i].trim().is_empty() {
                        out.push(range(&text[start..i]));
                    }
                    return out;
                }
                b',' if depth == 0 => {
                    out.push(range(&text[start..i]));
                    start = i + 1;
                }
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b')' | b']' | b'}' | b'>' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        return Vec::new();
    }
    Vec::new()
}
