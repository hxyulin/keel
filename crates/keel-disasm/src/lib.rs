//! Native little-endian AArch64 decoding from the checked-in Arm inventory.
//!
//! Inventory recognition is separate from assembly formatting and architectural
//! validity. Feature predicates assume all listed features are present. Reserved
//! operand combinations not expressed by inventory conditions may still match.
mod format;
mod generated;
pub mod x86;

struct Field {
    name: &'static str,
    start: u8,
    width: u8,
}

struct Encoding {
    name: &'static str,
    base: u32,
    mask: u32,
    #[allow(dead_code)] // Also used to exhaustively check overlapping predicates.
    predicate_mask: u32,
    predicate: fn(u32) -> bool,
    fields: &'static [Field],
}

/// An extracted named field, including fixed fields when the inventory names them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperandField {
    pub name: &'static str,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodingMatch {
    pub name: &'static str,
    pub fields: Vec<OperandField>,
}

impl EncodingMatch {
    pub fn field(&self, name: &str) -> Option<u32> {
        self.fields.iter().find(|f| f.name == name).map(|f| f.value)
    }
}

/// Matches describe the inventory, not CPU support or full ISA validity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recognition {
    Unknown,
    Unique(EncodingMatch),
    /// No first-match or table-order preference is applied.
    Ambiguous(Vec<EncodingMatch>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub address: u64,
    pub bytes: [u8; 4],
    pub recognition: Recognition,
}

impl Instruction {
    pub fn word(&self) -> u32 {
        u32::from_le_bytes(self.bytes)
    }

    /// Canonical base mnemonics plus a few familiar aliases. Unsupported,
    /// ambiguous and unknown words remain lossless `.inst` directives.
    pub fn assembly(&self) -> String {
        if let Recognition::Unique(encoding) = &self.recognition
            && let Some(text) = format::assembly(encoding, self.address)
        {
            return text;
        }
        format!(".inst 0x{:08x}", self.word())
    }
}

/// Decode exactly one word. The caller retains responsibility for alignment and
/// for distinguishing code from literal pools or other embedded data.
pub fn decode_word(bytes: [u8; 4], address: u64) -> Instruction {
    let word = u32::from_le_bytes(bytes);
    let mut matches = generated::ENCODINGS
        .iter()
        .filter(|e| word & e.mask == e.base && (e.predicate)(word))
        .map(|e| EncodingMatch {
            name: e.name,
            fields: e
                .fields
                .iter()
                .map(|f| OperandField {
                    name: f.name,
                    value: (word >> f.start) & (u32::MAX >> (32 - f.width)),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let recognition = match matches.len() {
        0 => Recognition::Unknown,
        1 => Recognition::Unique(matches.pop().unwrap()),
        _ => Recognition::Ambiguous(matches),
    };
    Instruction {
        address,
        bytes,
        recognition,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicates_disambiguate_all_mask_overlaps() {
        assert_eq!(generated::ENCODINGS.len(), 1670);
        let mut pairs = 0;
        for (i, a) in generated::ENCODINGS.iter().enumerate() {
            for b in &generated::ENCODINGS[i + 1..] {
                if (a.base ^ b.base) & a.mask & b.mask != 0 {
                    continue;
                }
                pairs += 1;
                // Only predicate bits unfixed by either mask can affect this
                // intersection. Enumerate every assignment, not one sample.
                let variable = (a.predicate_mask | b.predicate_mask) & !(a.mask | b.mask);
                let bits: Vec<_> = (0..32).filter(|bit| variable & (1 << bit) != 0).collect();
                assert!(bits.len() <= 16, "overlap audit must remain bounded");
                for assignment in 0..(1u32 << bits.len()) {
                    let word = bits
                        .iter()
                        .enumerate()
                        .fold(a.base | b.base, |word, (i, bit)| {
                            word | (((assignment >> i) & 1) << bit)
                        });
                    assert!(
                        !((a.predicate)(word) && (b.predicate)(word)),
                        "unresolved overlap: {} / {} at {word:08x}",
                        a.name,
                        b.name
                    );
                }
            }
        }
        assert_eq!(pairs, 81);
    }

    #[test]
    fn preserves_unknown_and_unsupported_bytes() {
        let unknown = decode_word(0u32.to_le_bytes(), 0x1000);
        assert_eq!(unknown.recognition, Recognition::Unknown);
        assert_eq!(unknown.bytes, [0; 4]);
        assert_eq!(unknown.assembly(), ".inst 0x00000000");
        let sve = decode_word(0x2518e3e0u32.to_le_bytes(), 0);
        assert!(matches!(sve.recognition, Recognition::Unique(_)));
        assert!(sve.assembly().starts_with(".inst "));
    }

    #[test]
    fn compiler_words() {
        for (word, expected) in [
            (0xd65f03c0, "ret"),
            (0xd503201f, "nop"),
            (0xa9bf7bfd, "stp x29, x30, [sp, #-16]!"),
            (0xa8c17bfd, "ldp x29, x30, [sp], #16"),
            (0x910003fd, "mov x29, sp"),
            (0xd10083ff, "sub sp, sp, #32"),
            (0xf94007e0, "ldr x0, [sp, #8]"),
            (0xf90007e0, "str x0, [sp, #8]"),
            (0x8b010000, "add x0, x0, x1"),
            (0x9b017c00, "mul x0, x0, x1"),
            (0x1e612800, "fadd d0, d0, d1"),
            (0x14000002, "b 0x1008"),
            (0x54ffffe0, "b.eq 0xffc"),
            (0x90000000, "adrp x0, 0x1000"),
            (0x52800540, "movz w0, #42"),
            (0x917ffc00, "add x0, x0, #4095, lsl #12"),
            (0xf17ffc1f, "cmp x0, #4095, lsl #12"),
            (0x3140041f, "cmn w0, #1, lsl #12"),
            (0x92401c3f, "and sp, x1, #0xff"),
            (0x32001c3f, "orr wsp, w1, #0xff"),
            (0xd2401c3f, "eor sp, x1, #0xff"),
            (0xf2401c3f, "tst x1, #0xff"),
        ] {
            let inst = decode_word(u32::to_le_bytes(word), 0x1000);
            assert_eq!(
                inst.assembly(),
                expected,
                "{word:08x}: {:?}",
                inst.recognition
            );
        }
    }
}
