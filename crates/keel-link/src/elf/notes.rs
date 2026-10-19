//! Bounded Linux/x86 GNU notes. Input arrays may be unsorted and repeated;
//! output is one sorted, unique property note (see docs/elf-linker.md).
use super::{Section, put32};
use crate::Result;
use std::collections::BTreeMap;

const FEATURE_1_AND: u32 = 0xc0000002;
const ISA_1_NEEDED: u32 = 0xc0008002;
const FEATURE_2_USED: u32 = 0xc0010001;
const ISA_1_USED: u32 = 0xc0010002;
pub(super) type Properties = BTreeMap<u32, u32>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Property,
    Abi,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate(
    name: &str,
    kind: u32,
    flags: u64,
    alignment: u64,
    entsize: u64,
    link: u32,
    info: u32,
) -> Result<Option<Kind>> {
    let note = match name {
        ".note.gnu.property" => Kind::Property,
        ".note.ABI-tag" => Kind::Abi,
        _ => {
            if flags & 2 != 0 && (kind == object::elf::SHT_NOTE || name.starts_with(".note.")) {
                return Err(format!("unsupported allocated note {name}"));
            }
            return Ok(None);
        }
    };
    let expected_alignment = if note == Kind::Property { 8 } else { 4 };
    if kind != object::elf::SHT_NOTE
        || flags != 2
        || alignment != expected_alignment
        || entsize != 0
        || link != 0
        || info != 0
    {
        return Err(format!(
            "{name} requires SHT_NOTE, SHF_ALLOC only, alignment {expected_alignment}, and zero entsize/link/info"
        ));
    }
    Ok(Some(note))
}

fn word(data: &[u8], at: usize) -> Result<u32> {
    let end = at.checked_add(4).ok_or("note offset overflow")?;
    let bytes = data
        .get(at..end)
        .ok_or("truncated note header or payload")?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub(super) fn properties(data: &[u8], out: &mut Properties) -> Result<()> {
    if data.is_empty() {
        return Err("empty .note.gnu.property".into());
    }
    let mut at: usize = 0;
    while at < data.len() {
        let namesz = word(data, at)?;
        let descsz = word(data, at.checked_add(4).ok_or("note offset overflow")?)? as usize;
        let kind = word(data, at.checked_add(8).ok_or("note offset overflow")?)?;
        let desc = at.checked_add(16).ok_or("note offset overflow")?;
        if namesz != 4 || kind != 5 || data.get(at + 12..desc) != Some(b"GNU\0") {
            return Err(".note.gnu.property requires owner GNU, namesz=4 and type=5".into());
        }
        if descsz == 0 || !descsz.is_multiple_of(8) {
            return Err("GNU property descriptor must be nonempty and 8-byte aligned".into());
        }
        let end = desc.checked_add(descsz).ok_or("note descriptor overflow")?;
        let descriptor = data
            .get(desc..end)
            .ok_or("truncated GNU property descriptor or padding")?;
        let mut p: usize = 0;
        while p < descriptor.len() {
            let id = word(descriptor, p)?;
            let size = word(descriptor, p + 4)?;
            let mask = match id {
                FEATURE_1_AND => 3,
                ISA_1_NEEDED | ISA_1_USED => 0xf,
                FEATURE_2_USED => 0xfff,
                0xc0008001 => return Err("GNU FEATURE_2_NEEDED is unsupported".into()),
                _ => return Err(format!("unsupported GNU property {id:#x}")),
            };
            if size != 4 {
                return Err(format!("GNU property {id:#x} requires a 4-byte payload"));
            }
            let value = word(descriptor, p + 8)?;
            if value & !mask != 0 {
                return Err(format!(
                    "unsupported bits {value:#x} in GNU property {id:#x}"
                ));
            }
            // A known u32 property occupies 16 bytes including four padding bytes.
            let next = p.checked_add(16).ok_or("GNU property offset overflow")?;
            if next > descriptor.len() {
                return Err("truncated GNU property padding".into());
            }
            *out.entry(id).or_default() |= value;
            p = next;
        }
        at = end;
    }
    Ok(())
}

pub(super) fn abi(data: &[u8]) -> Result<()> {
    if data.len() != 32
        || word(data, 0)? != 4
        || word(data, 4)? != 16
        || word(data, 8)? != 1
        || data.get(12..16) != Some(b"GNU\0")
    {
        return Err(".note.ABI-tag requires exactly one 32-byte GNU ABI note".into());
    }
    if word(data, 16)? != 0 {
        return Err(".note.ABI-tag supports only the Linux OS contract".into());
    }
    Ok(())
}

fn merge(objects: &[Properties], generated_code: bool) -> Properties {
    let mut result = Properties::new();
    let feature1 = objects
        .iter()
        .fold(3, |v, p| v & p.get(&FEATURE_1_AND).copied().unwrap_or(0));
    if feature1 != 0 && !objects.is_empty() {
        result.insert(FEATURE_1_AND, feature1);
    }
    let needed = objects
        .iter()
        .fold(0, |v, p| v | p.get(&ISA_1_NEEDED).copied().unwrap_or(0));
    if needed != 0 {
        result.insert(ISA_1_NEEDED, needed);
    }
    for id in [FEATURE_2_USED, ISA_1_USED] {
        if !objects.is_empty() && objects.iter().all(|p| p.contains_key(&id)) {
            let mut used = objects.iter().fold(0, |v, p| v | p[&id]);
            // Emitted FEATURE_2_USED always includes basic x86. Only actual
            // synthesized code adds baseline ISA usage to a complete ISA map.
            if id == FEATURE_2_USED || generated_code {
                used |= 1;
            }
            result.insert(id, used);
        }
    }
    result
}

#[derive(Default)]
pub(super) struct Plan {
    pub property: Option<usize>,
    pub abi: Option<usize>,
}
impl Plan {
    pub fn prepare(
        &mut self,
        sections: &mut Vec<Section>,
        objects: &[Properties],
        generated_code: bool,
    ) -> Result<()> {
        let properties = merge(objects, generated_code);
        if properties.get(&FEATURE_1_AND).is_some_and(|v| v & 1 != 0) {
            return Err("merged GNU IBT property is unsupported until ENDBR64 generated stubs are implemented".into());
        }
        if properties.is_empty() {
            return Ok(());
        }
        let mut data = vec![0; 16 + 16 * properties.len()];
        put32(&mut data, 0, 4);
        put32(&mut data, 4, (16 * properties.len()) as u32);
        put32(&mut data, 8, 5);
        data[12..16].copy_from_slice(b"GNU\0");
        for (i, (id, value)) in properties.into_iter().enumerate() {
            put32(&mut data, 16 + 16 * i, id);
            put32(&mut data, 20 + 16 * i, 4);
            put32(&mut data, 24 + 16 * i, value);
        }
        self.property = Some(sections.len());
        sections.push(Section {
            name: ".note.gnu.property".into(),
            flags: 2,
            kind: 7,
            align: 8,
            size: data.len() as u64,
            bss: false,
            data,
            offset: 0,
        });
        Ok(())
    }
    pub fn header_count(&self) -> usize {
        2 * usize::from(self.property.is_some()) + usize::from(self.abi.is_some())
    }
    pub fn headers(&self, sections: &[Section]) -> Vec<(u32, u32, u64, u64, u64, u64)> {
        let mut headers = Vec::new();
        if let Some(i) = self.property {
            let s = &sections[i];
            for kind in [object::elf::PT_GNU_PROPERTY, object::elf::PT_NOTE] {
                headers.push((kind, 4, s.offset, s.size, s.size, 8));
            }
        }
        if let Some(i) = self.abi {
            let s = &sections[i];
            headers.push((object::elf::PT_NOTE, 4, s.offset, s.size, s.size, 4));
        }
        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_gnu_split_oracle_before_ibt_policy() {
        let mut split = Properties::new();
        properties(
            include_bytes!("../../tests/fixtures/gnu-notes/split.o.note.gnu.property.bin"),
            &mut split,
        )
        .unwrap();
        let mut all = Properties::new();
        properties(
            include_bytes!("../../tests/fixtures/gnu-notes/allprops.o.note.gnu.property.bin"),
            &mut all,
        )
        .unwrap();
        for (objects, oracle) in [
            (
                vec![split.clone()],
                include_bytes!("../../tests/fixtures/gnu-notes/split-alone.note.gnu.property.bin")
                    .as_slice(),
            ),
            (
                vec![split, all],
                include_bytes!("../../tests/fixtures/gnu-notes/split-all.note.gnu.property.bin")
                    .as_slice(),
            ),
        ] {
            let mut expected = Properties::new();
            properties(oracle, &mut expected).unwrap();
            assert_eq!(merge(&objects, false), expected);
            let mut plan = Plan::default();
            assert!(
                plan.prepare(&mut Vec::new(), &objects, false)
                    .unwrap_err()
                    .contains("merged GNU IBT")
            );
        }
    }
}
