//! TLS GOT records keep access models separate even for the same symbol.
use super::{BASE, Got, Placed, Reloc, Section, Unit, dynamic, put64, selected};
use crate::Result;
use object::elf::{
    R_AARCH64_TLS_DTPMOD, R_AARCH64_TLS_DTPREL, R_AARCH64_TLS_TPREL, R_AARCH64_TLSDESC,
    R_X86_64_DTPMOD64, R_X86_64_DTPOFF64, R_X86_64_GOTPC32_TLSDESC, R_X86_64_GOTTPOFF,
    R_X86_64_TLSDESC, R_X86_64_TLSDESC_CALL, R_X86_64_TLSGD, R_X86_64_TLSLD, R_X86_64_TPOFF64,
    STT_TLS,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Model {
    Initial,
    General,
    Local,
    Descriptor,
}

pub(super) fn model(kind: u32, arm: bool) -> Option<Model> {
    if arm {
        return match kind {
            512..=516 => Some(Model::General),
            517..=522 => Some(Model::Local),
            539..=543 => Some(Model::Initial),
            560..=569 => Some(Model::Descriptor),
            _ => None,
        };
    }
    match kind {
        R_X86_64_GOTTPOFF => Some(Model::Initial),
        R_X86_64_TLSGD => Some(Model::General),
        R_X86_64_TLSLD => Some(Model::Local),
        R_X86_64_GOTPC32_TLSDESC | R_X86_64_TLSDESC_CALL => Some(Model::Descriptor),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct Plan {
    section: usize,
    slots: BTreeMap<((usize, usize), Model), u64>,
    resolver: Option<usize>,
    arm: bool,
}
impl Plan {
    pub fn prepare(
        sections: &mut Vec<Section>,
        units: &[Unit],
        selected: &selected::Index<'_>,
        arm: bool,
    ) -> Result<Self> {
        let mut plan = Self {
            arm,
            ..Self::default()
        };
        let mut size = 0;
        {
            for (ui, unit) in units.iter().enumerate() {
                for r in &unit.relocs {
                    if let Some(model) = model(r.kind, arm) {
                        let selection = selected.get(ui, r.symbol)?;
                        if model != Model::Local && selection.definition.info & 15 != STT_TLS {
                            return Err(format!(
                                "{}: TLS relocation requires a TLS symbol",
                                unit.name
                            )
                            .into());
                        }
                        let key = if model == Model::Local {
                            (0, 0)
                        } else {
                            selection.key
                        };
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            plan.slots.entry((key, model))
                        {
                            entry.insert(size);
                            size = super::end(size, if model == Model::Initial { 8 } else { 16 })?;
                        }
                    }
                }
            }
        }
        plan.section = sections.len();
        sections.push(Section {
            merge: None,
            name: ".got.tls".into(),
            flags: 3,
            kind: 1,
            align: 8,
            size,
            bss: false,
            data: vec![0; size as usize].into(),
            offset: 0,
        });
        let mut needs_resolver = false;
        for &(key, model) in plan.slots.keys() {
            if model == Model::Descriptor && selected.get(key.0, key.1)?.import.is_none() {
                needs_resolver = true;
            }
        }
        if needs_resolver {
            plan.resolver = Some(sections.len());
            // TLSDESC passes its descriptor in RAX (x86-64) or X0 (AArch64).
            // Return its TP offset while preserving every other register.
            sections.push(Section {
                merge: None,
                name: ".text.tls".into(),
                flags: 6,
                kind: 1,
                align: 16,
                size: if arm { 8 } else { 5 },
                bss: false,
                data: if arm {
                    vec![0x00, 0x04, 0x40, 0xf9, 0xc0, 0x03, 0x5f, 0xd6].into()
                } else {
                    vec![0x48, 0x8b, 0x40, 8, 0xc3].into()
                },
                offset: 0,
            });
        }
        Ok(plan)
    }
    pub fn script_address(
        &self,
        key: (usize, usize),
        model: Model,
        address: impl Fn(usize) -> Result<(u64, u64)>,
    ) -> Result<u64> {
        let key = if model == Model::Local { (0, 0) } else { key };
        Ok(address(self.section)?.0 + self.slots[&(key, model)])
    }
    pub fn emit_script(
        &self,
        bytes: &mut [u8],
        address: impl Fn(usize) -> Result<(u64, u64)>,
        symbol: impl Fn((usize, usize)) -> Result<u64>,
        size: u64,
        alignment: u64,
    ) -> Result<()> {
        if self.slots.is_empty() {
            return Ok(());
        }
        let (_, file) = address(self.section)?;
        for (&(key, model), &offset) in &self.slots {
            let at = (file + offset) as usize;
            if model == Model::Local {
                put64(bytes, at, 1);
                continue;
            }
            let value = symbol(key)?;
            let tp = value.wrapping_sub(super::align(size, alignment)?);
            let resolver = self.resolver.map(|i| address(i).map(|a| a.0)).transpose()?;
            write_static(bytes, at, model, value, tp, resolver)?;
        }
        Ok(())
    }
    pub fn relocation_count(&self, selected: &selected::Index<'_>) -> Result<usize> {
        let mut count = 0;
        for &(key, model) in self.slots.keys() {
            if model == Model::Local {
                continue;
            }
            if selected.get(key.0, key.1)?.import.is_some() {
                count += if model == Model::General { 2 } else { 1 };
            }
        }
        Ok(count)
    }
    pub fn address(&self, sections: &[Section], key: (usize, usize), model: Model) -> u64 {
        let key = if model == Model::Local { (0, 0) } else { key };
        BASE + sections[self.section].offset + self.slots[&(key, model)]
    }
    pub fn emit(
        &self,
        placed: &Placed<'_>,
        bytes: &mut [u8],
        dynamic: &mut Option<dynamic::Dynamic>,
    ) -> Result<()> {
        for (&(key, model), &offset) in &self.slots {
            let at = (placed.sections[self.section].offset + offset) as usize;
            if model == Model::Local {
                if super::tls_header(placed.sections).is_none() {
                    return Err("TLSLD requires an executable TLS template".into());
                }
                put64(bytes, at, 1);
                continue;
            }
            let selection = placed.selected.get(key.0, key.1)?;
            if let Some(name) = selection.import {
                let d = dynamic
                    .as_mut()
                    .ok_or("imported TLS requires dynamic output")?;
                let symbol = d.imports[name].index;
                let address = BASE + at as u64;
                match model {
                    Model::Initial => d.relocations.push((
                        address,
                        symbol,
                        if self.arm {
                            R_AARCH64_TLS_TPREL
                        } else {
                            R_X86_64_TPOFF64
                        },
                        0,
                    )),
                    Model::General => {
                        d.relocations.push((
                            address,
                            symbol,
                            if self.arm {
                                R_AARCH64_TLS_DTPMOD
                            } else {
                                R_X86_64_DTPMOD64
                            },
                            0,
                        ));
                        d.relocations.push((
                            address + 8,
                            symbol,
                            if self.arm {
                                R_AARCH64_TLS_DTPREL
                            } else {
                                R_X86_64_DTPOFF64
                            },
                            0,
                        ));
                    }
                    Model::Descriptor => d.relocations.push((
                        address,
                        symbol,
                        if self.arm {
                            R_AARCH64_TLSDESC
                        } else {
                            R_X86_64_TLSDESC
                        },
                        0,
                    )),
                    Model::Local => unreachable!(),
                }
            } else {
                let offset = placed.definition_address(key.0, selection.definition)?;
                let tls = super::tls_header(placed.sections).ok_or("TLS GOT without template")?;
                let tp = if self.arm {
                    offset + super::align(16, tls.5)?
                } else {
                    offset.wrapping_sub(super::align(tls.4, tls.5)?)
                };
                let resolver = self.resolver.map(|i| BASE + placed.sections[i].offset);
                write_static(bytes, at, model, offset, tp, resolver)?;
            }
        }
        Ok(())
    }
}

fn write_static(
    bytes: &mut [u8],
    at: usize,
    model: Model,
    offset: u64,
    tp: u64,
    resolver: Option<u64>,
) -> Result<()> {
    match model {
        Model::Initial => put64(bytes, at, tp),
        Model::General => {
            put64(bytes, at, 1);
            put64(bytes, at + 8, offset);
        }
        Model::Descriptor => {
            put64(bytes, at, resolver.ok_or("missing TLS resolver")?);
            put64(bytes, at + 8, tp);
        }
        Model::Local => put64(bytes, at, 1),
    }
    Ok(())
}

pub(super) fn value(
    placed: &Placed<'_>,
    got: &Got,
    r: &Reloc,
    ui: usize,
    model: Model,
) -> Result<i128> {
    let selection = placed.selected.get(ui, r.symbol)?;
    let address = got.tls.address(placed.sections, selection.key, model);
    let value = if got.tls.arm && matches!(r.kind, 515 | 516 | 520 | 521 | 539 | 540 | 565 | 566) {
        i128::from(address) - i128::from(BASE + placed.sections[got.section].offset)
    } else {
        i128::from(address)
    };
    Ok(value)
}
