//! Native executable linkers: arm64 Mach-O (`link`) and x86-64/AArch64 ELF (`elf::link`).
//! See docs/linking/macho.md and docs/linking/elf.md.
//! `link` returns an unsigned executable image. `write_signed` and `link_signed`
//! produce linker ad-hoc signatures in process on any host and install atomically.
pub mod archive;
pub mod elf;
pub mod elf_selection;
mod extract;
mod gc;
mod read;
mod sign;
mod tbd;
mod unwind;
mod write;
use bbtree::unsync::{BBTreeMap as BTreeMap, BBTreeSet as BTreeSet};
use read::{Object, Symbol};
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

/// Named input bytes. Accepted formats depend on the selected linker API.
pub struct Input<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}
/// Explicit system interfaces. No implicit flat-namespace symbol lookup.
pub struct Options {
    /// Collect unreachable input sections. Default off; symbols sharing an input
    /// section are indivisible, even with MH_SUBSECTIONS_VIA_SYMBOLS.
    pub dead_strip: bool,
    /// Selected-input definitions required by runtime DSO/dlsym consumers. These
    /// are roots, not an export filter, and do not force archive extraction.
    pub required_exports: Vec<String>,
    pub dylibs: Vec<Dylib>,
    pub minimum_macos: u32,
    pub sdk_version: u32,
    /// Absolute directory for retained copies of debug inputs. No input name is
    /// treated as a filesystem path. `write_signed` materializes these artifacts.
    /// Use a distinct directory for each executable; keep it for debug-map use,
    /// or explicitly run dsymutil before removing it.
    pub debug_directory: Option<std::path::PathBuf>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            dead_strip: false,
            required_exports: Vec::new(),
            dylibs: Vec::new(),
            minimum_macos: 11 << 16,
            sdk_version: 0,
            debug_directory: None,
        }
    }
}
pub struct DebugObject {
    pub path: std::path::PathBuf,
    pub bytes: Vec<u8>,
}
pub struct Executable {
    /// Exact object snapshots referenced by the native Darwin debug map.
    pub debug_objects: Vec<DebugObject>,
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}
/// Artifacts returned when the unsigned image is written directly to a sink.
pub struct LinkArtifacts {
    pub debug_objects: Vec<DebugObject>,
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
    SectionBoundary(usize, bool),
    Import(usize),
}
pub(crate) struct Linker {
    objects: Vec<Object>,
    discarded: BTreeSet<(usize, usize)>,
    demanded: Option<BTreeSet<(usize, usize)>>,
    outputs: Vec<OutputSection>,
    globals: BTreeMap<String, Target>,
    imports: Vec<(String, i32)>,
    weak_imports: BTreeSet<usize>,
    weak_names: BTreeSet<String>,
    weak_binds: BTreeMap<u64, (String, i64)>,
    got: BTreeMap<String, usize>,
    stubs: BTreeMap<String, usize>,
    got_targets: BTreeMap<String, Target>,
    got_section: usize,
    stub_section: usize,
    unwind_section: usize,
    common_section: usize,
    private_commons: BTreeSet<String>,
    rebases: BTreeSet<u64>,
    binds: BTreeMap<u64, (usize, i64)>,
    options: Options,
    warnings: Vec<String>,
}

// These names are assembler labels: no C ABI underscore is added.
fn section_boundary(name: &str) -> Result<Option<(&str, &str, bool)>> {
    let (rest, end) = if let Some(rest) = name.strip_prefix("section$start$") {
        (rest, false)
    } else if let Some(rest) = name.strip_prefix("section$end$") {
        (rest, true)
    } else {
        return Ok(None);
    };
    let (segment, section) = rest
        .split_once('$')
        .ok_or_else(|| format!("malformed section boundary `{name}`"))?;
    if segment.is_empty()
        || segment.len() > 16
        || section.is_empty()
        || section.len() > 16
        || section.contains('$')
    {
        return fail(format!("malformed section boundary `{name}`"));
    }
    Ok(Some((segment, section, end)))
}

pub fn link(inputs: &[Input<'_>], options: Options) -> Result<Executable> {
    let mut image = std::io::Cursor::new(Vec::new());
    let artifacts = prepare_linker(inputs, options)?.emit_to(&mut image, |image, size| {
        image.get_mut().reserve(size);
    })?;
    Ok(Executable {
        bytes: image.into_inner(),
        debug_objects: artifacts.debug_objects,
        warnings: artifacts.warnings,
    })
}

/// Write an unsigned Mach-O image to an empty, seekable output, leaving its
/// cursor at the end. On error, the sink may contain a partial image. Callers
/// installing an executable should use a temporary file or `link_signed`.
/// Input bytes must remain stable until this function returns.
pub fn link_to<W: std::io::Write + std::io::Seek>(
    inputs: &[Input<'_>],
    options: Options,
    output: &mut W,
) -> Result<LinkArtifacts> {
    if output
        .seek(std::io::SeekFrom::End(0))
        .map_err(|e| e.to_string())?
        != 0
    {
        return fail("link output must be empty");
    }
    prepare_linker(inputs, options)?.emit_to(output, |_, _| {})
}

fn prepare_linker(inputs: &[Input<'_>], options: Options) -> Result<Linker> {
    if let Some(directory) = &options.debug_directory
        && (!directory.is_absolute() || directory.to_str().is_none_or(|p| p.contains('\0')))
    {
        return fail("debug directory must be an absolute UTF-8 path without NUL bytes");
    }
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
    // The temporary set borrows options until it is dropped.
    drop(install_names);
    let mut objects = Vec::new();
    let mut archives = Vec::new();
    let mut archive_bodies = Vec::new();
    for input in inputs {
        if input.bytes.starts_with(b"!<arch>\n") {
            let (members, bodies) = read::archive(input.name, input.bytes)?;
            archives.extend(members);
            archive_bodies.extend(bodies);
        } else {
            objects.push(Object::parse(
                input.name,
                input.bytes,
                options.debug_directory.is_some(),
            )?);
        }
    }
    extract::archives(
        &mut objects,
        archives,
        archive_bodies,
        options.debug_directory.is_some(),
    )?;
    if objects.is_empty() {
        return fail("no input objects");
    }
    let mut l = Linker {
        objects,
        discarded: BTreeSet::new(),
        demanded: None,
        outputs: Vec::new(),
        globals: BTreeMap::new(),
        imports: Vec::new(),
        weak_imports: BTreeSet::new(),
        weak_names: BTreeSet::new(),
        weak_binds: BTreeMap::new(),
        got: BTreeMap::new(),
        stubs: BTreeMap::new(),
        got_targets: BTreeMap::new(),
        got_section: 0,
        stub_section: 0,
        unwind_section: 0,
        common_section: 0,
        private_commons: BTreeSet::new(),
        rebases: BTreeSet::new(),
        binds: BTreeMap::new(),
        options,
        warnings: Vec::new(),
    };
    l.prepare()?;
    Ok(l)
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
        let mut commons: BTreeMap<String, (u64, u32, bool)> = BTreeMap::new();
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
                        let c = commons.entry(s.name.clone()).or_insert((0, 0, true));
                        c.0 = c.0.max(s.value);
                        c.1 = c.1.max(u32::from((s.desc >> 8) & 15));
                        c.2 &= s.kind & 0x10 != 0;
                    }
                    continue;
                }
                if s.desc & 0x80 != 0 {
                    self.weak_names.insert(s.name.clone());
                }
                if let Some(&previous) = self.globals.get(&s.name) {
                    let weak = matches!(previous, Target::Definition(o, sym) if self.objects[o].symbols[sym].desc & 0x80 != 0);
                    if s.desc & 0x80 != 0 {
                        continue;
                    }
                    if !weak {
                        return fail(format!("{}: duplicate definition `{}`", o.name, s.name));
                    }
                }
                self.globals
                    .insert(s.name.clone(), Target::Definition(oi, si));
            }
        }
        if self.options.dead_strip {
            let live_commons = self.collect()?;
            commons.retain(|name, _| live_commons.contains(name));
        }
        for oi in 0..self.objects.len() {
            for si in 0..self.objects[oi].sections.len() {
                if self.discarded.contains(&(oi, si)) {
                    continue;
                }
                let s = &self.objects[oi].sections[si];
                if s.flags & 255 == 0x13 && !s.size.is_multiple_of(24) {
                    return fail(
                        "arm64 TLS descriptors must be 8-byte aligned triples of pointers",
                    );
                }
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
                // Only metadata crosses the mutable output lookup; cloning
                // Section here would also duplicate its payload and relocations.
                let name = s.name.clone();
                let flags = s.flags & !(0x300 | 0x02000000);
                let alignment = if s.flags & 255 == 0x13 {
                    s.alignment.max(3)
                } else {
                    s.alignment
                };
                let id = self.output(&seg, &name, flags, alignment)?;
                let s = &mut self.objects[oi].sections[si];
                let out = &mut self.outputs[id];
                let offset = align(out.size, 1 << s.alignment);
                let end = offset.checked_add(s.size).ok_or("section size overflow")?;
                if end > 512 * 1024 * 1024 {
                    return fail("merged section exceeds 512 MiB");
                }
                if !matches!(s.flags & 255, 1 | 0x12) {
                    out.data.resize(offset, 0);
                    out.data.extend_from_slice(&s.data);
                }
                out.size = end;
                s.output = Some(id);
                s.offset = offset;
            }
            if self.options.debug_directory.is_none()
                && self.objects[oi]
                    .sections
                    .iter()
                    .any(|s| s.segment == "__DWARF")
            {
                self.warnings.push(format!("{}: DWARF input omitted; request --debug-map or --dsym to retain source debugging",self.objects[oi].name));
            }
        }
        self.common_section = self.output("__DATA", "__common", 1, 3)?;
        for (name, (size, alignment, private)) in commons {
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
            if private {
                self.private_commons.insert(name.clone());
            }
            self.globals.insert(name, Target::Common(off));
        }
        let mut undefined = BTreeMap::new();
        for (oi, o) in self.objects.iter().enumerate() {
            for (si, s) in o.symbols.iter().enumerate() {
                if s.kind & 0xe == 0
                    && s.value == 0
                    && self.demanded.as_ref().is_none_or(|d| d.contains(&(oi, si)))
                {
                    let weak = undefined.entry(s.name.clone()).or_insert(true);
                    *weak &= s.desc & 0x40 != 0;
                }
            }
        }
        for (name, weak) in undefined {
            if self.globals.contains_key(&name) {
                continue;
            }
            // ld64 searches selected libraries before inventing boundary atoms.
            if !self
                .options
                .dylibs
                .iter()
                .any(|d| d.exports.contains(&name))
                && let Some((segment, section, end)) = section_boundary(&name)?
            {
                // Match the same mapping used for admitted input contributions.
                let segment = if segment == "__DATA_CONST" {
                    "__DATA"
                } else {
                    segment
                };
                if !matches!(segment, "__TEXT" | "__DATA") {
                    return fail(format!("unsupported boundary segment `{segment}`"));
                }
                if matches!(section, "__stubs" | "__got" | "__unwind_info" | "__common") {
                    return fail(format!(
                        "boundary for reserved linker section `{section}` is unsupported"
                    ));
                }
                let id = if let Some(id) = self
                    .outputs
                    .iter()
                    .position(|s| s.segment == segment && s.name == section)
                {
                    if !matches!(self.outputs[id].flags & 255, 0 | 1) {
                        return fail(format!(
                            "boundary for special section `{section}` is unsupported"
                        ));
                    }
                    id
                } else {
                    // Like ld64/lld, a missing section denotes an empty span.
                    self.output(segment, section, 0, 0)?
                };
                self.globals.insert(name, Target::SectionBoundary(id, end));
                continue;
            }
            let ordinal = match self
                .options
                .dylibs
                .iter()
                .enumerate()
                .filter(|(_, d)| d.exports.contains(&name))
                .min_by_key(|(_, d)| d.weak_exports.contains(&name))
                .map(|(i, _)| i)
            {
                Some(i) => (i + 1) as i32,
                None if weak => -2, // weak flat lookup: dyld writes zero if absent
                None => {
                    return fail(format!(
                        "undefined symbol `{name}`: absent from input definitions and selected macOS arm64 TBD interfaces"
                    ));
                }
            };
            let id = self.imports.len();
            if weak {
                self.weak_imports.insert(id);
            }
            if ordinal > 0
                && self.options.dylibs[ordinal as usize - 1]
                    .weak_exports
                    .contains(&name)
            {
                self.weak_names.insert(name.clone());
            }
            self.imports.push((name.clone(), ordinal));
            self.globals.insert(name, Target::Import(id));
        }
        for dylib in &self.options.dylibs {
            self.weak_names.extend(
                dylib
                    .weak_exports
                    .iter()
                    .filter(|n| matches!(self.globals.get(*n), Some(Target::Definition(_, _))))
                    .cloned(),
            );
        }
        // Private extern definitions coalesce statically, never across images.
        self.weak_names.retain(|name| !matches!(self.globals.get(name), Some(Target::Definition(o,s)) if self.objects[*o].symbols[*s].kind & 0x10 != 0));
        for (oi, o) in self.objects.iter().enumerate() {
            for (si, s) in o.sections.iter().enumerate() {
                if self.discarded.contains(&(oi, si)) || s.segment == "__DWARF" {
                    continue;
                }
                for r in &s.relocs {
                    if r.external && matches!(r.kind, 2 | 5 | 6 | 7 | 8 | 9) {
                        let sym = o
                            .symbols
                            .get(r.symbol)
                            .ok_or("relocation symbol index out of range")?;
                        if r.kind != 2
                            || matches!(self.symbol_target(oi, r.symbol)?, Target::Import(_))
                            || self.coalesces(self.symbol_target(oi, r.symbol)?, &sym.name)
                        {
                            let next = self.got.len();
                            self.got.entry(self.got_key(oi, r.symbol)?).or_insert(next);
                            self.got_targets.insert(
                                self.got_key(oi, r.symbol)?,
                                self.symbol_target(oi, r.symbol)?,
                            );
                        }
                        if r.kind == 2
                            && (matches!(self.symbol_target(oi, r.symbol)?, Target::Import(_))
                                || self.coalesces(self.symbol_target(oi, r.symbol)?, &sym.name))
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
        Ok(())
    }
    fn coalesces(&self, target: Target, name: &str) -> bool {
        match target {
            Target::Definition(o, s) => {
                self.objects[o].symbols[s].desc & 0x80 != 0
                    && self.objects[o].symbols[s].kind & 0x10 == 0
            }
            Target::Import(_) => self.weak_names.contains(name),
            _ => false,
        }
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
        Ok(out.address + s.offset as u64 + (symbol.value - s.address))
    }
    fn address(&self, target: Target) -> Result<u64> {
        match target {
            Target::Definition(oi, si) => self.symbol_address(oi, &self.objects[oi].symbols[si]),
            Target::Common(off) => Ok(self.outputs[self.common_section].address + off as u64),
            Target::Header => Ok(BASE),
            Target::SectionBoundary(id, end) => {
                let section = &self.outputs[id];
                Ok(section.address + if end { section.size as u64 } else { 0 })
            }
            Target::Import(_) => fail("import requires a stub, GOT, or pointer binding"),
        }
    }
}

/// Link directly to a sibling temporary file, sign, and atomically install it.
/// This avoids a complete executable byte buffer. Returns linker warnings.
pub fn link_signed(inputs: &[Input<'_>], options: Options, path: &Path) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    install_output(path, true, |file| {
        let artifacts = link_to(inputs, options, file)?;
        write_debug_objects(&artifacts.debug_objects)?;
        warnings = artifacts.warnings;
        Ok(())
    })?;
    Ok(warnings)
}

/// Sign to a sibling temporary file, then replace the destination only on success.
/// Uses a host-independent linker ad-hoc signature, without external tools.
/// The image must be an unsigned Keel executable as returned by `link`.
/// This supplies local integrity checking, not Developer ID or notarization.
pub fn write_signed(executable: &Executable, path: &Path) -> Result<()> {
    use std::io::Write;
    write_debug_objects(&executable.debug_objects)?;
    install_output(path, true, |file| {
        file.write_all(&executable.bytes).map_err(|e| e.to_string())
    })
}

fn write_debug_objects(objects: &[DebugObject]) -> Result<()> {
    for object in objects {
        std::fs::create_dir_all(
            object
                .path
                .parent()
                .ok_or("debug object has no directory")?,
        )
        .map_err(|e| e.to_string())?;
        // Snapshot paths may themselves be mapped inputs on a later link.
        // Atomic replacement keeps the old inode stable until maps are dropped.
        install_output(&object.path, false, |file| {
            use std::io::Write;
            file.write_all(&object.bytes).map_err(|e| e.to_string())
        })
        .map_err(|e| format!("debug object {}: {e}", object.path.display()))?;
    }
    Ok(())
}

fn install_output(
    path: &Path,
    sign: bool,
    write: impl FnOnce(&mut std::fs::File) -> Result<()>,
) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = None;
    for n in 0..100 {
        let p = parent.join(format!(".keel-link-{}-{n}", std::process::id()));
        match std::fs::OpenOptions::new()
            .read(true)
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
        write(&mut file)?;
        if sign {
            sign::sign(&mut file)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
        }
        // Never mmap the output: Darwin can cache a signature at mmap time.
        // Close this fresh, fully signed inode before atomically replacing the
        // destination; a running executable keeps its previous inode intact.
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
