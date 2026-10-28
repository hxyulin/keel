//! Explicit placement names. Restricted ASCII prevents assembly injection.
use std::fmt;
/// A bounded name keeps declaration/callee attributes Copy without interning
/// or silently truncating object-format names. Target-specific limits follow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SectionName {
    bytes: [u8; 128],
    len: u8,
}
impl SectionName {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-$,".contains(&b))
        {
            return Err("section name must contain 1..128 safe ASCII characters");
        }
        let mut bytes = [0; 128];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Ok(Self {
            bytes,
            len: name.len() as u8,
        })
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap()
    }
}
impl fmt::Display for SectionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\"", self.as_str())
    }
}

/// Storage properties determine flags; C syntax cannot supply raw flags/types.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    Code,
    Data,
    ReadOnly,
    /// Allocated ELF note bytes supplied by the producer, without relocations.
    Note,
    ZeroFill,
    Relro,
    TlsData,
    TlsZeroFill,
}
impl Placement {
    pub fn for_data(init: &crate::DataInit) -> Self {
        if init.constant {
            if init.has_relocations() {
                Self::Relro
            } else {
                Self::ReadOnly
            }
        } else if init.is_zero() {
            Self::ZeroFill
        } else {
            Self::Data
        }
    }
    pub fn for_object(thread_local: bool, init: &crate::DataInit) -> Self {
        if thread_local {
            if init.is_zero() {
                Self::TlsZeroFill
            } else {
                Self::TlsData
            }
        } else {
            Self::for_data(init)
        }
    }
}
impl SectionName {
    /// Storage for explicit ELF names: data zeros occupy bytes and readonly
    /// sections can contain statically resolved pointer relocations.
    pub fn initialized_storage(self, placement: Placement) -> Placement {
        let name = self.as_str();
        if placement == Placement::ReadOnly && (name == ".note" || name.starts_with(".note.")) {
            Placement::Note
        } else if placement == Placement::Relro
            && (name == ".rodata1"
                || [
                    ".rodata",
                    ".init.rodata",
                    ".exit.rodata",
                    ".meminit.rodata",
                    ".ref.rodata",
                ]
                .iter()
                .any(|p| name == *p || name.strip_prefix(p).is_some_and(|s| s.starts_with('.'))))
        {
            Placement::ReadOnly
        } else if placement == Placement::ZeroFill
            && [
                ".data",
                ".data1",
                ".init.data",
                ".exit.data",
                ".meminit.data",
                ".ref.data",
            ]
            .iter()
            .any(|p| name == *p || name.strip_prefix(p).is_some_and(|s| s.starts_with('.')))
        {
            Placement::Data
        } else {
            placement
        }
    }

    pub fn validate(self, target: crate::Target, placement: Placement) -> Result<(), &'static str> {
        use Placement::*;
        let name = self.as_str();
        let placement = self.initialized_storage(placement);
        match target {
            crate::Target::Wasm32UnknownUnknown => {
                return Err(
                    "unsupported target wasm32-unknown-unknown: native sections are unavailable",
                );
            }
            crate::Target::X86_64LinuxGnu | crate::Target::Aarch64LinuxGnu => {
                if name.contains(".keel.retain.") {
                    return Err("compiler retain section namespace is reserved");
                }
                if name.contains(',') {
                    return Err("ELF section requires one name, without flags or segment syntax");
                }
                let prefix =
                    |p: &str| name == p || name.strip_prefix(p).is_some_and(|s| s.starts_with('.'));
                if matches!(name, ".note.GNU-stack" | ".note.gnu.property") {
                    return Err(
                        "compiler-owned stack/property note section placement is unsupported",
                    );
                }
                if prefix(".note") && placement != Note {
                    return Err("ELF notes require non-TLS readonly bytes without relocations");
                }
                if placement == Note && !prefix(".note") {
                    return Err("ELF note placement requires a .note or .note.* section");
                }
                if name.starts_with(".gnu_")
                    || name.starts_with(".debug_")
                    || name.starts_with(".zdebug_")
                    || [
                        ".preinit_array",
                        ".init_array",
                        ".fini_array",
                        ".ctors",
                        ".dtors",
                        ".debug",
                        ".zdebug",
                        ".eh_frame",
                        ".eh_frame_hdr",
                        ".gcc_except_table",
                        ".sframe",
                        ".llvm",
                        ".gnu",
                        ".symtab",
                        ".strtab",
                        ".shstrtab",
                        ".rela",
                        ".rel",
                        ".group",
                        ".dynamic",
                        ".hash",
                        ".relr",
                        ".symtab_shndx",
                        ".jcr",
                        ".dynsym",
                        ".dynstr",
                        ".interp",
                        ".got",
                        ".plt",
                    ]
                    .iter()
                    .any(|p| prefix(p))
                {
                    return Err("special/TLS/COMDAT section placement is unsupported");
                }
                if ((matches!(name, ".init" | ".fini")
                    || [".init.text", ".exit.text", ".meminit.text", ".ref.text"]
                        .iter()
                        .any(|p| prefix(p)))
                    && placement != Code)
                    || ([".init.data", ".exit.data", ".meminit.data", ".ref.data"]
                        .iter()
                        .any(|p| prefix(p))
                        && !matches!(placement, Data | ZeroFill | Relro))
                    || ([
                        ".init.rodata",
                        ".exit.rodata",
                        ".meminit.rodata",
                        ".ref.rodata",
                    ]
                    .iter()
                    .any(|p| prefix(p))
                        && !matches!(placement, ReadOnly | Relro))
                {
                    return Err("kernel lifetime section conflicts with storage class");
                }
                let tls = matches!(placement, TlsData | TlsZeroFill);
                if (prefix(".tdata") && placement != TlsData)
                    || (prefix(".tbss") && placement != TlsZeroFill)
                    || (tls
                        && [".text", ".data", ".rodata", ".bss", ".data1", ".rodata1"]
                            .iter()
                            .any(|p| prefix(p)))
                {
                    return Err("section name conflicts with TLS template storage class");
                }
                if (prefix(".text") && placement != Code)
                    || (prefix(".bss") && placement != ZeroFill)
                    || ((prefix(".rodata") || name == ".rodata1")
                        && !matches!(placement, ReadOnly | Relro))
                    || ((prefix(".data") || name == ".data1")
                        && !matches!(placement, Data | ZeroFill | Relro))
                {
                    return Err(
                        "section name conflicts with code/data/readonly/zero-fill placement",
                    );
                }
                if prefix(".data.rel.ro") && placement != Relro {
                    return Err(".data.rel.ro requires relocatable readonly data");
                }
            }
            crate::Target::Aarch64AppleDarwin => {
                if placement == Note {
                    return Err("ELF note placement is unavailable on Darwin");
                }
                let (segment, section) = name
                    .split_once(',')
                    .ok_or("Darwin section requires segment,section")?;
                if [segment, section].iter().any(|s| {
                    s.is_empty()
                        || s.len() > 16
                        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }) {
                    return Err(
                        "Darwin segment and section require 1..16 ASCII letters, digits or underscores",
                    );
                }
                if !matches!(segment, "__TEXT" | "__DATA") {
                    return Err("only __TEXT and __DATA custom segments are supported");
                }
                let tls = matches!(placement, TlsData | TlsZeroFill);
                if tls
                    && (segment != "__DATA" || ["__text", "__data", "__const"].contains(&section))
                {
                    return Err("Darwin TLS template requires a __DATA template section");
                }
                if ["__thread_vars", "__thread_ptrs", "__thread_init"].contains(&section) {
                    return Err(
                        "Darwin TLS descriptor/pointer/initializer sections are compiler-owned; section places template storage",
                    );
                }
                if (section == "__thread_data" && placement != TlsData)
                    || (section == "__thread_bss" && placement != TlsZeroFill)
                {
                    return Err(
                        "Darwin TLS template section name conflicts with initialized/zero-fill storage",
                    );
                }
                if (section.starts_with("__thread")
                    && !((section == "__thread_data" && placement == TlsData)
                        || (section == "__thread_bss" && placement == TlsZeroFill)))
                    || section.starts_with("__mod_")
                    || section.starts_with("__objc")
                    || [
                        "__StaticInit",
                        "__cfstring",
                        "__bss",
                        "__common",
                        "__cstring",
                        "__literal4",
                        "__literal8",
                        "__literal16",
                        "__eh_frame",
                        "__gcc_except_tab",
                        "__unwind_info",
                        "__got",
                        "__la_symbol_ptr",
                        "__nl_symbol_ptr",
                        "__stubs",
                        "__stub_helper",
                        "__interpose",
                        "__init_offsets",
                    ]
                    .contains(&section)
                {
                    return Err(
                        "special/TLS/coalesced/zero-fill Darwin section placement is unsupported",
                    );
                }
                if (placement == Code && segment != "__TEXT")
                    || (matches!(placement, Data | ZeroFill | Relro) && segment != "__DATA")
                    || (section == "__text" && placement != Code)
                    || (section == "__data" && !matches!(placement, Data | ZeroFill))
                    || (section == "__const" && !matches!(placement, ReadOnly | Relro))
                {
                    return Err(
                        "Darwin segment/section conflicts with code/data/readonly placement",
                    );
                }
            }
        }
        Ok(())
    }
}

/// Validates the full emitted module, shared by both native backend paths.
pub fn validate_module(module: &crate::Module, target: crate::Target) -> Result<(), String> {
    if !module.assembly.is_empty() && target != crate::Target::X86_64LinuxGnu {
        return Err("module assembly requires the native x86-64 Linux backend".into());
    }
    if target == crate::Target::Wasm32UnknownUnknown {
        return Err(
            "unsupported target wasm32-unknown-unknown: native module sections are unavailable"
                .into(),
        );
    }
    let mut seen = std::collections::BTreeMap::new();
    let mut check =
        |name: &str, section: Option<SectionName>, placement: Placement, retain: bool| {
            if let Some(section) = section {
                section
                    .validate(target, placement)
                    .map_err(|e| format!("@{name}: {e}"))?;
                // A named ELF input section has one type and flag set. Reject
                // incompatible requests rather than broadening retention silently.
                let key = section.as_str().to_owned();
                let properties = (
                    section.initialized_storage(placement),
                    matches!(
                        target,
                        crate::Target::X86_64LinuxGnu | crate::Target::Aarch64LinuxGnu
                    ) && retain,
                );
                if let Some(old) = seen.insert(key, properties)
                    && old != properties
                {
                    return Err(format!(
                        "@{name}: incompatible storage or retention in shared custom section"
                    ));
                }
            }
            Ok(())
        };
    for decl in &module.decls {
        if decl.attrs.section.is_some() || decl.attrs.retain {
            return Err(format!(
                "@{}: section/retain requires a definition",
                decl.name
            ));
        }
    }
    for f in &module.functions {
        if f.weak_odr && (f.attrs.section.is_some() || f.attrs.retain) {
            return Err(format!(
                "@{}: section/retain with COMDAT is unsupported",
                f.name
            ));
        }
        check(&f.name, f.attrs.section, Placement::Code, f.attrs.retain)?;
    }
    for d in &module.data {
        if (d.section.is_some() || d.retain) && (d.weak_odr || d.init.is_none()) {
            return Err(format!(
                "@{}: section/retain requires a non-COMDAT definition",
                d.name
            ));
        }
        if let Some(init) = &d.init {
            if (d.section.is_some() || d.retain)
                && (!init.align.is_power_of_two() || init.align > crate::MAX_DATA_ALIGN)
            {
                return Err(format!(
                    "@{}: custom/retained data alignment must be a power of two up to {}",
                    d.name,
                    crate::MAX_DATA_ALIGN
                ));
            }
            if d.section.is_some_and(|section| {
                section.initialized_storage(Placement::for_object(d.thread_local, init))
                    == Placement::Note
            }) && init.align < 4
            {
                return Err(format!(
                    "@{}: ELF notes require alignment of at least 4",
                    d.name
                ));
            }
            check(
                &d.name,
                d.section,
                Placement::for_object(d.thread_local, init),
                d.retain,
            )?;
        }
    }
    Ok(())
}
