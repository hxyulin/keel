//! Opt-in boundaries retained through layout; bytes come from patched sections.
use super::{Fix, Section};
use keel_asm_ir::x86;
use std::ops::Range;

/// Final section-relative encoding layout, before linking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EncodingReport {
    /// Assembler sections, indexed independently of ELF section-table indices.
    pub sections: Vec<EncodingSection>,
    /// Disjoint regions covering each section, ordered by section and offset.
    pub regions: Vec<EncodingRegion>,
}

/// One input or assembler-generated section.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EncodingSection {
    pub name: String,
    /// Logical size, including NOBITS reservations.
    pub size: u64,
    pub executable: bool,
    pub nobits: bool,
}

/// Origin of a byte range; encoded NOP padding is not a parsed instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegionKind {
    Instruction,
    Alignment,
    Data,
}

/// One final instruction, alignment range, or remaining data/reservation range.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EncodingRegion {
    pub kind: RegionKind,
    pub section: usize,
    /// One-based source line or program item; zero for unattributed/generated data.
    pub line: usize,
    pub offset: u64,
    pub size: u64,
    /// Final object bytes for instructions; empty for data and padding.
    pub bytes: Vec<u8>,
    /// Canonical parsed instruction, before layout-dependent operand folding.
    pub instruction: Option<String>,
    /// Actual leading legacy, REX/REX2, VEX or EVEX prefix bytes.
    pub prefixes: Vec<u8>,
    pub branch: Option<BranchEncoding>,
    /// Fields still requiring linker relocation; their bytes are placeholders.
    pub relocations: Vec<RelocationField>,
}

/// A direct PC-relative branch, including calls and loop branches.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BranchEncoding {
    pub displacement_bytes: u8,
    /// Signed distance from instruction end; unknown until linking for relocations.
    pub displacement: Option<i64>,
    /// Resolved target offset in the instruction's section, when representable.
    pub target_offset: Option<u64>,
}

/// A linker relocation field inside an instruction, in ELF x86 relocation terms.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RelocationField {
    /// Offset relative to the instruction start.
    pub offset: u64,
    pub width: u8,
    pub kind: u32,
}

pub(super) struct Span {
    region: EncodingRegion,
    pub line: usize,
    fixes: Range<usize>,
    direct_branch: bool,
}
#[derive(Default)]
pub(super) struct Collector {
    pub spans: Vec<Span>,
    pub unresolved: Vec<(usize, u64, u8, u32)>,
}

fn region(kind: RegionKind, section: usize, offset: u64, size: u64) -> EncodingRegion {
    EncodingRegion {
        kind,
        section,
        line: 0,
        offset,
        size,
        bytes: Vec::new(),
        instruction: None,
        prefixes: Vec::new(),
        branch: None,
        relocations: Vec::new(),
    }
}

impl Collector {
    pub fn alignment(&mut self, section: usize, offset: u64, size: u64) {
        if size != 0 {
            self.spans.push(Span {
                region: region(RegionKind::Alignment, section, offset, size),
                line: 0,
                fixes: 0..0,
                direct_branch: false,
            });
        }
    }
    pub fn record(
        &mut self,
        section: usize,
        offset: u64,
        size: u64,
        line: usize,
        instruction: Option<&x86::Instruction>,
        fixes: Range<usize>,
    ) {
        let Some(instruction) = instruction else {
            return;
        };
        let mut record = region(RegionKind::Instruction, section, offset, size);
        let mut text = String::new();
        x86::write_instruction(
            &mut text,
            instruction.opcode,
            &instruction.operands,
            instruction.lock,
        )
        .expect("validated instruction formats into String");
        record.instruction = Some(text);
        self.spans.push(Span {
            region: record,
            line,
            fixes,
            direct_branch: instruction.opcode.is_direct_branch()
                && matches!(
                    instruction.operands.as_slice(),
                    [x86::Operand::Immediate(_)]
                ),
        });
    }
    pub fn finish(mut self, sections: &[Section], fixes: &[Fix]) -> EncodingReport {
        self.unresolved
            .sort_by_key(|&(section, offset, _, _)| (section, offset));
        let mut regions = Vec::new();
        for mut span in self.spans {
            let r = &mut span.region;
            r.line = span.line;
            if r.kind == RegionKind::Instruction {
                let bytes =
                    &sections[r.section].data[r.offset as usize..(r.offset + r.size) as usize];
                r.bytes = bytes.to_vec();
                r.prefixes = bytes[..prefix_length(bytes)].to_vec();
                // Match final relocation fields by position, independently of
                // whether encoding came from a cache or a layout-dependent fold.
                let start = self.unresolved.partition_point(|&(section, offset, _, _)| {
                    (section, offset) < (r.section, r.offset)
                });
                for &(_, offset, width, kind) in
                    self.unresolved[start..]
                        .iter()
                        .take_while(|&&(section, offset, _, _)| {
                            section == r.section && offset < r.offset + r.size
                        })
                {
                    r.relocations.push(RelocationField {
                        offset: offset - r.offset,
                        width,
                        kind,
                    });
                }
                if span.direct_branch
                    && let Some(f) = fixes[span.fixes].first()
                {
                    let field = f.offset - r.offset;
                    let displacement = if r.relocations.iter().any(|r| {
                        r.offset < field + u64::from(f.width)
                            && r.offset + u64::from(r.width) > field
                    }) {
                        None
                    } else {
                        let at = (f.offset - r.offset) as usize;
                        match f.width {
                            1 => Some(i64::from(bytes[at] as i8)),
                            4 => Some(i64::from(i32::from_le_bytes(
                                bytes[at..at + 4].try_into().unwrap(),
                            ))),
                            _ => None,
                        }
                    };
                    r.branch = Some(BranchEncoding {
                        displacement_bytes: f.width,
                        displacement,
                        target_offset: displacement.and_then(|d| {
                            u64::try_from(i128::from(r.offset + r.size) + i128::from(d)).ok()
                        }),
                    });
                }
            }
            regions.push(span.region);
        }
        regions.sort_by_key(|r| (r.section, r.offset));
        let mut covered = Vec::new();
        let mut regions = regions.into_iter().peekable();
        for (section, s) in sections.iter().enumerate() {
            let mut offset = 0;
            while regions.peek().is_some_and(|r| r.section == section) {
                let r = regions.next().unwrap();
                if offset < r.offset {
                    covered.push(region(RegionKind::Data, section, offset, r.offset - offset));
                }
                offset = r.offset + r.size;
                covered.push(r);
            }
            if offset < s.size {
                covered.push(region(RegionKind::Data, section, offset, s.size - offset));
            }
        }
        EncodingReport {
            sections: sections
                .iter()
                .map(|s| EncodingSection {
                    name: s.name.clone(),
                    size: s.size,
                    executable: s.flags & 4 != 0,
                    nobits: s.nobits,
                })
                .collect(),
            regions: covered,
        }
    }
}

// Instruction boundaries come from the parser, never from decoding section data.
fn prefix_length(bytes: &[u8]) -> usize {
    let mut at = 0;
    while bytes.get(at).is_some_and(|b| {
        matches!(
            b,
            0xf0 | 0xf2 | 0xf3 | 0x2e | 0x36 | 0x3e | 0x26 | 0x64 | 0x65 | 0x66 | 0x67 | 0x40
                ..=0x4f
        )
    }) {
        at += 1;
    }
    let extra = match bytes.get(at) {
        Some(0xc5 | 0xd5) => 2,
        Some(0xc4) => 3,
        Some(0x62) => 4,
        _ => 0,
    };
    (at + extra).min(bytes.len())
}
