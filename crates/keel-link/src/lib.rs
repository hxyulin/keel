//! A bounded native linker for Keel's macOS arm64 objects. See docs/macos-linker.md.
//! `link` is host-independent and returns an unsigned executable image. On macOS,
//! `write_signed` installs it atomically after Apple's ad-hoc codesign succeeds.
mod read;
mod tbd;
mod unwind;
mod write;
use read::{Object, Symbol};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
pub use tbd::Dylib;

pub type Result<T> = std::result::Result<T, String>;
pub(crate) fn fail<T>(s: impl Into<String>) -> Result<T> {
    Err(s.into())
}
pub(crate) const BASE: u64 = 0x1_0000_0000;
pub(crate) const PAGE: usize = 0x4000;
pub(crate) fn align(n: usize, a: usize) -> usize {
    (n + a - 1) & !(a - 1)
}

/// Named input bytes, either a thin arm64 MH_OBJECT or BSD ar archive.
pub struct Input<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}
/// Explicit system interfaces. No implicit flat-namespace symbol lookup.
pub struct Options {
    pub dylibs: Vec<Dylib>,
    pub minimum_macos: u32,
    pub sdk_version: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            dylibs: Vec::new(),
            minimum_macos: 11 << 16,
            sdk_version: 0,
        }
    }
}
pub struct Executable {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}
#[derive(Clone)]
pub(crate) struct OutputSection {
    pub segment: String,
    pub name: String,
    pub flags: u32,
    pub alignment: u32,
    pub data: Vec<u8>,
    pub size: usize,
    pub address: u64,
    pub file: usize,
    pub ordinal: u8,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Target {
    Definition(usize, usize),
    Common(usize),
    Header,
    Import(usize),
}
pub(crate) struct Linker {
    objects: Vec<Object>,
    outputs: Vec<OutputSection>,
    globals: BTreeMap<String, Target>,
    imports: Vec<(String, usize)>,
    got: BTreeMap<String, usize>,
    stubs: BTreeMap<String, usize>,
    got_targets: BTreeMap<String, Target>,
    got_section: usize,
    stub_section: usize,
    unwind_section: usize,
    common_section: usize,
    rebases: BTreeSet<u64>,
    binds: BTreeMap<u64, (usize, i64)>,
    options: Options,
    warnings: Vec<String>,
}

pub fn link(inputs: &[Input<'_>], options: Options) -> Result<Executable> {
    if options.minimum_macos < 11 << 16 {
        return fail("arm64 executables require macOS 11 or newer");
    }
    if options.dylibs.is_empty() {
        return fail("explicit libSystem TBD interface is required");
    }
    let mut install_names = BTreeSet::new();
    for dylib in &options.dylibs {
        if !dylib.install_name.starts_with('/')
            || dylib.install_name.contains('\0')
            || !install_names.insert(&dylib.install_name)
        {
            return fail("dylib interfaces need unique absolute install names without NUL bytes");
        }
        if dylib
            .exports
            .iter()
            .any(|name| name.is_empty() || name.contains('\0'))
        {
            return fail("dylib interface contains an invalid export name");
        }
    }
    let mut objects = Vec::new();
    let mut archives = Vec::new();
    for input in inputs {
        if input.bytes.starts_with(b"!<arch>\n") {
            archives.extend(read::archive(input.name, input.bytes)?);
        } else {
            objects.push(Object::parse(input.name, input.bytes)?);
        }
    }
    // Fixed-point extraction supports archive dependency chains independent of
    // member order. Ordinary objects take precedence over tentative commons.
    loop {
        let defined: BTreeSet<_> = objects
            .iter()
            .flat_map(|o| &o.symbols)
            .filter(|s| s.kind & 1 != 0 && (s.kind & 0xe != 0 || s.value != 0))
            .map(|s| s.name.as_str())
            .collect();
        let needed: BTreeSet<_> = objects
            .iter()
            .flat_map(|o| &o.symbols)
            .filter(|s| s.kind & 0xe == 0 && s.value == 0 && !defined.contains(s.name.as_str()))
            .map(|s| s.name.clone())
            .chain((!defined.contains("_main")).then(|| "_main".into()))
            .collect();
        let found = archives.iter().position(|o| {
            o.symbols
                .iter()
                .any(|s| s.kind & 1 != 0 && s.kind & 0xe != 0 && needed.contains(&s.name))
        });
        let Some(index) = found else {
            break;
        };
        objects.push(archives.remove(index));
    }
    if objects.is_empty() {
        return fail("no input objects");
    }
    let mut l = Linker {
        objects,
        outputs: Vec::new(),
        globals: BTreeMap::new(),
        imports: Vec::new(),
        got: BTreeMap::new(),
        stubs: BTreeMap::new(),
        got_targets: BTreeMap::new(),
        got_section: 0,
        stub_section: 0,
        unwind_section: 0,
        common_section: 0,
        rebases: BTreeSet::new(),
        binds: BTreeMap::new(),
        options,
        warnings: Vec::new(),
    };
    l.prepare()?;
    l.emit()
}
impl Linker {
    fn output(&mut self, segment: &str, name: &str, flags: u32, alignment: u32) -> Result<usize> {
        if let Some(id) = self
            .outputs
            .iter()
            .position(|s| s.segment == segment && s.name == name)
        {
            let s = &mut self.outputs[id];
            if s.flags & 255 != flags & 255 {
                return fail(format!("incompatible section flags for {segment},{name}"));
            }
            s.flags |= flags;
            s.alignment = s.alignment.max(alignment);
            return Ok(id);
        }
        let id = self.outputs.len();
        self.outputs.push(OutputSection {
            segment: segment.into(),
            name: name.into(),
            flags,
            alignment,
            data: Vec::new(),
            size: 0,
            address: 0,
            file: 0,
            ordinal: 0,
        });
        Ok(id)
    }
    fn prepare(&mut self) -> Result<()> {
        self.globals
            .insert("__mh_execute_header".into(), Target::Header);
        let mut commons: BTreeMap<String, (u64, u32)> = BTreeMap::new();
        for (oi, o) in self.objects.iter().enumerate() {
            self.options.minimum_macos = self.options.minimum_macos.max(o.minimum);
            for (si, s) in o
                .symbols
                .iter()
                .enumerate()
                .filter(|(_, s)| s.kind & 1 != 0)
            {
                if s.kind & 0xe == 0 {
                    if s.value != 0 {
                        let c = commons.entry(s.name.clone()).or_default();
                        c.0 = c.0.max(s.value);
                        c.1 = c.1.max(u32::from((s.desc >> 8) & 15));
                    }
                    continue;
                }
                if self
                    .globals
                    .insert(s.name.clone(), Target::Definition(oi, si))
                    .is_some()
                {
                    return fail(format!("{}: duplicate definition `{}`", o.name, s.name));
                }
            }
        }
        for oi in 0..self.objects.len() {
            for si in 0..self.objects[oi].sections.len() {
                let s = self.objects[oi].sections[si].clone();
                if s.segment == "__DWARF" {
                    continue;
                }
                if s.segment == "__LD" && s.name == "__compact_unwind" {
                    continue;
                }
                if !matches!(s.segment.as_str(), "__TEXT" | "__DATA" | "__DATA_CONST") {
                    return fail(format!(
                        "{}: unsupported segment `{}`",
                        self.objects[oi].name, s.segment
                    ));
                }
                if matches!(s.name.as_str(), "__stubs" | "__got" | "__unwind_info") {
                    return fail("input uses a reserved linker section");
                }
                if s.flags & 0xff == 0xb && !(s.segment == "__TEXT" && s.name == "__eh_frame") {
                    return fail("coalesced sections require the system linker");
                }
                // Writable relocatable constants stay in __DATA until a future
                // chained-fixup/SG_READ_ONLY implementation.
                let seg = if s.segment == "__DATA_CONST" {
                    "__DATA"
                } else {
                    &s.segment
                }
                .to_owned();
                let id = self.output(
                    &seg,
                    &s.name.clone(),
                    s.flags & !(0x300 | 0x02000000),
                    s.alignment,
                )?;
                let s = &mut self.objects[oi].sections[si];
                let out = &mut self.outputs[id];
                let offset = align(out.size, 1 << s.alignment);
                let end = offset.checked_add(s.size).ok_or("section size overflow")?;
                if end > 512 * 1024 * 1024 {
                    return fail("merged section exceeds 512 MiB");
                }
                if s.flags & 255 != 1 {
                    out.data.resize(offset, 0);
                    out.data.extend_from_slice(&s.data);
                }
                out.size = end;
                s.output = Some(id);
                s.offset = offset;
            }
            if self.objects[oi]
                .sections
                .iter()
                .any(|s| s.segment == "__DWARF")
            {
                self.warnings.push(format!("{}: DWARF input omitted; native output does not yet support source debugging/dSYM",self.objects[oi].name));
            }
        }
        self.common_section = self.output("__DATA", "__common", 1, 3)?;
        for (name, (size, alignment)) in commons {
            if self.globals.contains_key(&name) {
                continue;
            }
            let s = &mut self.outputs[self.common_section];
            let off = align(s.size, 1 << alignment);
            s.size = off
                .checked_add(size.try_into().map_err(|_| "common too large")?)
                .ok_or("common overflow")?;
            if s.size > 512 * 1024 * 1024 {
                return fail("common section exceeds 512 MiB");
            }
            s.alignment = s.alignment.max(alignment);
            self.globals.insert(name, Target::Common(off));
        }
        let undefined: BTreeSet<_> = self
            .objects
            .iter()
            .flat_map(|o| &o.symbols)
            .filter(|s| s.kind & 0xe == 0 && s.value == 0)
            .map(|s| s.name.clone())
            .collect();
        for name in undefined {
            if self.globals.contains_key(&name) {
                continue;
            }
            let ordinal=self.options.dylibs.iter().position(|d|d.exports.contains(&name)).ok_or_else(||format!("undefined symbol `{name}`: absent from input definitions and selected macOS arm64 TBD interfaces"))?+1;
            let id = self.imports.len();
            self.imports.push((name.clone(), ordinal));
            self.globals.insert(name, Target::Import(id));
        }
        for (oi, o) in self.objects.iter().enumerate() {
            for s in &o.sections {
                if s.segment == "__DWARF" {
                    continue;
                }
                for r in &s.relocs {
                    if r.external && matches!(r.kind, 2 | 5 | 6 | 7) {
                        let sym = o
                            .symbols
                            .get(r.symbol)
                            .ok_or("relocation symbol index out of range")?;
                        if r.kind != 2
                            || matches!(self.symbol_target(oi, r.symbol)?, Target::Import(_))
                        {
                            let next = self.got.len();
                            self.got.entry(self.got_key(oi, r.symbol)?).or_insert(next);
                            self.got_targets.insert(
                                self.got_key(oi, r.symbol)?,
                                self.symbol_target(oi, r.symbol)?,
                            );
                        }
                        if r.kind == 2
                            && matches!(self.symbol_target(oi, r.symbol)?, Target::Import(_))
                        {
                            let next = self.stubs.len();
                            self.stubs.entry(sym.name.clone()).or_insert(next);
                        }
                    }
                }
                if s.name == "__compact_unwind" {
                    for r in &s.relocs {
                        if r.offset % 32 == 16 && r.external {
                            let next = self.got.len();
                            self.got.entry(self.got_key(oi, r.symbol)?).or_insert(next);
                            self.got_targets.insert(
                                self.got_key(oi, r.symbol)?,
                                self.symbol_target(oi, r.symbol)?,
                            );
                        }
                    }
                }
            }
        }
        self.got_section = self.output("__DATA", "__got", 6, 3)?;
        self.outputs[self.got_section].size = self.got.len() * 8;
        self.outputs[self.got_section].data = vec![0; self.got.len() * 8];
        self.stub_section = self.output("__TEXT", "__stubs", 0x80000408, 2)?;
        self.outputs[self.stub_section].size = self.stubs.len() * 12;
        self.outputs[self.stub_section].data = vec![0; self.stubs.len() * 12];
        self.unwind_section = self.output("__TEXT", "__unwind_info", 0, 2)?;
        let records: usize = self
            .objects
            .iter()
            .flat_map(|o| &o.sections)
            .filter(|s| s.name == "__compact_unwind")
            .map(|s| s.size / 32)
            .sum();
        let reserve = unwind::reserve(records);
        self.outputs[self.unwind_section].size = reserve;
        self.outputs[self.unwind_section].data = vec![0; reserve];
        Ok(())
    }
    fn got_key(&self, oi: usize, si: usize) -> Result<String> {
        let s = self.objects[oi]
            .symbols
            .get(si)
            .ok_or("invalid GOT symbol index")?;
        Ok(if s.kind & 1 != 0 || s.kind & 0xe == 0 {
            s.name.clone()
        } else {
            format!("\x01{oi}:{}", s.name)
        })
    }
    fn symbol_target(&self, oi: usize, si: usize) -> Result<Target> {
        let s = self.objects[oi]
            .symbols
            .get(si)
            .ok_or("relocation symbol index out of range")?;
        if s.kind & 1 != 0 || s.kind & 0xe == 0 {
            self.globals
                .get(&s.name)
                .copied()
                .ok_or_else(|| format!("undefined symbol `{}`", s.name))
        } else {
            Ok(Target::Definition(oi, si))
        }
    }
    fn symbol_address(&self, oi: usize, symbol: &Symbol) -> Result<u64> {
        if symbol.kind & 0xe == 2 {
            return Ok(symbol.value);
        }
        let s = self.objects[oi]
            .sections
            .get(symbol.section.wrapping_sub(1))
            .ok_or("invalid symbol section")?;
        let out = self
            .outputs
            .get(s.output.ok_or("reference to discarded metadata section")?)
            .ok_or("invalid output section")?;
        Ok(out.address + s.offset as u64 + symbol.value - s.address)
    }
    fn address(&self, target: Target) -> Result<u64> {
        match target {
            Target::Definition(oi, si) => self.symbol_address(oi, &self.objects[oi].symbols[si]),
            Target::Common(off) => Ok(self.outputs[self.common_section].address + off as u64),
            Target::Header => Ok(BASE),
            Target::Import(_) => fail("import requires a stub, GOT, or pointer binding"),
        }
    }
}

/// Sign to a sibling temporary file, then replace the destination only on success.
/// No Apple linker or compiler is invoked. `codesign` is the platform's signer.
pub fn write_signed(executable: &Executable, path: &Path) -> Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (executable, path);
        fail("ad-hoc signing requires macOS codesign")
    }
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = None;
        for n in 0..100 {
            let p = parent.join(format!(".keel-link-{}-{n}", std::process::id()));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
            {
                Ok(f) => {
                    temp = Some((p, f));
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return fail(e.to_string()),
            }
        }
        let (temp, mut file) = temp.ok_or("could not allocate output temporary file")?;
        let result = (|| {
            file.write_all(&executable.bytes)
                .map_err(|e| e.to_string())?;
            file.set_permissions(std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
            drop(file);
            let signed = std::process::Command::new("/usr/bin/codesign")
                .args([
                    "--force",
                    "--sign",
                    "-",
                    "--timestamp=none",
                    "--identifier",
                    "keel.native",
                ])
                .arg(&temp)
                .output()
                .map_err(|e| format!("codesign: {e}"))?;
            if !signed.status.success() {
                return fail(format!(
                    "ad-hoc signing failed: {}",
                    String::from_utf8_lossy(&signed.stderr)
                ));
            }
            std::fs::rename(&temp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }
}
