use std::fmt::{self, Write};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
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
impl Expression {
    fn write_to<W: Write + ?Sized>(&self, f: &mut W) -> fmt::Result {
        if let Some(add) = &self.add {
            f.write_str(add)?;
            if let Some(sub) = &self.subtract {
                f.write_str("-")?;
                f.write_str(sub)?;
            }
            if self.constant != 0 {
                write_int(f, self.constant, true)?;
            }
            Ok(())
        } else {
            write_int(f, self.constant, false)
        }
    }
}
impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
/// Decimal digits of `n`, with a sign when negative or when `plus` is set
/// (as `{}` and `{:+}` print it), without the formatting machinery.
pub(crate) fn write_int<W: Write + ?Sized>(w: &mut W, n: i128, plus: bool) -> fmt::Result {
    let mut buf = [0u8; 40];
    let mut i = buf.len();
    let mut m = n.unsigned_abs();
    // Most values fit a word; avoid 128-bit division for them.
    if let Ok(mut m64) = u64::try_from(m) {
        loop {
            i -= 1;
            buf[i] = b'0' + (m64 % 10) as u8;
            m64 /= 10;
            if m64 == 0 {
                break;
            }
        }
    } else {
        while m != 0 {
            i -= 1;
            buf[i] = b'0' + (m % 10) as u8;
            m /= 10;
        }
    }
    if n < 0 {
        i -= 1;
        buf[i] = b'-';
    } else if plus {
        i -= 1;
        buf[i] = b'+';
    }
    // Only ASCII digits and signs were written.
    w.write_str(std::str::from_utf8(&buf[i..]).map_err(|_| fmt::Error)?)
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
fn cfi_register<W: Write + ?Sized>(f: &mut W, r: u8) -> fmt::Result {
    match r {
        0..=30 => {
            f.write_str("x")?;
            write_int(f, r.into(), false)
        }
        64..=95 => {
            f.write_str("d")?;
            write_int(f, (r - 64).into(), false)
        }
        _ => write_int(f, r.into(), false),
    }
}
impl Cfi {
    pub fn write_to<W: Write + ?Sized>(&self, f: &mut W) -> fmt::Result {
        match self {
            Self::DefCfaOffset(n) => {
                f.write_str(".cfi_def_cfa_offset ")?;
                write_int(f, (*n).into(), false)
            }
            Self::DefCfaRegister(r) => {
                f.write_str(".cfi_def_cfa_register ")?;
                write_int(f, (*r).into(), false)
            }
            Self::Start => f.write_str(".cfi_startproc"),
            Self::End => f.write_str(".cfi_endproc"),
            Self::DefCfa { register, offset } => {
                f.write_str(".cfi_def_cfa ")?;
                cfi_register(f, *register)?;
                f.write_str(", ")?;
                write_int(f, (*offset).into(), false)
            }
            Self::Offset { register, offset } => {
                f.write_str(".cfi_offset ")?;
                cfi_register(f, *register)?;
                f.write_str(", ")?;
                write_int(f, (*offset).into(), false)
            }
            Self::Personality(s) => {
                f.write_str(".cfi_personality 155, ")?;
                f.write_str(s)
            }
            Self::Lsda(s) => {
                f.write_str(".cfi_lsda 16, ")?;
                f.write_str(s)
            }
        }
    }
}
impl fmt::Display for Cfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Directive {
    /// ELF integer with an x86 relocation modifier (for example DTPOFF64).
    X86Integer {
        width: u8,
        address: crate::x86::Address,
    },
    /// Pointer-sized executable lifecycle array with SHT_INIT_ARRAY/SHT_FINI_ARRAY.
    ElfLifecycleArray {
        fini: bool,
    },
    /// Allocated ELF SHT_NOTE section, optionally retained by the linker.
    ElfNoteSection {
        name: String,
        retain: bool,
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
    /// ELF STV_HIDDEN: visible to the other objects of a link but not
    /// exported from the linked image.
    ElfHidden(String),
    Size {
        name: String,
        expression: Expression,
    },
    Section {
        segment: String,
        name: String,
        kind: SectionKind,
    },
    /// ELF STB_WEAK binding, defined or undefined.
    ElfWeak(String),
    /// Mach-O optional undefined symbol (N_WEAK_REF).
    WeakReference(String),
    Global(String),
    /// Mach-O coalescable definition; the symbol must also be exported.
    WeakDefinition(String),
    NoDeadStrip(String),
    /// Mach-O symbol visible to other objects but hidden from dynamic exports.
    PrivateExtern(String),
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
fn quoted<W: Write + ?Sized>(f: &mut W, s: &[u8]) -> fmt::Result {
    // Write runs of printable ASCII whole; they are always valid UTF-8.
    let run = |f: &mut W, run: &[u8]| match std::str::from_utf8(run) {
        Ok(s) => f.write_str(s),
        Err(_) => Err(fmt::Error),
    };
    f.write_str("\"")?;
    let mut start = 0;
    for (i, &b) in s.iter().enumerate() {
        if matches!(b, b' '..=b'~') && b != b'"' && b != b'\\' {
            continue;
        }
        run(f, &s[start..i])?;
        start = i + 1;
        if b == b'"' || b == b'\\' {
            f.write_char('\\')?;
            f.write_char(b as char)?;
        } else {
            let octal = [
                b'\\',
                b'0' + (b >> 6),
                b'0' + ((b >> 3) & 7),
                b'0' + (b & 7),
            ];
            f.write_str(std::str::from_utf8(&octal).map_err(|_| fmt::Error)?)?;
        }
    }
    run(f, &s[start..])?;
    f.write_str("\"")
}
impl fmt::Display for Directive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
impl Directive {
    /// The directive's spelling, written piece by piece: a `String` sink
    /// compiles to plain appends. The same text as its `Display`.
    pub fn write_to<W: Write + ?Sized>(&self, f: &mut W) -> fmt::Result {
        match self {
            Self::X86Integer { width, address } => {
                f.write_str(match width {
                    1 => ".byte ",
                    2 => ".short ",
                    4 => ".long ",
                    8 => ".quad ",
                    _ => return Err(fmt::Error),
                })?;
                address.write_to(f)
            }
            Self::ElfLifecycleArray { fini } => {
                let name = if *fini { "fini_array" } else { "init_array" };
                write!(f, ".section .{name},\"aw\",@{name}")
            }
            Self::ElfNoteSection { name, retain } => {
                f.write_str(".section\t")?;
                quoted(f, name.as_bytes())?;
                f.write_str(if *retain {
                    ",\"aR\",@note"
                } else {
                    ",\"a\",@note"
                })
            }
            Self::ElfSection {
                name,
                flags,
                nobits,
            } => {
                f.write_str(".section\t")?;
                quoted(f, name.as_bytes())?;
                f.write_str(",\"")?;
                for (bit, c) in [(1, 'w'), (2, 'a'), (4, 'x'), (0x400, 'T'), (0x200000, 'R')] {
                    if flags & bit != 0 {
                        f.write_char(c)?;
                    }
                }
                f.write_str(if *nobits {
                    "\",@nobits"
                } else {
                    "\",@progbits"
                })
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
                    (0x200000, 'R'),
                ] {
                    if flags & bit != 0 {
                        f.write_char(c)?;
                    }
                }
                f.write_str(if *nobits {
                    "\",@nobits"
                } else {
                    "\",@progbits"
                })?;
                if flags & 0x10 != 0 {
                    f.write_str(",")?;
                    write_int(f, (*entry_size).into(), false)?;
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
                f.write_str(if *function { ",@function" } else { ",@object" })
            }
            Self::ElfHidden(name) => {
                f.write_str(".hidden\t")?;
                quoted(f, name.as_bytes())
            }
            Self::Size { name, expression } => {
                f.write_str(".size\t")?;
                quoted(f, name.as_bytes())?;
                f.write_str(", ")?;
                expression.write_to(f)
            }

            Self::Section {
                segment,
                name,
                kind,
            } => {
                write!(f, ".section\t{segment},{name}")?;
                match kind {
                    SectionKind::Debug => f.write_str(",regular,debug"),
                    SectionKind::Code => f.write_str(",regular,pure_instructions"),
                    SectionKind::ModuleInit => f.write_str(",mod_init_funcs"),
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
            Self::ElfWeak(s) => write!(f, ".weak {s}"),
            Self::WeakReference(s) => write!(f, ".weak_reference {s}"),
            Self::Global(s) => write!(f, ".globl\t{s}"),
            Self::PrivateExtern(s) => write!(f, ".private_extern\t{s}"),
            Self::WeakDefinition(name) => write!(f, ".weak_definition {name}"),
            Self::NoDeadStrip(name) => write!(f, ".no_dead_strip {name}"),
            Self::SubsectionsViaSymbols => f.write_str(".subsections_via_symbols"),
            Self::Align(n) => {
                f.write_str(".p2align\t")?;
                write_int(f, (*n).into(), false)
            }
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
                    write_int(f, (*b).into(), false)?;
                }
                Ok(())
            }
            Self::Integer {
                width,
                expression,
                got,
            } => {
                f.write_str(match width {
                    1 => ".byte\t",
                    2 => ".short\t",
                    4 => ".long\t",
                    8 => ".quad\t",
                    _ => return Err(fmt::Error),
                })?;
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
                    expression.write_to(f)
                }
            }
            Self::Leb { signed, expression } => write!(
                f,
                "{}\t{expression}",
                if *signed { ".sleb128" } else { ".uleb128" }
            ),
            Self::Space(n) => {
                f.write_str(".space\t")?;
                write_int(f, (*n).into(), false)
            }
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
            Self::Cfi(c) => c.write_to(f),
        }
    }
}

/// Mach-O data-section types used by Keel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Data,
    /// A regular executable section with pure instructions.
    Code,
    /// Pointers to functions invoked at image initialization.
    ModuleInit,
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
            ("__TEXT", "__StaticInit") => Self::Code,
            ("__DATA", "__mod_init_func") => Self::ModuleInit,
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
                | "__debug_str" | "__debug_loc" | "__debug_aranges" | "__debug_line_str"
                | "__debug_str_offs" | "__debug_addr" | "__debug_rnglists" | "__debug_loclists"
                | "__debug_names" | "__apple_names" | "__apple_objc" | "__apple_namespac"
                | "__apple_types",
            ) => Self::Debug,
            ("__TEXT", "__eh_frame") => Self::EhFrame,
            _ => return None,
        })
    }
    pub fn flags(self) -> u32 {
        match self {
            Self::Data => 0,
            Self::Code => 0x8000_0000,
            Self::ModuleInit => 9,
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
            Self::NoDeadStrip(n)
            | Self::WeakReference(n)
            | Self::ElfWeak(n)
            | Self::Global(n)
            | Self::ElfHidden(n)
            | Self::PrivateExtern(n)
            | Self::WeakDefinition(n)
            | Self::ZeroFill { symbol: n, .. } => n.contains(s),
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
                if SectionKind::for_section(segment, name).is_some_and(|expected| expected != *kind)
                {
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
            Self::NoDeadStrip(s)
            | Self::ElfWeak(s)
            | Self::Global(s)
            | Self::ZeroFill { symbol: s, .. } => symbol(s)?,
            Self::WeakReference(s) => {
                symbol(s)?;
                if s.starts_with('L') {
                    return Err("weak reference requires a non-local symbol".into());
                }
            }
            Self::WeakDefinition(s) => {
                symbol(s)?;
                if s.starts_with('L') {
                    return Err("weak definition requires a non-local symbol".into());
                }
            }
            Self::PrivateExtern(s) => {
                symbol(s)?;
                if s.starts_with('L') {
                    return Err("private extern requires a non-local symbol".into());
                }
            }
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
            Self::Cfi(Cfi::DefCfa { register, .. } | Cfi::Offset { register, .. })
                if *register > 95 =>
            {
                return Err("invalid CFI register".into());
            }
            _ => {}
        }
        if let Self::Align(n) | Self::ZeroFill { alignment: n, .. } = self
            && *n > 16
        {
            return Err("alignment exceeds 16".into());
        }
        Ok(())
    }
}
