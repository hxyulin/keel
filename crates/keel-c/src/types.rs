//! C types, with the sizes and alignments of Apple arm64 (LP64, signed
//! `char`, `long double` the same as `double`).

use bumpalo::Bump;
use keel_ir::{Layout, Type};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IntKind {
    Bool,
    Char,
    SChar,
    UChar,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    LLong,
    ULLong,
    Int128,
    UInt128,
}

impl IntKind {
    pub fn size(self) -> u64 {
        match self {
            IntKind::Bool | IntKind::Char | IntKind::SChar | IntKind::UChar => 1,
            IntKind::Short | IntKind::UShort => 2,
            IntKind::Int | IntKind::UInt => 4,
            IntKind::Long | IntKind::ULong | IntKind::LLong | IntKind::ULLong => 8,
            IntKind::Int128 | IntKind::UInt128 => 16,
        }
    }

    pub fn signed(self) -> bool {
        matches!(
            self,
            IntKind::Char
                | IntKind::SChar
                | IntKind::Short
                | IntKind::Int
                | IntKind::Long
                | IntKind::LLong
                | IntKind::Int128
        )
    }

    /// The conversion rank: which of two types wins the usual arithmetic
    /// conversions.
    pub fn rank(self) -> u8 {
        match self {
            IntKind::Bool => 0,
            IntKind::Char | IntKind::SChar | IntKind::UChar => 1,
            IntKind::Short | IntKind::UShort => 2,
            IntKind::Int | IntKind::UInt => 3,
            IntKind::Long | IntKind::ULong => 4,
            IntKind::LLong | IntKind::ULLong => 5,
            IntKind::Int128 | IntKind::UInt128 => 6,
        }
    }

    pub fn to_unsigned(self) -> IntKind {
        match self {
            IntKind::Char | IntKind::SChar => IntKind::UChar,
            IntKind::Short => IntKind::UShort,
            IntKind::Int => IntKind::UInt,
            IntKind::Long => IntKind::ULong,
            IntKind::LLong => IntKind::ULLong,
            IntKind::Int128 => IntKind::UInt128,
            k => k,
        }
    }
}

/// A C type. Composite types refer to nodes in the compilation's arena, so
/// a type is two words and copies freely.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Ty<'a> {
    Void,
    Int(IntKind),
    Float,
    /// `double` and `long double`.
    Double,
    /// `_Float16`: only so headers declaring functions with it parse.
    Half,
    /// A type keel only knows the size of, so headers declaring functions
    /// and typedefs with it parse: `__float128`, `_Complex double`, and
    /// `long double` on x86-64. Its values cannot be used.
    Opaque(&'a OpaqueTy),
    Ptr(&'a Ty<'a>),
    Array(&'a ArrayTy<'a>),
    Func(&'a FuncTy<'a>),
    /// A struct or union, by its index in the struct table.
    Struct(usize),
    /// `_Atomic(T)`, of an integer or pointer: reading the object is an
    /// atomic load, assigning it an atomic store, `+=` and `++` atomic
    /// read-modify-writes, all seq_cst. A value of it has type `T`.
    Atomic(&'a Ty<'a>),
}

#[derive(PartialEq, Debug)]
pub struct OpaqueTy {
    pub name: &'static str,
    pub size: u64,
}

/// Element type and length; no length for `int a[]`.
#[derive(PartialEq, Debug)]
pub struct ArrayTy<'a> {
    pub elem: Ty<'a>,
    pub len: Option<u64>,
}

#[derive(PartialEq, Debug)]
pub struct FuncTy<'a> {
    pub ret: Ty<'a>,
    pub params: &'a [Ty<'a>],
    pub variadic: bool,
    /// False for `int f()`, which says nothing about the parameters.
    pub prototyped: bool,
}

/// What a declaration says about a type that [`Ty`] does not keep, for
/// debuggers and access semantics: typedef names, enums and qualifiers. It has the shape of
/// the `Ty` it goes with, down to where it stops saying more (`Plain`).
/// Type checking only looks at the `Ty`.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub enum Sugar<'a> {
    #[default]
    Plain,
    /// The typedef by its index in the typedef table.
    Typedef(usize),
    /// The enum by its index in the enum table.
    Enum(usize),
    Const(&'a Sugar<'a>),
    Volatile(&'a Sugar<'a>),
    /// C restrict qualification on this type layer.
    Restrict(&'a Sugar<'a>),
    /// A pointer, an array, or a function (its return type).
    Ptr(&'a Sugar<'a>),
    Array(&'a Sugar<'a>),
    Func(&'a Sugar<'a>),
}

/// The shared `Plain` node that sugar wrapping nothing refers to.
pub static PLAIN_SUGAR: Sugar<'static> = Sugar::Plain;

impl<'a> Sugar<'a> {
    /// The outer type constructor, resolving typedefs and qualifiers.
    pub fn unqualified<'s>(&'s self, defs: &'s [TypedefDef<'a>]) -> &'s Sugar<'a> {
        match self {
            Self::Typedef(i) => defs[*i].sugar.unqualified(defs),
            Self::Const(inner) | Self::Volatile(inner) | Self::Restrict(inner) => {
                inner.unqualified(defs)
            }
            other => other,
        }
    }

    /// Alignment attached to an alias, inherited through qualifiers and arrays.
    pub fn alignment(&self, defs: &[TypedefDef<'_>]) -> Option<u64> {
        match self {
            Self::Typedef(i) => defs[*i].align.or_else(|| defs[*i].sugar.alignment(defs)),
            Self::Const(s) | Self::Volatile(s) | Self::Restrict(s) | Self::Array(s) => {
                s.alignment(defs)
            }
            _ => None,
        }
    }

    pub fn is_volatile(&self, defs: &[TypedefDef<'_>]) -> bool {
        match self {
            Self::Volatile(_) => true,
            Self::Const(inner) | Self::Restrict(inner) | Self::Array(inner) => {
                inner.is_volatile(defs)
            }
            Self::Typedef(i) => defs[*i].sugar.is_volatile(defs),
            _ => false,
        }
    }

    pub fn is_restrict(&self, defs: &[TypedefDef<'_>]) -> bool {
        match self {
            Self::Restrict(_) => true,
            Self::Const(s) | Self::Volatile(s) => s.is_restrict(defs),
            Self::Typedef(i) => defs[*i].sugar.is_restrict(defs),
            _ => false,
        }
    }

    pub fn pointee(&self, defs: &[TypedefDef<'a>]) -> Sugar<'a> {
        match *self.unqualified(defs) {
            Self::Ptr(inner) => *inner,
            Self::Array(inner) if self.is_volatile(defs) => Self::Volatile(inner),
            Self::Array(inner) => *inner,
            _ => Self::Plain,
        }
    }

    pub fn function_return(&self, defs: &[TypedefDef<'a>]) -> Sugar<'a> {
        let target = self.pointee(defs);
        match *target.unqualified(defs) {
            Self::Func(ret) => *ret,
            _ => Self::Plain,
        }
    }

    /// Conditional pointers carry qualifiers from both operands. Expand
    /// aliases here so nested pointer/array qualifications merge by depth.
    pub fn merge(&self, other: &Sugar<'a>, defs: &[TypedefDef<'a>], arena: &'a Bump) -> Sugar<'a> {
        let volatile = self.is_volatile(defs) || other.is_volatile(defs);
        let merged = match (self.unqualified(defs), other.unqualified(defs)) {
            (Self::Ptr(a), Self::Ptr(b)) => Self::Ptr(arena.alloc(a.merge(b, defs, arena))),
            (Self::Array(a), Self::Array(b)) => Self::Array(arena.alloc(a.merge(b, defs, arena))),
            (Self::Func(a), Self::Func(b)) => Self::Func(arena.alloc(a.merge(b, defs, arena))),
            (Self::Plain, b) => *b,
            (a, _) => *a,
        };
        if volatile {
            Self::Volatile(arena.alloc(merged))
        } else {
            merged
        }
    }

    /// `Plain` when nothing is known, else `wrap` of `self`.
    pub fn wrap(self, wrap: fn(&'a Sugar<'a>) -> Sugar<'a>, arena: &'a Bump) -> Sugar<'a> {
        match self {
            Sugar::Plain => Sugar::Plain,
            s => wrap(arena.alloc(s)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypedefDef<'a> {
    pub name: &'a str,
    pub ty: Ty<'a>,
    pub sugar: Sugar<'a>,
    /// GNU typedef alignment does not change size or type compatibility.
    pub align: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct EnumDef<'a> {
    pub tag: Option<&'a str>,
    /// The type of its values.
    pub ty: Ty<'a>,
    pub values: Vec<(&'a str, i64)>,
}

#[derive(Clone, Debug)]
pub struct StructDef<'a> {
    pub tag: Option<&'a str>,
    pub is_union: bool,
    pub members: Vec<Member<'a>>,
    pub size: u64,
    pub align: u64,
    pub complete: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct Member<'a> {
    /// `None` for an anonymous struct or union, whose members are found
    /// as if they were this struct's, and for unnamed bit-fields.
    pub name: Option<&'a str>,
    pub ty: Ty<'a>,
    pub sugar: Sugar<'a>,
    pub offset: u64,
    pub align: u64,
    pub bits: Option<BitField>,
}

/// Where a bit-field is: `width` bits from bit `shift` of the storage unit
/// of its declared type at the member's offset.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct BitField {
    pub shift: u32,
    pub width: u32,
}

pub const INT: Ty<'static> = Ty::Int(IntKind::Int);
pub const UINT: Ty<'static> = Ty::Int(IntKind::UInt);
pub const LONG: Ty<'static> = Ty::Int(IntKind::Long);
pub const ULONG: Ty<'static> = Ty::Int(IntKind::ULong);
pub const CHAR: Ty<'static> = Ty::Int(IntKind::Char);

/// Scalar nodes, shared by every compilation.
static SCALAR_NODES: [Ty<'static>; IntKind::UInt128 as usize + 5] = [
    Ty::Void,
    Ty::Int(IntKind::Bool),
    Ty::Int(IntKind::Char),
    Ty::Int(IntKind::SChar),
    Ty::Int(IntKind::UChar),
    Ty::Int(IntKind::Short),
    Ty::Int(IntKind::UShort),
    Ty::Int(IntKind::Int),
    Ty::Int(IntKind::UInt),
    Ty::Int(IntKind::Long),
    Ty::Int(IntKind::ULong),
    Ty::Int(IntKind::LLong),
    Ty::Int(IntKind::ULLong),
    Ty::Int(IntKind::Int128),
    Ty::Int(IntKind::UInt128),
    Ty::Float,
    Ty::Double,
    Ty::Half,
];

/// Type nodes, in the compilation's arena. Leaf nodes are shared; struct
/// nodes identify the mutable table by index, so sharing does not freeze
/// an incomplete tag's eventual definition.
pub(crate) struct TypeNodes<'a> {
    arena: &'a Bump,
    structs: Vec<Option<&'a Ty<'a>>>,
}

impl<'a> TypeNodes<'a> {
    pub fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            structs: Vec::new(),
        }
    }

    pub fn node(&mut self, ty: Ty<'a>) -> &'a Ty<'a> {
        match ty {
            Ty::Void => &SCALAR_NODES[0],
            Ty::Int(kind) => &SCALAR_NODES[1 + kind as usize],
            Ty::Float => &SCALAR_NODES[IntKind::UInt128 as usize + 2],
            Ty::Double => &SCALAR_NODES[IntKind::UInt128 as usize + 3],
            Ty::Half => &SCALAR_NODES[IntKind::UInt128 as usize + 4],
            Ty::Struct(id) => {
                if self.structs.len() <= id {
                    self.structs.resize(id + 1, None);
                }
                let arena = self.arena;
                self.structs[id].get_or_insert_with(|| arena.alloc(ty))
            }
            other => self.arena.alloc(other),
        }
    }
}

impl<'a> Ty<'a> {
    /// The type without `_Atomic`.
    pub fn unatomic(&self) -> &Ty<'a> {
        match self {
            Ty::Atomic(t) => t,
            t => t,
        }
    }

    pub fn int_kind(&self) -> Option<IntKind> {
        match self {
            Ty::Int(k) => Some(*k),
            _ => None,
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Ty::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Ty::Float | Ty::Double | Ty::Half)
    }

    pub fn is_arith(&self) -> bool {
        self.is_integer() || self.is_float()
    }

    pub fn is_ptr(&self) -> bool {
        matches!(self, Ty::Ptr(_))
    }

    pub fn is_scalar(&self) -> bool {
        self.is_arith() || self.is_ptr()
    }

    pub fn is_struct(&self) -> bool {
        matches!(self, Ty::Struct(_))
    }

    pub fn is_signed(&self) -> bool {
        self.int_kind().is_some_and(IntKind::signed)
    }

    /// What a pointer or array points to.
    pub fn pointee(&self) -> Option<&'a Ty<'a>> {
        match *self {
            Ty::Ptr(t) => Some(t),
            Ty::Array(a) => Some(&a.elem),
            _ => None,
        }
    }

    pub fn func(&self) -> Option<&'a FuncTy<'a>> {
        match *self {
            Ty::Func(f) => Some(f),
            _ => None,
        }
    }

    /// The keel type of a scalar.
    /// The name of a type whose values keel cannot use, if this is one:
    /// `_Float16` and the opaque types.
    pub fn unusable(&self) -> Option<&'static str> {
        match self {
            Ty::Half => Some("_Float16"),
            Ty::Opaque(o) => Some(o.name),
            _ => None,
        }
    }

    pub fn ir(&self) -> Type {
        match self {
            Ty::Int(k) => match k.size() {
                1 => Type::I8,
                2 => Type::I16,
                4 => Type::I32,
                8 => Type::I64,
                _ => Type::I128,
            },
            Ty::Float => Type::F32,
            Ty::Double => Type::F64,
            Ty::Ptr(_) => Type::Ptr,
            _ => panic!("{self:?} is not a scalar"),
        }
    }
}

impl<'a> StructDef<'a> {
    /// The member called `name`, looking inside anonymous members; gives
    /// the path of members to it, and its offset.
    pub fn find(
        &self,
        name: &str,
        structs: &[StructDef<'a>],
        defs: &[TypedefDef<'a>],
        arena: &'a Bump,
    ) -> Option<(u64, Member<'a>)> {
        for m in &self.members {
            match &m.name {
                Some(n) if *n == name => return Some((m.offset, *m)),
                None if m.bits.is_none() => {
                    if let Ty::Struct(id) = m.ty
                        && let Some((off, mut found)) = structs[id].find(name, structs, defs, arena)
                    {
                        if m.sugar.is_volatile(defs) {
                            found.sugar = Sugar::Volatile(arena.alloc(found.sugar));
                        }
                        return Some((m.offset + off, found));
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// Sizes, alignments and ABI layouts need the struct table.
pub struct Types<'a>(pub &'a [StructDef<'a>]);

impl Types<'_> {
    pub fn size(&self, ty: &Ty) -> u64 {
        match ty {
            Ty::Void => 1,
            Ty::Int(k) => k.size(),
            Ty::Float => 4,
            Ty::Double | Ty::Ptr(_) => 8,
            Ty::Half => 2,
            Ty::Opaque(o) => o.size,
            Ty::Array(a) => self.size(&a.elem) * a.len.unwrap_or(0),
            Ty::Func(_) => 1,
            Ty::Struct(id) => self.0[*id].size,
            Ty::Atomic(t) => self.size(t),
        }
    }

    pub fn align(&self, ty: &Ty) -> u64 {
        match ty {
            Ty::Array(a) => self.align(&a.elem),
            Ty::Struct(id) => self.0[*id].align,
            Ty::Atomic(t) => self.align(t),
            Ty::Void | Ty::Func(_) => 1,
            _ => self.size(ty),
        }
    }

    /// The layout the calling convention classifies a struct by. Only two
    /// things matter: the size, and whether it is a homogeneous float
    /// aggregate (one to four floats of one type). So an HFA is an array
    /// of its float, and anything else an array of integers as wide as
    /// its alignment, which has the same size and alignment.
    pub fn abi_layout(&self, ty: &Ty) -> Layout {
        if let Some((float, n)) = self.hfa(ty)
            && n <= 4
        {
            return Layout::Array(Box::new(Layout::Scalar(float)), n as u32);
        }
        let align = self.align(ty);
        let unit = match align {
            1 => Type::I8,
            2 => Type::I16,
            4 => Type::I32,
            8 => Type::I64,
            _ => Type::I128,
        };
        let n = self.size(ty) / align;
        Layout::Array(Box::new(Layout::Scalar(unit)), n as u32)
    }

    /// SysV classifies aggregates with unaligned scalar fields as MEMORY.
    /// The current IR layouts cannot express those explicit field offsets.
    pub fn has_unaligned_fields(&self, ty: &Ty, base: u64) -> bool {
        match ty {
            Ty::Struct(id) => self.0[*id]
                .members
                .iter()
                .any(|m| m.bits.is_none() && self.has_unaligned_fields(&m.ty, base + m.offset)),
            Ty::Array(a) => self.has_unaligned_fields(&a.elem, base),
            _ => !base.is_multiple_of(self.align(ty)),
        }
    }

    pub fn contains_atomic(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Atomic(_) => true,
            Ty::Struct(id) => self.0[*id]
                .members
                .iter()
                .any(|m| self.contains_atomic(&m.ty)),
            Ty::Array(a) => self.contains_atomic(&a.elem),
            _ => false,
        }
    }

    /// The float type and count if `ty` is made only of floats of one type.
    fn hfa(&self, ty: &Ty) -> Option<(Type, u64)> {
        match ty {
            Ty::Float => Some((Type::F32, 1)),
            Ty::Double => Some((Type::F64, 1)),
            Ty::Array(ArrayTy { elem, len: Some(n) }) => self.hfa(elem).map(|(t, k)| (t, k * n)),
            Ty::Struct(id) => {
                let def = &self.0[*id];
                if def.is_union {
                    return None;
                }
                let mut float = None;
                let mut count = 0;
                for m in &def.members {
                    if m.bits.is_some() {
                        return None;
                    }
                    let (t, k) = self.hfa(&m.ty)?;
                    if float.is_some_and(|f| f != t) {
                        return None;
                    }
                    float = Some(t);
                    count += k;
                }
                // No padding: the floats must be all there is.
                let float = float?;
                (count * u64::from(float.bytes().unwrap()) == def.size).then_some((float, count))
            }
            _ => None,
        }
    }
}

pub type StructField<'a> = (
    Option<&'a str>,
    Ty<'a>,
    Sugar<'a>,
    Option<u32>,
    Option<u64>,
    bool,
);

/// Lays out a struct or union's members, including alignment and packing,
/// returning the members with their offsets, the size and the alignment.
/// `max_align` caps member alignment for `#pragma pack(n)`.
/// Bit-fields follow AAPCS64: a bit-field goes in the next bits that do not
/// cross a boundary of its declared type's size, and a zero-width one
/// aligns the next to its type.
pub fn layout_struct<'a>(
    types: &Types,
    typedefs: &[TypedefDef<'a>],
    is_union: bool,
    fields: Vec<StructField<'a>>,
    max_align: Option<u64>,
) -> Result<(Vec<Member<'a>>, u64, u64), &'static str> {
    let mut members = Vec::new();
    let mut bit: u64 = 0;
    let mut size: u64 = 0;
    let mut align: u64 = 1;
    let mut packed_accesses = Vec::new();
    for (name, ty, sugar, width, requested_align, packed) in fields {
        let natural = sugar
            .alignment(typedefs)
            .unwrap_or_else(|| types.align(&ty));
        // Explicit GNU alignment overrides packed layout, but pragma pack
        // caps even an explicitly aligned member.
        let falign = if packed { 1 } else { natural }
            .max(requested_align.unwrap_or(1))
            .min(max_align.unwrap_or(u64::MAX));
        let fsize = types.size(&ty);
        if is_union {
            bit = 0;
        }
        match width {
            Some(0) => {
                bit = bit.next_multiple_of(natural * 8);
            }
            Some(w) => {
                if requested_align.is_some() {
                    bit = bit.next_multiple_of(falign * 8);
                }
                let unit_bits = fsize * 8;
                if bit / unit_bits != (bit + u64::from(w) - 1) / unit_bits {
                    if packed {
                        return Err(
                            "packed bit-fields requiring cross-container accesses are not supported",
                        );
                    }
                    bit = bit.next_multiple_of(unit_bits);
                }
                let offset = bit / unit_bits * fsize;
                if packed {
                    packed_accesses.push((offset, fsize));
                }
                members.push(Member {
                    name,
                    ty,
                    sugar,
                    offset,
                    align: falign,
                    bits: Some(BitField {
                        shift: (bit - offset * 8) as u32,
                        width: w,
                    }),
                });
                bit += u64::from(w);
                align = align.max(falign);
            }
            None => {
                bit = bit.next_multiple_of(falign * 8);
                members.push(Member {
                    name,
                    ty,
                    sugar,
                    offset: bit / 8,
                    align: falign,
                    bits: None,
                });
                bit += fsize * 8;
                align = align.max(falign);
            }
        }
        size = size.max(bit.div_ceil(8));
    }
    let size = size.next_multiple_of(align);
    if packed_accesses
        .iter()
        .any(|(offset, width)| offset + width > size)
    {
        return Err("packed bit-fields requiring partial-container accesses are not supported");
    }
    for m in &members {
        let natural = types.align(&m.ty);
        if types.contains_atomic(&m.ty) && (m.align < natural || !m.offset.is_multiple_of(natural))
        {
            return Err("unaligned atomic members are not supported");
        }
    }
    Ok((members, size, align))
}

#[cfg(test)]
mod node_tests {
    use super::*;

    #[test]
    fn common_nodes_share_storage_without_conflating_types() {
        let arena = Bump::new();
        let mut nodes = TypeNodes::new(&arena);
        let types = [
            Ty::Void,
            Ty::Int(IntKind::Bool),
            Ty::Int(IntKind::Char),
            Ty::Int(IntKind::SChar),
            Ty::Int(IntKind::UChar),
            Ty::Int(IntKind::Short),
            Ty::Int(IntKind::UShort),
            Ty::Int(IntKind::Int),
            Ty::Int(IntKind::UInt),
            Ty::Int(IntKind::Long),
            Ty::Int(IntKind::ULong),
            Ty::Int(IntKind::LLong),
            Ty::Int(IntKind::ULLong),
            Ty::Int(IntKind::Int128),
            Ty::Int(IntKind::UInt128),
            Ty::Float,
            Ty::Double,
            Ty::Half,
            Ty::Struct(2),
            Ty::Struct(100),
        ];
        for ty in &types {
            let a = nodes.node(*ty);
            let b = nodes.node(*ty);
            assert!(std::ptr::eq(a, b));
            assert_eq!(a, ty);
        }
        for ty in &types {
            assert_eq!(nodes.node(*ty), ty);
        }
        let composite = Ty::Array(arena.alloc(ArrayTy {
            elem: *nodes.node(INT),
            len: Some(7),
        }));
        assert_eq!(*nodes.node(composite), composite);
    }
}
