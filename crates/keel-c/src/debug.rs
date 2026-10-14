//! C types as debug types, for `-g`.
//!
//! What a debugger is not told: typedef names (`size_t` is shown as
//! `unsigned long`), enums (shown as their integer type) and qualifiers,
//! none of which the parser's types keep.

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
    function: Option<DebugType>,
}

fn push(module: &mut Module, data: DebugTypeData) -> DebugType {
    module.debug_types.push(data);
    DebugType::new(module.debug_types.len() - 1)
}

impl DebugTypes {
    /// The debug type of `ty`, `None` for `void`.
    pub fn get(
        &mut self,
        module: &mut Module,
        structs: &[StructDef],
        ty: &Ty,
    ) -> Option<DebugType> {
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
                let pointee = self.get(module, structs, pointee);
                *self
                    .pointers
                    .entry(pointee)
                    .or_insert_with(|| push(module, DebugTypeData::Pointer(pointee)))
            }
            Ty::Array(elem, count) => {
                let elem = self.get(module, structs, elem)?;
                *self.arrays.entry((elem, *count)).or_insert_with(|| {
                    push(
                        module,
                        DebugTypeData::Array {
                            elem,
                            count: *count,
                        },
                    )
                })
            }
            Ty::Func(_) => *self
                .function
                .get_or_insert_with(|| push(module, DebugTypeData::Function)),
            Ty::Struct(id) => {
                if let Some(&dt) = self.structs.get(id) {
                    return Some(dt);
                }
                let def = &structs[*id];
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
                            ty: self.get(module, structs, &m.ty).expect("a void member"),
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
