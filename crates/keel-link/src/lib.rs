//! Native executable linkers: arm64 Mach-O (`link`) and x86-64/AArch64 ELF (`elf::link`).
//! See docs/linking/macho.md and docs/linking/elf.md.
//! `link` returns an unsigned executable image. `write_signed` and `link_signed`
//! produce linker ad-hoc signatures in process on any host and install atomically.
pub mod archive;
pub mod elf;
pub mod elf_selection;
mod extract;
mod gc;
mod parallel;
mod read;
mod sign;
mod tbd;
mod unwind;
mod write;
use read::{Object, Symbol};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
pub use tbd::Dylib;

/// A link failure. The message names the input, section or symbol involved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn message(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self(message)
    }
}
impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self(message.into())
    }
}
impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.0
    }
}
impl std::ops::Deref for Error {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl PartialEq<str> for Error {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}
impl PartialEq<String> for Error {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}
impl PartialEq<&str> for Error {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn fail<T>(s: impl Into<String>) -> Result<T> {
    Err(Error(s.into()))
}
pub(crate) const BASE: u64 = 0x1_0000_0000;
pub(crate) const PAGE: usize = 0x4000;
pub(crate) fn align(n: usize, a: usize) -> usize {
    (n + a - 1) & !(a - 1)
}

/// Named input bytes. Accepted formats depend on the selected linker API.
#[derive(Debug)]
pub struct Input<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}
/// Explicit system interfaces. No implicit flat-namespace symbol lookup.
#[derive(Debug)]
pub struct Options {
    /// Remove what no root reaches. Default off. Sections of objects with
    /// MH_SUBSECTIONS_VIA_SYMBOLS are removed atom by atom, an atom starting at
    /// each defined symbol; other sections only whole.
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
#[derive(Debug)]
pub struct DebugObject {
    pub path: std::path::PathBuf,
    pub bytes: Vec<u8>,
}
#[derive(Debug)]
pub struct Executable {
    /// Exact object snapshots referenced by the native Darwin debug map.
    pub debug_objects: Vec<DebugObject>,
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}
/// Artifacts returned when the unsigned image is written directly to a sink.
#[derive(Debug)]
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
pub(crate) struct Linker<'a> {
    objects: Vec<Object<'a>>,
    discarded: BTreeSet<(usize, usize)>,
    demanded: Option<Vec<Vec<bool>>>,
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
    rebases: Vec<u64>,
    binds: BTreeMap<u64, (usize, i64)>,
    options: Options,
    warnings: Vec<String>,
    /// Each object symbol's target, indexed like `objects`, once `globals` is
    /// final. `None` marks an undefined name. Empty until then.
    resolved: Vec<Vec<Option<Target>>>,
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
    let (artifacts, _) = prepare_linker(inputs, options)?.emit_to(&mut image, |image, size| {
        image.get_mut().reserve(size);
    })?;
    Ok(Executable {
        bytes: image.into_inner(),
        debug_objects: artifacts.debug_objects,
        warnings: artifacts.warnings,
    })
}

/// An owned AArch64 Darwin unit prepared by keel-asm or decoded from its cache.
#[derive(Debug)]
pub struct ObjectInput {
    pub name: String,
    pub object: keel_object::Object,
}

/// Link owned units alongside ordinary object/archive bytes. Section payloads
/// move into the linker without Mach-O serialization, parsing or copying.
/// Owned units precede byte inputs in symbol-selection order.
///
/// # Errors
/// Returns a link error for invalid inputs or unresolved symbols. Native debug
/// maps require Mach-O snapshots and are unsupported for owned inputs.
pub fn link_objects(
    objects: Vec<ObjectInput>,
    inputs: &[Input<'_>],
    options: Options,
) -> Result<Executable> {
    if !objects.is_empty() && options.debug_directory.is_some() {
        return fail("owned link inputs do not support Darwin debug maps; use Mach-O inputs");
    }
    let objects = parallel::map(
        objects,
        |input| {
            input.object.symbols.len()
                + input
                    .object
                    .sections
                    .iter()
                    .map(|s| s.fixups.len())
                    .sum::<usize>()
        },
        32768,
        |input| Object::from_owned(input.name, input.object),
    )
    .into_iter()
    .collect::<Result<Vec<_>>>()?;
    let mut image = std::io::Cursor::new(Vec::new());
    let (artifacts, _) =
        prepare_with_objects(inputs, objects, options)?.emit_to(&mut image, |image, size| {
            image.get_mut().reserve(size);
        })?;
    Ok(Executable {
        bytes: image.into_inner(),
        debug_objects: artifacts.debug_objects,
        warnings: artifacts.warnings,
    })
}

/// Link flat cache entries without copying their input section payloads.
/// Cache units precede ordinary object/archive byte inputs. The cache bytes
/// remain borrowed until linking completes. Linker metadata is materialized;
/// payloads become owned only when relocation or unwind rewriting needs it.
///
/// # Errors
/// Returns a link error for invalid units, unresolved symbols or debug maps.
pub fn link_caches(
    caches: &[Input<'_>],
    inputs: &[Input<'_>],
    options: Options,
) -> Result<Executable> {
    if !caches.is_empty() && options.debug_directory.is_some() {
        return fail("cached link inputs do not support Darwin debug maps; use Mach-O inputs");
    }
    let objects = parallel::map(
        caches.iter().collect(),
        |input| input.bytes.len(),
        2 * 1024 * 1024,
        |input| {
            let cache = keel_object::Cache::parse(input.bytes)
                .map_err(|e| Error::from(format!("{}: {e}", input.name)))?;
            Object::from_cache(input.name.into(), cache)
        },
    )
    .into_iter()
    .collect::<Result<Vec<_>>>()?;
    let mut image = std::io::Cursor::new(Vec::new());
    let (artifacts, _) =
        prepare_with_objects(inputs, objects, options)?.emit_to(&mut image, |image, size| {
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
    link_to_hashed(inputs, options, output).map(|(artifacts, _)| artifacts)
}

/// [`link_to`], also returning the image's code-signature page hashes.
fn link_to_hashed<W: std::io::Write + std::io::Seek>(
    inputs: &[Input<'_>],
    options: Options,
    output: &mut W,
) -> Result<(LinkArtifacts, Vec<[u8; 32]>)> {
    if output
        .seek(std::io::SeekFrom::End(0))
        .map_err(|e| e.to_string())?
        != 0
    {
        return fail("link output must be empty");
    }
    prepare_linker(inputs, options)?.emit_to(output, |_, _| {})
}

fn prepare_linker(inputs: &[Input<'_>], options: Options) -> Result<Linker<'static>> {
    prepare_with_objects(inputs, Vec::new(), options)
}

fn prepare_with_objects<'a>(
    inputs: &[Input<'_>],
    mut objects: Vec<Object<'a>>,
    options: Options,
) -> Result<Linker<'a>> {
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
        rebases: Vec::new(),
        binds: BTreeMap::new(),
        options,
        warnings: Vec::new(),
        resolved: Vec::new(),
    };
    // Embedded LLVM bitcode (`__LLVM,__bitcode` and `__LLVM,__cmdline`, as
    // in Rust's prebuilt standard library) is input for LLVM's tools, never
    // part of the image. Apple's linker drops it too.
    for (oi, o) in l.objects.iter().enumerate() {
        for (si, s) in o.sections.iter().enumerate() {
            if s.segment == "__LLVM" {
                l.discarded.insert((oi, si));
            }
        }
    }
    l.prepare()?;
    Ok(l)
}
impl Linker<'_> {
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
                match self.globals.entry(s.name.clone()) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(Target::Definition(oi, si));
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        let weak = matches!(*entry.get(), Target::Definition(o, sym) if self.objects[o].symbols[sym].desc & 0x80 != 0);
                        if s.desc & 0x80 != 0 {
                            continue;
                        }
                        if !weak {
                            return fail(format!("{}: duplicate definition `{}`", o.name, s.name));
                        }
                        entry.insert(Target::Definition(oi, si));
                    }
                }
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
                let zero_fill = matches!(s.flags & 255, 1 | 0x12);
                let offset = align(out.size, 1 << s.alignment);
                let end = if s.atoms.is_empty() {
                    let end = offset.checked_add(s.size).ok_or("section size overflow")?;
                    if end <= 512 * 1024 * 1024 && !zero_fill {
                        out.data.resize(offset, 0);
                        out.data.extend_from_slice(&s.data);
                    }
                    end
                } else {
                    // Each live atom keeps its offset modulo the section
                    // alignment, so alignment inside an atom still holds, and
                    // atoms that follow each other need no padding.
                    let mask = (1 << s.alignment) - 1;
                    let mut cursor = out.size;
                    for a in s.atoms.iter_mut().filter(|a| a.live) {
                        a.offset = cursor + (a.start.wrapping_sub(cursor) & mask);
                        cursor = a
                            .offset
                            .checked_add(a.end - a.start)
                            .filter(|&end| end <= 512 * 1024 * 1024)
                            .ok_or("merged section exceeds 512 MiB")?;
                        if !zero_fill {
                            out.data.resize(a.offset, 0);
                            out.data.extend_from_slice(&s.data[a.start..a.end]);
                        }
                    }
                    cursor
                };
                if end > 512 * 1024 * 1024 {
                    return fail("merged section exceeds 512 MiB");
                }
                out.size = end;
                s.output = Some(id);
                s.offset = offset;
            }
        }
        if self.options.debug_directory.is_none()
            && self.objects.iter().any(|object| {
                object
                    .sections
                    .iter()
                    .any(|section| section.segment == "__DWARF")
            })
        {
            self.warnings.push(
                "DWARF input omitted; request --debug-map or --dsym to retain source debugging"
                    .into(),
            );
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
                    && self.demanded.as_ref().is_none_or(|d| d[oi][si])
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
        // `globals` is final. Relocations look symbols up by index from here on.
        self.resolved = (0..self.objects.len())
            .map(|oi| {
                (0..self.objects[oi].symbols.len())
                    .map(|si| self.lookup(oi, si))
                    .collect()
            })
            .collect();
        for (oi, o) in self.objects.iter().enumerate() {
            // GOT use and branch use are independent: a static branch may need
            // no entry, while a later page or personality reference still does.
            let mut got_seen = vec![false; o.symbols.len()];
            let mut branch_seen = vec![false; o.symbols.len()];
            for (si, s) in o.sections.iter().enumerate() {
                if self.discarded.contains(&(oi, si)) || s.segment == "__DWARF" {
                    continue;
                }
                for r in &s.relocs {
                    // Dead code's references need no GOT slot or stub, and
                    // their imports may not exist.
                    if r.external
                        && matches!(r.kind, 2 | 5 | 6 | 7 | 8 | 9)
                        && s.live_at(r.offset as usize)
                    {
                        let symbol = r.symbol as usize;
                        let sym = o
                            .symbols
                            .get(symbol)
                            .ok_or("relocation symbol index out of range")?;
                        if r.kind == 2 && branch_seen[symbol] || r.kind != 2 && got_seen[symbol] {
                            continue;
                        }
                        let target = self.symbol_target(oi, symbol)?;
                        let stub = r.kind == 2
                            && (matches!(target, Target::Import(_))
                                || self.coalesces(target, &sym.name));
                        if (r.kind != 2 || stub) && !got_seen[symbol] {
                            let key = self.got_key(oi, symbol)?;
                            let next = self.got.len();
                            self.got.entry(key.clone()).or_insert(next);
                            self.got_targets.insert(key, target);
                            got_seen[symbol] = true;
                        }
                        if r.kind == 2 {
                            branch_seen[symbol] = true;
                            if stub {
                                let next = self.stubs.len();
                                self.stubs.entry(sym.name.clone()).or_insert(next);
                            }
                        }
                    }
                }
                if s.name == "__compact_unwind" {
                    for r in &s.relocs {
                        if (r.offset as usize) % 32 == 16 && r.external {
                            let symbol = r.symbol as usize;
                            let seen = got_seen
                                .get_mut(symbol)
                                .ok_or("relocation symbol index out of range")?;
                            if !*seen {
                                let key = self.got_key(oi, symbol)?;
                                let next = self.got.len();
                                self.got.entry(key.clone()).or_insert(next);
                                self.got_targets
                                    .insert(key, self.symbol_target(oi, symbol)?);
                                *seen = true;
                            }
                        }
                    }
                }
            }
        }
        for (oi, o) in self.objects.iter().enumerate() {
            let mut seen = vec![false; o.symbols.len()];
            for u in &o.unwind {
                if let Some(si) = u.personality {
                    let done = seen
                        .get_mut(si)
                        .ok_or("invalid unwind personality symbol")?;
                    if std::mem::replace(done, true) {
                        continue;
                    }
                    let key = self.got_key(oi, si)?;
                    let next = self.got.len();
                    self.got.entry(key.clone()).or_insert(next);
                    self.got_targets.insert(key, self.symbol_target(oi, si)?);
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
            format!("\x01{oi}:{si}")
        })
    }
    fn symbol_target(&self, oi: usize, si: usize) -> Result<Target> {
        let s = self.objects[oi]
            .symbols
            .get(si)
            .ok_or("relocation symbol index out of range")?;
        let target = match self.resolved.get(oi) {
            Some(table) => table[si],
            None => self.lookup(oi, si),
        };
        target.ok_or_else(|| format!("undefined symbol `{}`", s.name).into())
    }
    /// The target `globals` gives symbol `si` of object `oi`; `None` if undefined.
    fn lookup(&self, oi: usize, si: usize) -> Option<Target> {
        let s = &self.objects[oi].symbols[si];
        if s.kind & 1 != 0 || s.kind & 0xe == 0 {
            self.globals.get(&s.name).copied()
        } else {
            Some(Target::Definition(oi, si))
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
        Ok(out.address + s.place((symbol.value - s.address) as usize)? as u64)
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
        let (artifacts, pages) = link_to_hashed(inputs, options, file)?;
        write_debug_objects(&artifacts.debug_objects)?;
        warnings = artifacts.warnings;
        Ok(Some(pages))
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
        file.write_all(&executable.bytes)
            .map_err(|e| e.to_string())?;
        Ok(None)
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
            file.write_all(&object.bytes).map_err(|e| e.to_string())?;
            Ok(None)
        })
        .map_err(|e| format!("debug object {}: {e}", object.path.display()))?;
    }
    Ok(())
}

/// Write `path` through a fresh temporary file. A signed output is an
/// executable, so it also gets mode 0755.
/// `write` may return the page hashes of the image it wrote, which signing
/// then reuses instead of reading the image back.
fn install_output(
    path: &Path,
    sign: bool,
    write: impl FnOnce(&mut std::fs::File) -> Result<Option<Vec<[u8; 32]>>>,
) -> Result<()> {
    // Never mmap the output: Darwin can cache a signature at mmap time. The
    // helper closes this fully signed inode before it replaces the
    // destination, so a running executable keeps its previous inode intact.
    keel_util::fs::replace(path, sign.then_some(0o755), |file| {
        let pages = write(file).map_err(std::io::Error::other)?;
        if sign {
            match pages {
                Some(pages) => sign::sign_hashed(file, Some(&pages)),
                None => sign::sign(file),
            }
            .map_err(std::io::Error::other)?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string().into())
}
