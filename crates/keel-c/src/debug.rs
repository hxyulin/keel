//! C types as debug types, for `-g`: a [`Ty`] with its [`Sugar`], which
//! adds the typedef names, enums and qualifiers.

use crate::parse::Parser;
use crate::types::*;
use keel_ir::{DebugMember, DebugType, DebugTypeData, Encoding, EntityRef, MemberLocation, Module};
use std::collections::HashMap;

/// Adds C types to a module's debug type table, each once.
#[derive(Default)]
pub struct DebugTypes {
    bases: HashMap<&'static str, DebugType>,
    pointers: HashMap<Option<DebugType>, DebugType>,
    arrays: HashMap<(DebugType, Option<u64>), DebugType>,
    structs: HashMap<usize, DebugType>,
    typedefs: HashMap<usize, DebugType>,
    enums: HashMap<usize, DebugType>,
    /// By the type qualified and whether `volatile`.
    qualified: HashMap<(Option<DebugType>, bool), DebugType>,
    function: Option<DebugType>,
}

fn push(module: &mut Module, data: DebugTypeData) -> DebugType {
    module.debug_types.push(data);
    DebugType::new(module.debug_types.len() - 1)
}

impl DebugTypes {
    /// The debug type of `ty` with `sugar`, `None` for `void`.
    pub fn get(
        &mut self,
        module: &mut Module,
        p: &Parser,
        ty: &Ty,
        sugar: &Sugar,
    ) -> Option<DebugType> {
        // DWARF 4 has no atomic types.
        let ty = ty.unatomic();
        match (sugar, ty) {
            (Sugar::Typedef(i), _) => {
                if let Some(&dt) = self.typedefs.get(i) {
                    return Some(dt);
                }
                let def = &p.typedefs[*i];
                let target = self.get(module, p, &def.ty, &def.sugar);
                let dt = push(
                    module,
                    DebugTypeData::Typedef {
                        name: def.name.clone(),
                        ty: target,
                    },
                );
                self.typedefs.insert(*i, dt);
                Some(dt)
            }
            (Sugar::Enum(i), Ty::Int(_)) => {
                if let Some(&dt) = self.enums.get(i) {
                    return Some(dt);
                }
                let def = &p.enums[*i];
                let base = self.get(module, p, &def.ty, &Sugar::Plain)?;
                let dt = push(
                    module,
                    DebugTypeData::Enum {
                        name: def.tag.clone(),
                        ty: base,
                        values: def.values.clone(),
                    },
                );
                self.enums.insert(*i, dt);
                Some(dt)
            }
            (Sugar::Const(inner) | Sugar::Volatile(inner), _) => {
                let volatile = matches!(sugar, Sugar::Volatile(_));
                let inner = self.get(module, p, ty, inner);
                Some(*self.qualified.entry((inner, volatile)).or_insert_with(|| {
                    let data = if volatile {
                        DebugTypeData::Volatile(inner)
                    } else {
                        DebugTypeData::Const(inner)
                    };
                    push(module, data)
                }))
            }
            (Sugar::Ptr(inner), Ty::Ptr(pointee)) => {
                let pointee = self.get(module, p, pointee, inner);
                Some(self.pointer(module, pointee))
            }
            (Sugar::Array(inner), Ty::Array(elem, count)) => {
                let elem = self.get(module, p, elem, inner)?;
                Some(self.array(module, elem, *count))
            }
            _ => self.plain(module, p, ty),
        }
    }

    fn pointer(&mut self, module: &mut Module, pointee: Option<DebugType>) -> DebugType {
        *self
            .pointers
            .entry(pointee)
            .or_insert_with(|| push(module, DebugTypeData::Pointer(pointee)))
    }

    fn array(&mut self, module: &mut Module, elem: DebugType, count: Option<u64>) -> DebugType {
        *self
            .arrays
            .entry((elem, count))
            .or_insert_with(|| push(module, DebugTypeData::Array { elem, count }))
    }

    /// `ty` with nothing more known.
    fn plain(&mut self, module: &mut Module, p: &Parser, ty: &Ty) -> Option<DebugType> {
        Some(match ty {
            Ty::Void => return None,
            Ty::Int(_) | Ty::Float | Ty::Double | Ty::Half => {
                let (name, encoding, size) = base(ty);
                *self.bases.entry(name).or_insert_with(|| {
                    push(
                        module,
                        DebugTypeData::Base {
                            name: name.into(),
                            encoding,
                            size,
                        },
                    )
                })
            }
            Ty::Ptr(pointee) => {
                let pointee = self.plain(module, p, pointee);
                self.pointer(module, pointee)
            }
            Ty::Array(elem, count) => {
                let elem = self.plain(module, p, elem)?;
                self.array(module, elem, *count)
            }
            Ty::Func(_) => *self
                .function
                .get_or_insert_with(|| push(module, DebugTypeData::Function)),
            Ty::Atomic(t) => return self.plain(module, p, t),
            Ty::Struct(id) => {
                if let Some(&dt) = self.structs.get(id) {
                    return Some(dt);
                }
                let def = &p.structs[*id];
                // Enter it before its members, which may point back to it.
                let dt = push(
                    module,
                    DebugTypeData::Struct {
                        name: def.tag.clone(),
                        union: def.is_union,
                        size: None,
                        members: Vec::new(),
                    },
                );
                self.structs.insert(*id, dt);
                if def.complete {
                    let members = def
                        .members
                        .iter()
                        .map(|m| DebugMember {
                            name: m.name.clone(),
                            ty: self.get(module, p, &m.ty, &m.sugar).expect("a void member"),
                            location: match m.bits {
                                None => MemberLocation::Byte(m.offset),
                                Some(b) => MemberLocation::Bits {
                                    offset: 8 * m.offset + u64::from(b.shift),
                                    size: b.width,
                                },
                            },
                        })
                        .collect();
                    if let DebugTypeData::Struct {
                        size, members: m, ..
                    } = &mut module.debug_types[dt.index()]
                    {
                        *size = Some(def.size);
                        *m = members;
                    }
                }
                dt
            }
        })
    }
}

/// A base type's C name, encoding and size.
fn base(ty: &Ty) -> (&'static str, Encoding, u32) {
    use Encoding::*;
    let Ty::Int(kind) = ty else {
        return match ty {
            Ty::Float => ("float", Float, 4),
            Ty::Double => ("double", Float, 8),
            _ => ("_Float16", Float, 2),
        };
    };
    let (name, encoding) = match kind {
        IntKind::Bool => ("_Bool", Bool),
        IntKind::Char => ("char", SignedChar),
        IntKind::SChar => ("signed char", SignedChar),
        IntKind::UChar => ("unsigned char", UnsignedChar),
        IntKind::Short => ("short", Signed),
        IntKind::UShort => ("unsigned short", Unsigned),
        IntKind::Int => ("int", Signed),
        IntKind::UInt => ("unsigned int", Unsigned),
        IntKind::Long => ("long", Signed),
        IntKind::ULong => ("unsigned long", Unsigned),
        IntKind::LLong => ("long long", Signed),
        IntKind::ULLong => ("unsigned long long", Unsigned),
        IntKind::Int128 => ("__int128", Signed),
        IntKind::UInt128 => ("unsigned __int128", Unsigned),
    };
    (name, encoding, kind.size() as u32)
}
