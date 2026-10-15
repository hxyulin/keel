use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expression {
    pub add: Option<String>,
    pub subtract: Option<String>,
    pub constant: i128,
}
impl Expression {
    pub fn constant(value: i128) -> Self {
        Self {
            constant: value,
            ..Self::default()
        }
    }
    pub fn symbol(name: impl Into<String>) -> Self {
        Self {
            add: Some(name.into()),
            ..Self::default()
        }
    }
    pub fn difference(add: impl Into<String>, subtract: impl Into<String>) -> Self {
        Self {
            add: Some(add.into()),
            subtract: Some(subtract.into()),
            constant: 0,
        }
    }
    pub fn with_addend(mut self, value: i128) -> Self {
        self.constant = value;
        self
    }
}
impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(add) = &self.add {
            f.write_str(add)?;
            if let Some(sub) = &self.subtract {
                write!(f, "-{sub}")?;
            }
            if self.constant != 0 {
                write!(f, "{:+}", self.constant)?;
            }
            Ok(())
        } else {
            write!(f, "{}", self.constant)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cfi {
    DefCfaOffset(u64),
    DefCfaRegister(u8),
    Start,
    End,
    DefCfa { register: u8, offset: u64 },
    Offset { register: u8, offset: i64 },
    Personality(String),
    Lsda(String),
}
struct CfiRegister(u8);
impl fmt::Display for CfiRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0..=30 => write!(f, "x{}", self.0),
            64..=95 => write!(f, "d{}", self.0 - 64),
            _ => write!(f, "{}", self.0),
        }
    }
}
impl fmt::Display for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DefCfaOffset(n) => write!(f, ".cfi_def_cfa_offset {n}"),
            Self::DefCfaRegister(r) => write!(f, ".cfi_def_cfa_register {r}"),
            Self::Start => f.write_str(".cfi_startproc"),
            Self::End => f.write_str(".cfi_endproc"),
            Self::DefCfa { register, offset } => {
                write!(f, ".cfi_def_cfa {}, {offset}", CfiRegister(*register))
            }
            Self::Offset { register, offset } => {
                write!(f, ".cfi_offset {}, {offset}", CfiRegister(*register))
            }
            Self::Personality(s) => write!(f, ".cfi_personality 155, {s}"),
            Self::Lsda(s) => write!(f, ".cfi_lsda 16, {s}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Directive {
    /// ELF integer with an x86 relocation modifier (for example DTPOFF64).
    X86Integer {
        width: u8,
        address: crate::x86::Address,
    },
    /// ELF section name, SHF_* flags, and SHT_NOBITS selection.
    ElfSection {
        name: String,
        flags: u64,
        nobits: bool,
    },
    /// ELF merge/string entry size and optional COMDAT signature. The original
    /// `ElfSection` remains available for sections without these attributes.
    ElfSectionMetadata {
        name: String,
        flags: u64,
        nobits: bool,
        entry_size: u64,
        group: Option<String>,
    },
    /// ELF STT_FUNC when true, otherwise STT_OBJECT.
    ElfType {
        name: String,
        function: bool,
    },
    Size {
        name: String,
        expression: Expression,
    },
    Section {
        segment: String,
        name: String,
        kind: SectionKind,
    },
    Global(String),
    SubsectionsViaSymbols,
    Align(u8),
    Bytes(Vec<u8>),
    CString(Vec<u8>),
    Integer {
        width: u8,
        expression: Expression,
        got: bool,
    },
    Leb {
        signed: bool,
        expression: Expression,
    },
    Space(u64),
    ZeroFill {
        symbol: String,
        size: u64,
        alignment: u8,
        thread_local: bool,
    },
    Set {
        name: String,
        expression: Expression,
    },
    File {
        index: u32,
        path: Vec<u8>,
    },
    Loc {
        file: u32,
        line: u32,
        column: u32,
        prologue_end: bool,
    },
    Cfi(Cfi),
}
fn quoted(f: &mut fmt::Formatter<'_>, s: &[u8]) -> fmt::Result {
    f.write_str("\"")?;
    for &b in s {
        match b {
            b'"' | b'\\' => write!(f, "\\{}", b as char)?,
            b' '..=b'~' => write!(f, "{}", b as char)?,
            _ => write!(f, "\\{b:03o}")?,
        }
    }
    f.write_str("\"")
}
impl fmt::Display for Directive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X86Integer { width, address } => write!(
                f,
                "{} {address}",
                match width {
                    1 => ".byte",
                    2 => ".short",
                    4 => ".long",
                    8 => ".quad",
                    _ => return Err(fmt::Error),
                }
            ),
            Self::ElfSection {
                name,
                flags,
                nobits,
            } => {
                f.write_str(".section\t")?;
                quoted(f, name.as_bytes())?;
                f.write_str(",\"")?;
                for (bit, c) in [(1, 'w'), (2, 'a'), (4, 'x'), (0x400, 'T')] {
                    if flags & bit != 0 {
                        write!(f, "{c}")?;
                    }
                }
                write!(f, "\",@{}", if *nobits { "nobits" } else { "progbits" })
            }
            Self::ElfSectionMetadata {
                name,
                flags,
                nobits,
                entry_size,
                group,
            } => {
                f.write_str(".section\t")?;
                quoted(f, name.as_bytes())?;
                f.write_str(",\"")?;
                for (bit, c) in [
                    (1, 'w'),
                    (2, 'a'),
                    (4, 'x'),
                    (0x10, 'M'),
                    (0x20, 'S'),
                    (0x200, 'G'),
                    (0x400, 'T'),
                ] {
                    if flags & bit != 0 {
                        write!(f, "{c}")?;
                    }
                }
                write!(f, "\",@{}", if *nobits { "nobits" } else { "progbits" })?;
                if flags & 0x10 != 0 {
                    write!(f, ",{entry_size}")?;
                }
                if let Some(signature) = group {
                    f.write_str(",")?;
                    quoted(f, signature.as_bytes())?;
                    f.write_str(",comdat")?;
                }
                Ok(())
            }
            Self::ElfType { name, function } => {
                f.write_str(".type\t")?;
                quoted(f, name.as_bytes())?;
                write!(f, ",@{}", if *function { "function" } else { "object" })
            }
            Self::Size { name, expression } => {
                f.write_str(".size\t")?;
                quoted(f, name.as_bytes())?;
                write!(f, ", {expression}")
            }

            Self::Section {
                segment,
                name,
                kind,
            } => {
                write!(f, ".section\t{segment},{name}")?;
                match kind {
                    SectionKind::Debug => f.write_str(",regular,debug"),
                    SectionKind::ThreadData => f.write_str(",thread_local_regular"),
                    SectionKind::ThreadVariables => f.write_str(",thread_local_variables"),
                    SectionKind::CString => f.write_str(",cstring_literals"),
                    SectionKind::Literal4 => f.write_str(",4byte_literals"),
                    SectionKind::Literal8 => f.write_str(",8byte_literals"),
                    SectionKind::Literal16 => f.write_str(",16byte_literals"),
                    SectionKind::EhFrame => f.write_str(",coalesced"),
                    SectionKind::ZeroFill => f.write_str(",zerofill"),
                    SectionKind::ThreadZeroFill => f.write_str(",thread_local_zerofill"),
                    SectionKind::Data => Ok(()),
                }
            }
            Self::Global(s) => write!(f, ".globl\t{s}"),
            Self::SubsectionsViaSymbols => f.write_str(".subsections_via_symbols"),
            Self::Align(n) => write!(f, ".p2align\t{n}"),
            Self::CString(bytes) => {
                f.write_str(".asciz\t")?;
                quoted(f, bytes)
            }
            Self::Bytes(bytes) => {
                if bytes.is_empty() {
                    return Ok(());
                }
                f.write_str(".byte\t")?;
                for (i, b) in bytes.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{b}")?;
                }
                Ok(())
            }
            Self::Integer {
                width,
                expression,
                got,
            } => {
                write!(
                    f,
                    "{}\t",
                    match width {
                        1 => ".byte",
                        2 => ".short",
                        4 => ".long",
                        8 => ".quad",
                        _ => return Err(fmt::Error),
                    }
                )?;
                if *got {
                    let add = expression.add.as_ref().ok_or(fmt::Error)?;
                    write!(f, "{add}@GOT")?;
                    if let Some(sub) = &expression.subtract {
                        write!(f, "-{sub}")?;
                    }
                    if expression.constant != 0 {
                        write!(f, "{:+}", expression.constant)?;
                    }
                    Ok(())
                } else {
                    expression.fmt(f)
                }
            }
            Self::Leb { signed, expression } => write!(
                f,
                "{}\t{expression}",
                if *signed { ".sleb128" } else { ".uleb128" }
            ),
            Self::Space(n) => write!(f, ".space\t{n}"),
            Self::ZeroFill {
                symbol,
                size,
                alignment,
                thread_local,
            } => {
                if *thread_local {
                    write!(f, ".tbss\t{symbol}, {size}, {alignment}")
                } else {
                    write!(f, ".zerofill\t__DATA,__bss,{symbol},{size},{alignment}")
                }
            }
            Self::Set { name, expression } => write!(f, "{name} = {expression}"),
            Self::File { index, path } => {
                write!(f, ".file\t{index} ")?;
                quoted(f, path)
            }
            Self::Loc {
                file,
                line,
                column,
                prologue_end,
            } => write!(
                f,
                ".loc\t{file} {line} {column}{}",
                if *prologue_end { " prologue_end" } else { "" }
            ),
            Self::Cfi(c) => c.fmt(f),
        }
    }
}

/// Mach-O data-section types used by Keel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Data,
    CString,
    Literal4,
    Literal8,
    Literal16,
    /// DWARF metadata, excluded from the linked image.
    Debug,
    /// Runtime DWARF call-frame records.
    EhFrame,
    ZeroFill,
    ThreadData,
    ThreadZeroFill,
    ThreadVariables,
}
impl SectionKind {
    pub fn is_zero_fill(self) -> bool {
        matches!(self, Self::ZeroFill | Self::ThreadZeroFill)
    }
    pub fn for_section(segment: &str, name: &str) -> Option<Self> {
        Some(match (segment, name) {
            ("__TEXT", "__text" | "__const" | "__gcc_except_tab")
            | ("__DATA", "__data" | "__const") => Self::Data,
            ("__TEXT", "__cstring") => Self::CString,
            ("__TEXT", "__literal4") => Self::Literal4,
            ("__TEXT", "__literal8") => Self::Literal8,
            ("__TEXT", "__literal16") => Self::Literal16,
            ("__DATA", "__bss") => Self::ZeroFill,
            ("__DATA", "__thread_data") => Self::ThreadData,
            ("__DATA", "__thread_bss") => Self::ThreadZeroFill,
            ("__DATA", "__thread_vars") => Self::ThreadVariables,
            (
                "__DWARF",
                "__debug_info" | "__debug_abbrev" | "__debug_line" | "__debug_ranges"
                | "__debug_str" | "__debug_loc",
            ) => Self::Debug,
            ("__TEXT", "__eh_frame") => Self::EhFrame,
            _ => return None,
        })
    }
    pub fn flags(self) -> u32 {
        match self {
            Self::Data => 0,
            Self::CString => 2,
            Self::Literal4 => 3,
            Self::Literal8 => 4,
            Self::Literal16 => 0xe,
            Self::Debug => 0x0200_0000,
            Self::EhFrame => 0x6800_000b,
            Self::ZeroFill => 1,
            Self::ThreadData => 0x11,
            Self::ThreadZeroFill => 0x12,
            Self::ThreadVariables => 0x13,
        }
    }
}

fn symbol(s: &str) -> Result<(), String> {
    let mut bytes = s.bytes();
    if !bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b"_.$".contains(&b))
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b"_.$".contains(&b))
    {
        return Err(format!("invalid symbol `{s}`"));
    }
    Ok(())
}
impl Expression {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(s) = &self.add {
            symbol(s)?;
        }
        if let Some(s) = &self.subtract {
            symbol(s)?;
            if self.add.is_none() {
                return Err("subtraction requires an add symbol".into());
            }
        }
        Ok(())
    }
    fn contains(&self, s: &str) -> bool {
        self.add.as_ref().is_some_and(|v| v.contains(s))
            || self.subtract.as_ref().is_some_and(|v| v.contains(s))
    }
}
impl Directive {
    pub fn contains(&self, s: &str) -> bool {
        match self {
            Self::Global(n) | Self::ZeroFill { symbol: n, .. } => n.contains(s),
            Self::CString(bytes) | Self::File { path: bytes, .. } => {
                s.is_empty() || bytes.windows(s.len()).any(|w| w == s.as_bytes())
            }
            Self::Integer { expression, .. } | Self::Leb { expression, .. } => {
                expression.contains(s)
            }
            Self::Set { name, expression } => name.contains(s) || expression.contains(s),
            Self::Cfi(Cfi::Personality(n) | Cfi::Lsda(n)) => n.contains(s),
            _ => false,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Section {
                segment,
                name,
                kind,
            } => {
                if SectionKind::for_section(segment, name) != Some(*kind) {
                    return Err("unsupported Mach-O section or type".into());
                }
                for s in [segment, name] {
                    if s.len() > 16
                        || s.is_empty()
                        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    {
                        return Err("invalid Mach-O section name".into());
                    }
                }
            }
            Self::Global(s) | Self::ZeroFill { symbol: s, .. } => symbol(s)?,
            Self::Integer {
                width, expression, ..
            } => {
                if ![1, 2, 4, 8].contains(width) {
                    return Err("invalid data width".into());
                }
                expression.validate()?;
            }
            Self::Leb { expression, .. } => expression.validate()?,
            Self::Set { name, expression } => {
                symbol(name)?;
                expression.validate()?;
            }
            Self::File { index, path } => {
                if *index == 0 || path.contains(&0) {
                    return Err("invalid file number or NUL path".into());
                }
            }
            Self::Cfi(Cfi::Personality(s) | Cfi::Lsda(s)) => symbol(s)?,
            Self::Cfi(Cfi::DefCfa { register, .. } | Cfi::Offset { register, .. }) => {
                if *register > 95 {
                    return Err("invalid CFI register".into());
                }
            }
            _ => {}
        }
        if let Self::Align(n) | Self::ZeroFill { alignment: n, .. } = self {
            if *n > 16 {
                return Err("alignment exceeds 16".into());
            }
        }
        Ok(())
    }
}
