//! A small A64 assembler for Apple arm64 Mach-O objects.
//!
//! Instruction bit layouts come from Arm's machine-readable specification.
//! Syntax, operand transformations, and object layout are implemented here.
//! DWARF 4 line tables, debug data, and ordinary call-frame directives are
//! supported, including Darwin exception personality/LSDA references.
//! Symbol-defined subsections support linker reordering and dead stripping.
//! Unsupported instructions/directives are errors.
mod generated;
mod macho;
mod parser;

use std::fmt;

/// An assembly diagnostic with a one-based source line (zero for object errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl Error {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line != 0 {
            write!(f, "line {}: ", self.line)?;
        }
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

/// A defined text symbol or an undefined external symbol. Names are preserved, including Darwin's `_` prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// Offset within the defining section; `None` for undefined symbols.
    pub offset: Option<u64>,
    /// Zero is __TEXT,__text; subsequent indices address Assembly::sections.
    pub section: Option<usize>,
    pub global: bool,
    /// Tentative common definition: byte size and power-of-two alignment.
    pub common: Option<(u64, u8)>,
}

/// Darwin arm64 instruction relocation types (Apple's arm64/reloc.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RelocationKind {
    Unsigned = 0,
    Branch26 = 2,
    Page21 = 3,
    PageOff12 = 4,
    GotPage21 = 5,
    GotPageOff12 = 6,
    /// PC-relative 32-bit data pointer to a linker-created GOT slot.
    PointerToGot = 7,
    TlvpPage21 = 8,
    TlvpPageOff12 = 9,
}
impl RelocationKind {
    fn pc_relative(self) -> bool {
        matches!(
            self,
            Self::Branch26 | Self::Page21 | Self::GotPage21 | Self::TlvpPage21 | Self::PointerToGot
        )
    }
    fn accepts_addend(self) -> bool {
        matches!(self, Self::Branch26 | Self::Page21 | Self::PageOff12)
    }
}

/// A symbol reference to be resolved by the linker. Instruction addends use
/// ARM64_RELOC_ADDEND pairs; data addends are serialized into the data word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocation {
    pub section: usize,
    /// Number of relocated bytes: 4 for instructions, 4/8 for data.
    pub size: u8,
    /// A data difference uses SUBTRACTOR(subtract_symbol) then UNSIGNED(symbol).
    pub subtract_symbol: Option<String>,
    pub offset: u32,
    pub symbol: String,
    pub kind: RelocationKind,
    pub addend: i64,
}

pub use keel_asm_ir::SectionKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionContents {
    Bytes(Vec<u8>),
    ZeroFill(u64),
}

/// An additional section. The text section remains in Assembly::text;
/// this vector has no duplicate copy of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub segment: String,
    pub name: String,
    pub alignment: u32,
    pub kind: SectionKind,
    pub contents: SectionContents,
}
impl Section {
    pub fn size(&self) -> u64 {
        match &self.contents {
            SectionContents::Bytes(bytes) => bytes.len() as u64,
            SectionContents::ZeroFill(size) => *size,
        }
    }
    pub fn data(&self) -> &[u8] {
        match &self.contents {
            SectionContents::Bytes(bytes) => bytes,
            SectionContents::ZeroFill(_) => &[],
        }
    }
}

/// Mach-O LC_BUILD_VERSION deployment target and SDK (packed xxxx.yy.zz).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildVersion {
    pub platform: u32,
    pub minimum: u32,
    pub sdk: u32,
}
impl Default for BuildVersion {
    fn default() -> Self {
        Self {
            platform: 1,
            minimum: 11 << 16,
            sdk: 0,
        }
    }
}

/// A Mach-O linker optimization hint referencing text instruction offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkerOptimizationHint {
    pub kind: u8,
    pub offsets: [u64; 3],
}
impl LinkerOptimizationHint {
    fn argument_count(&self) -> Result<usize, Error> {
        match self.kind {
            1 | 2 | 7 | 8 => Ok(2),
            3..=6 => Ok(3),
            _ => Err(Error::new(0, "invalid linker optimization hint kind")),
        }
    }
}

/// Encoded text, extra data sections, and symbols before writing the object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembly {
    pub build_version: BuildVersion,
    pub linker_optimization_hints: Vec<LinkerOptimizationHint>,
    /// Permit the Mach-O linker to reorder/dead-strip symbol-defined atoms.
    pub subsections_via_symbols: bool,
    pub text: Vec<u8>,
    /// Section alignment as a power of two (at least 2 for A64).
    pub text_alignment: u32,
    pub symbols: Vec<Symbol>,
    pub relocations: Vec<Relocation>,
    /// Extra sections, indexed from one in symbols and relocations.
    pub sections: Vec<Section>,
}

impl Assembly {
    /// Write a little-endian arm64 Mach-O relocatable object.
    pub fn to_macho(&self) -> Result<Vec<u8>, Error> {
        macho::write(self)
    }
}

/// Assemble A64 syntax, resolving local branches/ADR and retaining linker relocations.
pub fn assemble(source: &str) -> Result<Assembly, Error> {
    parser::assemble(source, false, TargetFeatures::default()).map(|(assembly, _)| assembly)
}

/// A source instruction's location in the assembled text (excluding padding/data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstructionLocation {
    pub line: usize,
    pub offset: usize,
}

/// Assemble while retaining instruction locations for metrics and CPU modeling.
/// Words must be read from the final text after branch/address fixups.
pub fn assemble_with_metrics(source: &str) -> Result<(Assembly, Vec<InstructionLocation>), Error> {
    parser::assemble(source, true, TargetFeatures::default())
}

/// Assemble source directly into an Apple arm64 `.o` file.
pub fn assemble_object(source: &str) -> Result<Vec<u8>, Error> {
    assemble(source)?.to_macho()
}

use keel_asm_ir::FieldId;

/// Optional ISA features added to the fixed Apple FP/NEON/LSE baseline.
///
/// Selection permits encoding; it does not detect hardware, enable OS support,
/// or manage streaming mode/ZA state. In particular, SME does not imply SVE
/// outside streaming mode on Apple CPUs. Callers must arrange execution safely.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TargetFeatures {
    sve: bool,
    sme: bool,
    numeric: u16,
    compat: u16,
    matrix: u8,
}
impl TargetFeatures {
    /// Permit SVE encodings. This does not imply SME.
    pub const fn with_sve(mut self) -> Self {
        self.sve = true;
        self
    }
    /// Permit SME and instructions available in streaming SVE mode through SME.
    /// This does not enable non-streaming SVE or SME2.
    pub const fn with_sme(mut self) -> Self {
        self.sme = true;
        self
    }
    /// Permit SME2, including SME and streaming vector instructions.
    pub const fn with_sme2(mut self) -> Self {
        self.sme = true;
        self.matrix |= 1;
        self
    }
    /// Permit double-precision SME arithmetic, independently of SME2.
    pub const fn with_sme_f64f64(mut self) -> Self {
        self.sme = true;
        self.matrix |= 2;
        self
    }
    /// Permit 16-bit integer outer products into 64-bit SME tiles.
    pub const fn with_sme_i16i64(mut self) -> Self {
        self.sme = true;
        self.matrix |= 4;
        self
    }
    /// Permit non-widening FP16 SME arithmetic, including SME2.
    pub const fn with_sme_f16f16(mut self) -> Self {
        self = self.with_sme2();
        self.matrix |= 8;
        self
    }
    /// Permit non-widening BF16 SME arithmetic, including SME2.
    pub const fn with_sme_b16b16(mut self) -> Self {
        self = self.with_sme2();
        self.matrix |= 16;
        self
    }
    /// Permit half-precision arithmetic.
    pub const fn with_fp16(mut self) -> Self {
        self.numeric |= 1 << 0;
        self
    }
    /// Permit widening FP16 multiply-accumulate.
    pub const fn with_fhm(mut self) -> Self {
        self.numeric |= 1 << 1;
        self
    }
    /// Permit BF16 conversions and arithmetic.
    pub const fn with_bf16(mut self) -> Self {
        self.numeric |= 1 << 2;
        self
    }
    /// Permit integer dot products.
    pub const fn with_dotprod(mut self) -> Self {
        self.numeric |= 1 << 3;
        self
    }
    /// Permit integer matrix multiply and mixed-sign dot products.
    pub const fn with_i8mm(mut self) -> Self {
        self.numeric |= 1 << 4;
        self
    }
    /// Permit rounding doubling multiply-accumulate.
    pub const fn with_rdm(mut self) -> Self {
        self.numeric |= 1 << 5;
        self
    }
    /// Permit complex arithmetic (H forms also require FP16).
    pub const fn with_fcma(mut self) -> Self {
        self.numeric |= 1 << 6;
        self
    }
    /// Permit JavaScript-compatible floating-point conversion.
    pub const fn with_jscvt(mut self) -> Self {
        self.numeric |= 1 << 7;
        self
    }
    /// Permit bounded 32/64-bit FP rounding.
    pub const fn with_frintts(mut self) -> Self {
        self.numeric |= 1 << 8;
        self
    }
    /// Select the M5 features currently modeled by this assembler: the Apple
    /// baseline, numeric extensions, SME/SME2 and optional matrix element types,
    /// PAC/BTI, AES/PMULL, SHA-1/SHA-256, CRC32, CSSC and RCpc loads. This does
    /// not enable ordinary SVE or claim coverage of unimplemented M5 families.
    /// Selection is for encoding only; execution still needs OS/hardware checks.
    pub const fn apple_m5() -> Self {
        Self {
            sve: false,
            sme: true,
            numeric: (1 << 9) - 1,
            compat: 511,
            matrix: 31,
        }
    }
    /// Permit pointer authentication encodings (without enabling an ABI).
    pub const fn with_pauth(mut self) -> Self {
        self.compat |= 1;
        self
    }
    /// Permit branch target identification.
    pub const fn with_bti(mut self) -> Self {
        self.compat |= 2;
        self
    }
    /// Permit AES round instructions.
    pub const fn with_aes(mut self) -> Self {
        self.compat |= 4;
        self
    }
    /// Permit SHA-1 instructions.
    pub const fn with_sha1(mut self) -> Self {
        self.compat |= 8;
        self
    }
    /// Permit SHA-256 instructions.
    pub const fn with_sha256(mut self) -> Self {
        self.compat |= 16;
        self
    }
    /// Permit CRC32 and CRC32C instructions.
    pub const fn with_crc32(mut self) -> Self {
        self.compat |= 32;
        self
    }
    /// Permit 64-bit polynomial multiply long.
    pub const fn with_pmull(mut self) -> Self {
        self.compat |= 64;
        self
    }
    /// Permit common short sequence compression instructions.
    pub const fn with_cssc(mut self) -> Self {
        self.compat |= 128;
        self
    }
    /// Permit RCpc acquire loads.
    pub const fn with_lrcpc(mut self) -> Self {
        self.compat |= 256;
        self
    }
    fn supports(self, feature: &str) -> bool {
        TARGET_FEATURES.contains(&feature)
            || [
                "FEAT_PAuth",
                "FEAT_BTI",
                "FEAT_AES",
                "FEAT_SHA1",
                "FEAT_SHA256",
                "FEAT_CRC32",
                "FEAT_PMULL",
                "FEAT_CSSC",
                "FEAT_LRCPC",
            ]
            .iter()
            .position(|f| *f == feature)
            .is_some_and(|i| self.compat & (1 << i) != 0)
            || (feature == "FEAT_SVE" && self.sve)
            || (feature == "FEAT_SME" && self.sme)
            || [
                "FEAT_SME2",
                "FEAT_SME_F64F64",
                "FEAT_SME_I16I64",
                "FEAT_SME_F16F16",
                "FEAT_SME_B16B16",
            ]
            .iter()
            .position(|f| *f == feature)
            .is_some_and(|index| self.matrix & (1 << index) != 0)
            || NUMERIC_FEATURES
                .iter()
                .position(|f| *f == feature)
                .is_some_and(|index| self.numeric & (1 << index) != 0)
    }
}

/// Assemble with explicit optional ISA features; default APIs retain the baseline.
///
/// ```
/// use keel_asm::{TargetFeatures, assemble, assemble_with_features};
/// let source = "smstart\nsmstop";
/// assert!(assemble(source).is_err());
/// let features = TargetFeatures::default().with_sme();
/// assert!(assemble_with_features(source, features).is_ok());
/// ```
pub fn assemble_with_features(source: &str, features: TargetFeatures) -> Result<Assembly, Error> {
    parser::assemble(source, false, features).map(|(assembly, _)| assembly)
}

/// Assemble an Apple arm64 object with explicit optional ISA features.
pub fn assemble_object_with_features(
    source: &str,
    features: TargetFeatures,
) -> Result<Vec<u8>, Error> {
    assemble_with_features(source, features)?.to_macho()
}

/// Encode an exact shared instruction without mnemonic/operand parsing or a
/// name lookup. The same range, reserved-bit and feature checks still apply.
pub fn encode_instruction(
    instruction: &keel_asm_ir::Instruction,
    features: TargetFeatures,
) -> Result<u32, String> {
    encode_record(
        &generated::ENCODINGS[instruction.opcode.index()],
        &instruction.fields,
        features,
    )
}

/// Assemble a structured program in-process. Explicit text items alone use the
/// source parser; typed instructions directly use the generated encoder.
pub fn assemble_program(
    program: &keel_asm_ir::Program,
    features: TargetFeatures,
) -> Result<Assembly, Error> {
    parser::assemble_program(program, features)
}

pub fn assemble_program_object(
    program: &keel_asm_ir::Program,
    features: TargetFeatures,
) -> Result<Vec<u8>, Error> {
    assemble_program(program, features)?.to_macho()
}

struct Field {
    id: FieldId,
    name: &'static str,
    start: u8,
    width: u8,
}
#[derive(Debug)]
enum Requirement {
    Feature(&'static str),
    NotEqual(FieldId, u32),
    MaskedEqual(FieldId, u32, u32),
    All(&'static [Requirement]),
    Any(&'static [Requirement]),
    Not(&'static Requirement),
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Feature(feature) => f.write_str(feature),
            Self::NotEqual(field, value) => write!(f, "{field} != {value}"),
            Self::MaskedEqual(field, value, mask) => {
                write!(f, "({field} & {mask:#b}) == {value:#b}")
            }
            Self::Not(term) => write!(f, "not ({term})"),
            Self::All(terms) | Self::Any(terms) => {
                let separator = if matches!(self, Self::All(_)) {
                    " and "
                } else {
                    " or "
                };
                f.write_str("(")?;
                for (index, term) in terms.iter().enumerate() {
                    if index != 0 {
                        f.write_str(separator)?;
                    }
                    write!(f, "{term}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl Requirement {
    fn satisfied(&self, values: &[(FieldId, u32)], features: TargetFeatures) -> bool {
        match self {
            Self::Feature(feature) => features.supports(feature),
            Self::NotEqual(field, excluded) => {
                values.iter().any(|(n, v)| n == field && v != excluded)
            }
            Self::MaskedEqual(field, value, mask) => {
                values.iter().any(|(n, v)| n == field && v & mask == *value)
            }
            Self::All(terms) => terms.iter().all(|r| r.satisfied(values, features)),
            Self::Any(terms) => terms.iter().any(|r| r.satisfied(values, features)),
            Self::Not(term) => !term.satisfied(values, features),
        }
    }
}

const NUMERIC_FEATURES: &[&str] = &[
    "FEAT_FP16",
    "FEAT_FHM",
    "FEAT_BF16",
    "FEAT_DotProd",
    "FEAT_I8MM",
    "FEAT_RDM",
    "FEAT_FCMA",
    "FEAT_JSCVT",
    "FEAT_FRINTTS",
];

// Fixed Apple Silicon target: scalar FP, Advanced SIMD, and LSE atomics.
const TARGET_FEATURES: &[&str] = &["FEAT_FP", "FEAT_AdvSIMD", "FEAT_LSE"];

struct Encoding {
    name: &'static str,
    base: u32,
    mask: u32,
    fields: &'static [Field],
    requirements: &'static [Requirement],
}

/// Every variable field must be supplied once. Fixed bits within partially
/// variable fields (e.g. MOVZ wN's `hw`) remain constrained by the ISA mask.
fn encode(name: &str, values: &[(FieldId, u32)]) -> Result<u32, String> {
    encode_with_features(name, values, TargetFeatures::default())
}

fn encode_with_features(
    name: &str,
    values: &[(FieldId, u32)],
    features: TargetFeatures,
) -> Result<u32, String> {
    let encoding =
        generated::encoding(name).ok_or_else(|| format!("unsupported encoding `{name}`"))?;
    encode_record(encoding, values, features)
}

fn encode_record(
    encoding: &Encoding,
    values: &[(FieldId, u32)],
    features: TargetFeatures,
) -> Result<u32, String> {
    let name = encoding.name;
    if values.len() != encoding.fields.len() {
        return Err(format!("wrong number of fields for `{name}`"));
    }
    let mut word = encoding.base;
    for field in encoding.fields {
        let mut matches = values.iter().filter(|(n, _)| *n == field.id);
        let (_, value) = matches
            .next()
            .ok_or_else(|| format!("missing field `{}`", field.name))?;
        if matches.next().is_some() {
            return Err(format!("duplicate field `{}`", field.name));
        }
        if u64::from(*value) >= 1u64 << field.width {
            return Err(format!("field `{}` is out of range", field.name));
        }
        let span = (((1u64 << field.width) - 1) as u32) << field.start;
        word = (word & !span) | (value << field.start);
    }
    if word & encoding.mask != encoding.base {
        return Err(format!("reserved field value for `{name}`"));
    }
    for requirement in encoding.requirements {
        if !requirement.satisfied(values, features) {
            return Err(format!("`{name}` requires {requirement}"));
        }
    }
    Ok(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_opcode_ids_match_generated_encoder_record_order() {
        assert_eq!(keel_asm_ir::Opcode::ALL.len(), generated::ENCODINGS.len());
        for opcode in keel_asm_ir::Opcode::ALL {
            assert_eq!(generated::ENCODINGS[opcode.index()].name, opcode.name());
        }
    }

    #[test]
    fn numeric_fields_preserve_validation_and_diagnostic_names() {
        use FieldId::*;
        for encoding in generated::ENCODINGS {
            for field in encoding.fields {
                assert_eq!(field.name, field.id.to_string());
            }
        }
        let name = "MOVZ_64_movewide";
        let fields = generated::encoding(name).unwrap().fields;
        let mut duplicate: Vec<_> = fields.iter().map(|field| (field.id, 0)).collect();
        duplicate.pop();
        duplicate.push((fields[0].id, 0));
        assert_eq!(
            encode(name, &duplicate).unwrap_err(),
            format!("duplicate field `{}`", fields[0].name)
        );
        assert!(
            encode(name, &[(Rd, 0), (imm12, 0), (hw, 0)])
                .unwrap_err()
                .contains("missing field `imm16`")
        );
        assert!(
            encode(name, &[(Rd, 32), (imm16, 0), (hw, 0)])
                .unwrap_err()
                .contains("field `Rd` is out of range")
        );
        assert!(
            encode("MOVZ_32_movewide", &[(Rd, 0), (imm16, 0), (hw, 2)])
                .unwrap_err()
                .contains("reserved field value")
        );
    }

    #[test]
    fn m5_profile_models_numeric_features_and_streaming_sme_without_sve() {
        let m5 = TargetFeatures::apple_m5();
        for feature in NUMERIC_FEATURES {
            assert!(m5.supports(feature), "{feature}");
        }
        assert!(m5.supports("FEAT_SME"));
        assert!(!m5.supports("FEAT_SVE"));
        assert!(m5.supports("FEAT_SME2"));
        assert!(!m5.supports("FEAT_SME2p1"));
        for feature in NUMERIC_FEATURES {
            assert!(!TargetFeatures::default().supports(feature));
        }
    }

    #[test]
    fn streaming_cases_match_each_selected_generated_form() {
        let mut seen = std::collections::BTreeSet::new();
        for line in include_str!("../tests/streaming-cases.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
        {
            let (name, source) = line.split_once('\t').unwrap();
            assert!(seen.insert(name));
            let e = generated::encoding(name).unwrap();
            let bytes = assemble_with_features(source, TargetFeatures::apple_m5())
                .unwrap()
                .text;
            let word = u32::from_le_bytes(bytes.try_into().unwrap());
            assert_eq!(word & e.mask, e.base, "{name}");
        }
        assert_eq!(seen.len(), 46);
    }

    #[test]
    fn every_selected_sme_record_has_a_matching_gated_case() {
        use std::collections::BTreeSet;
        let expected: BTreeSet<_> = include_str!("../isa/sme.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
            .collect();
        let mut covered = BTreeSet::new();
        for line in include_str!("../tests/sme-cases.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
        {
            let (name, source) = line.split_once('\t').unwrap();
            assert!(covered.insert(name), "duplicate SME case {name}");
            let bytes = assemble_with_features(source, TargetFeatures::apple_m5())
                .unwrap_or_else(|e| panic!("{name}: {source}: {e}"))
                .text;
            let word = u32::from_le_bytes(bytes.try_into().unwrap());
            let encoding = generated::encoding(name).unwrap();
            assert_eq!(word & encoding.mask, encoding.base, "{name}: {source}");
            assert!(assemble(source).is_err(), "SME leaked into default: {name}");
            assert!(
                assemble_with_features(source, TargetFeatures::default().with_sve()).is_err(),
                "SME leaked into SVE: {name}"
            );
        }
        assert_eq!(covered, expected);
        assert_eq!(covered.len(), 123);
    }

    #[test]
    fn every_m5_numeric_record_has_a_matching_gated_case() {
        use std::collections::BTreeSet;
        let expected: BTreeSet<_> = include_str!("../isa/m5-numeric.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
            .collect();
        let mut covered = BTreeSet::new();
        for line in include_str!("../tests/numeric-cases.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
        {
            let (name, source) = line.split_once('\t').unwrap();
            assert!(covered.insert(name), "duplicate numeric case {name}");
            let bytes = assemble_with_features(source, TargetFeatures::apple_m5())
                .unwrap_or_else(|e| panic!("{name}: {source}: {e}"))
                .text;
            assert_eq!(bytes.len(), 4, "{name}");
            let word = u32::from_le_bytes(bytes.try_into().unwrap());
            let encoding = generated::encoding(name).unwrap();
            assert_eq!(word & encoding.mask, encoding.base, "{name}: {source}");
            assert!(
                assemble(source).is_err(),
                "numeric feature leaked into default: {name}"
            );
        }
        assert_eq!(covered, expected);
        assert_eq!(covered.len(), 217);
    }

    #[test]
    fn every_baseline_neon_record_has_a_matching_assembled_case() {
        use std::collections::BTreeSet;
        let expected: BTreeSet<_> = include_str!("../isa/neon.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
            .collect();
        let mut covered = BTreeSet::new();
        for line in include_str!("../tests/neon-cases.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
        {
            let (name, source) = line.split_once('\t').unwrap();
            assert!(covered.insert(name), "duplicate NEON case: {name}");
            let bytes = assemble(source)
                .unwrap_or_else(|e| panic!("{name}: {source}: {e}"))
                .text;
            assert_eq!(bytes.len(), 4, "{name}");
            let word = u32::from_le_bytes(bytes.try_into().unwrap());
            let encoding = generated::encoding(name).unwrap();
            assert_eq!(
                word & encoding.mask,
                encoding.base,
                "wrong encoding form for {name}: {source}"
            );
        }
        assert_eq!(covered, expected);
        assert_eq!(covered.len(), 519);
    }

    #[test]
    fn imported_alias_conditions_preserve_masks_negation_and_feature_checks() {
        let features = TargetFeatures::default().with_sme();
        for (name, bit) in [("SMSTART", 1), ("SMSTOP", 0)] {
            for op1 in 0..8 {
                for op2 in 0..8 {
                    let fields = [
                        (FieldId::op1, op1),
                        (FieldId::op2, op2),
                        (FieldId::CRm, 6 | bit),
                    ];
                    // The first alias requirement is the parent's exclusion
                    // of MSR encodings belonging to other instruction classes.
                    let parent = &generated::encoding(name).unwrap().requirements[0];
                    assert_eq!(parent.satisfied(&fields, features), !(op1 == 0 && op2 <= 2));
                    assert_eq!(
                        encode_with_features(name, &fields, features).is_ok(),
                        op1 == 3 && op2 == 3
                    );
                    assert!(encode(name, &fields).is_err());
                }
            }
            for crm in 0..16 {
                let fields = [(FieldId::op1, 3), (FieldId::op2, 3), (FieldId::CRm, crm)];
                assert_eq!(
                    encode_with_features(name, &fields, features).is_ok(),
                    crm < 8 && crm & 1 == bit
                );
            }
        }
    }

    #[test]
    fn generated_lookup_returns_every_encoding_record() {
        for expected in generated::ENCODINGS {
            let actual = generated::encoding(expected.name).unwrap();
            assert!(std::ptr::eq(actual, expected), "{}", expected.name);
        }
        for unsupported in [
            "",
            "add",
            "ADD_64_addsub_imm ",
            "add_64_addsub_imm",
            "UNKNOWN",
        ] {
            assert!(generated::encoding(unsupported).is_none());
            assert_eq!(
                encode(unsupported, &[]),
                Err(format!("unsupported encoding `{unsupported}`"))
            );
        }
    }
}
