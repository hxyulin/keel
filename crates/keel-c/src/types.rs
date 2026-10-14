//! C types, with the sizes and alignments of Apple arm64 (LP64, signed
//! `char`, `long double` the same as `double`).

use keel_ir::{Layout, Type};
use std::rc::Rc;

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

#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    Void,
    Int(IntKind),
    Float,
    /// `double` and `long double`.
    Double,
    /// `_Float16`: only so headers declaring functions with it parse.
    Half,
    Ptr(Rc<Ty>),
    /// Element type and length; no length for `int a[]`.
    Array(Rc<Ty>, Option<u64>),
    Func(Rc<FuncTy>),
    /// A struct or union, by its index in the struct table.
    Struct(usize),
}

#[derive(Clone, PartialEq, Debug)]
pub struct FuncTy {
    pub ret: Ty,
    pub params: Vec<Ty>,
    pub variadic: bool,
    /// False for `int f()`, which says nothing about the parameters.
    pub prototyped: bool,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub tag: Option<String>,
    pub is_union: bool,
    pub members: Vec<Member>,
    pub size: u64,
    pub align: u64,
    pub complete: bool,
}

#[derive(Clone, Debug)]
pub struct Member {
    /// `None` for an anonymous struct or union, whose members are found
    /// as if they were this struct's, and for unnamed bit-fields.
    pub name: Option<String>,
    pub ty: Ty,
    pub offset: u64,
    pub bits: Option<BitField>,
}

/// Where a bit-field is: `width` bits from bit `shift` of the storage unit
/// of its declared type at the member's offset.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct BitField {
    pub shift: u32,
    pub width: u32,
}

pub const INT: Ty = Ty::Int(IntKind::Int);
pub const UINT: Ty = Ty::Int(IntKind::UInt);
pub const LONG: Ty = Ty::Int(IntKind::Long);
pub const ULONG: Ty = Ty::Int(IntKind::ULong);
pub const CHAR: Ty = Ty::Int(IntKind::Char);

impl Ty {
    pub fn ptr_to(ty: Ty) -> Ty {
        Ty::Ptr(Rc::new(ty))
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
    pub fn pointee(&self) -> Option<&Ty> {
        match self {
            Ty::Ptr(t) | Ty::Array(t, _) => Some(t),
            _ => None,
        }
    }

    pub fn func(&self) -> Option<&Rc<FuncTy>> {
        match self {
            Ty::Func(f) => Some(f),
            _ => None,
        }
    }

    /// The keel type of a scalar.
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

impl StructDef {
    /// The member called `name`, looking inside anonymous members; gives
    /// the path of members to it, and its offset.
    pub fn find(&self, name: &str, structs: &[StructDef]) -> Option<(u64, Member)> {
        for m in &self.members {
            match &m.name {
                Some(n) if n == name => return Some((m.offset, m.clone())),
                None if m.bits.is_none() => {
                    if let Ty::Struct(id) = m.ty
                        && let Some((off, found)) = structs[id].find(name, structs)
                    {
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
pub struct Types<'a>(pub &'a [StructDef]);

impl Types<'_> {
    pub fn size(&self, ty: &Ty) -> u64 {
        match ty {
            Ty::Void => 1,
            Ty::Int(k) => k.size(),
            Ty::Float => 4,
            Ty::Double | Ty::Ptr(_) => 8,
            Ty::Half => 2,
            Ty::Array(elem, n) => self.size(elem) * n.unwrap_or(0),
            Ty::Func(_) => 1,
            Ty::Struct(id) => self.0[*id].size,
        }
    }

    pub fn align(&self, ty: &Ty) -> u64 {
        match ty {
            Ty::Array(elem, _) => self.align(elem),
            Ty::Struct(id) => self.0[*id].align,
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

    /// The float type and count if `ty` is made only of floats of one type.
    fn hfa(&self, ty: &Ty) -> Option<(Type, u64)> {
        match ty {
            Ty::Float => Some((Type::F32, 1)),
            Ty::Double => Some((Type::F64, 1)),
            Ty::Array(elem, Some(n)) => self.hfa(elem).map(|(t, k)| (t, k * n)),
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

/// Lays out a struct or union's members (name, type, bit-field width),
/// returning the members with their offsets, the size and the alignment.
/// `max_align` caps member alignment (`packed` is 1, `#pragma pack(n)` n).
/// Bit-fields follow AAPCS64: a bit-field goes in the next bits that do not
/// cross a boundary of its declared type's size, and a zero-width one
/// aligns the next to its type.
pub fn layout_struct(
    types: &Types,
    is_union: bool,
    fields: Vec<(Option<String>, Ty, Option<u32>)>,
    max_align: Option<u64>,
) -> (Vec<Member>, u64, u64) {
    let mut members = Vec::new();
    let mut bit: u64 = 0;
    let mut size: u64 = 0;
    let mut align: u64 = 1;
    for (name, ty, width) in fields {
        let falign = types.align(&ty).min(max_align.unwrap_or(u64::MAX));
        let fsize = types.size(&ty);
        if is_union {
            bit = 0;
        }
        match width {
            Some(0) => {
                bit = bit.next_multiple_of(falign * 8);
            }
            Some(w) => {
                let unit_bits = fsize * 8;
                if bit / unit_bits != (bit + u64::from(w) - 1) / unit_bits {
                    bit = bit.next_multiple_of(unit_bits);
                }
                let offset = bit / unit_bits * fsize;
                members.push(Member {
                    name,
                    ty,
                    offset,
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
                    offset: bit / 8,
                    bits: None,
                });
                bit += fsize * 8;
                align = align.max(falign);
            }
        }
        size = size.max(bit.div_ceil(8));
    }
    (members, size.next_multiple_of(align), align)
}
