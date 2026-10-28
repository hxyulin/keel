//! C type identities and explicit compilation-local storage queries.
//! Native targets use LP64; the bounded freestanding Wasm target uses ILP32.

use bumpalo::Bump;
use keel_ir::{Layout, Type, VectorType};

/// Validated C storage models, independent of host and IR/backend selection.
/// This controls long/pointer storage only, not char, extended FP or call ABI.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct CDataLayout {
    long_bytes: u64,
    pointer_bytes: u64,
}

impl CDataLayout {
    pub(crate) const NATIVE_LP64: Self = Self {
        long_bytes: 8,
        pointer_bytes: 8,
    };
    pub(crate) const WASM32: Self = Self {
        long_bytes: 4,
        pointer_bytes: 4,
    };

    pub(crate) const fn pointer_bytes(self) -> u64 {
        self.pointer_bytes
    }
    pub(crate) const fn pointer_align(self) -> u64 {
        self.pointer_bytes
    }
}

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
    /// Distinct plain char on targets where it is unsigned.
    UPlainChar,
}

impl IntKind {
    /// Compatibility query for native LP64, independent of any `Types` instance.
    pub fn size(self) -> u64 {
        self.size_with_layout(CDataLayout::NATIVE_LP64)
    }

    pub fn size_with_layout(self, layout: CDataLayout) -> u64 {
        match self {
            IntKind::Bool
            | IntKind::Char
            | IntKind::SChar
            | IntKind::UChar
            | IntKind::UPlainChar => 1,
            IntKind::Short | IntKind::UShort => 2,
            IntKind::Int | IntKind::UInt => 4,
            IntKind::Long | IntKind::ULong => layout.long_bytes,
            IntKind::LLong | IntKind::ULLong => 8,
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
            IntKind::Char | IntKind::SChar | IntKind::UChar | IntKind::UPlainChar => 1,
            IntKind::Short | IntKind::UShort => 2,
            IntKind::Int | IntKind::UInt => 3,
            IntKind::Long | IntKind::ULong => 4,
            IntKind::LLong | IntKind::ULLong => 5,
            IntKind::Int128 | IntKind::UInt128 => 6,
        }
    }

    pub fn to_unsigned(self) -> IntKind {
        match self {
            IntKind::Char | IntKind::SChar | IntKind::UPlainChar => IntKind::UChar,
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
    /// C complex pair, real first, with the component's alignment.
    Complex(&'a Ty<'a>),
    /// GNU fixed vector: C lane identity and the supported IR shape.
    Vector(&'a Ty<'a>, VectorType),
    /// `double` and `long double`.
    Double,
    /// `_Float16`: only so headers declaring functions with it parse.
    Half,
    /// A type keel only knows the declaration layout of, so headers declaring functions
    /// and typedefs with it parse: `__float128`, extended `_Complex`, and
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
    pub align: u64,
}

// Canonical GNU binary128 identities; neither has a value/IR capability.
pub const BINARY128: Ty<'static> = Ty::Opaque(&OpaqueTy {
    name: "__float128",
    size: 16,
    align: 16,
});
pub const COMPLEX_BINARY128: Ty<'static> = Ty::Opaque(&OpaqueTy {
    name: "_Complex __float128",
    size: 32,
    align: 16,
});

/// Element type and length. An absent length is incomplete unless `vla`
/// identifies a captured runtime element count.
#[derive(PartialEq, Debug)]
pub struct ArrayTy<'a> {
    pub elem: Ty<'a>,
    pub len: Option<u64>,
    /// Captured runtime element count, in a hidden function local.
    pub vla: Option<usize>,
}

/// Which parameter contract a function declaration or definition carries.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FunctionForm {
    /// An `f()` declaration with no parameter information.
    Unspecified,
    /// A prototype, including `f(void)`.
    Prototype,
    /// An identifier-list or empty old-style definition; parameters are promoted ABI types.
    OldStyleDefinition,
}

#[derive(PartialEq, Debug)]
pub struct FuncTy<'a> {
    /// Foreign ABI type metadata only; evaluated calls and definitions reject.
    pub microsoft_x64: bool,
    pub ret: Ty<'a>,
    pub params: &'a [Ty<'a>],
    /// Qualifiers below each parameter's top level, in pointer/array order.
    /// Top-level parameter qualifiers do not participate in compatibility.
    pub param_quals: &'a [&'a [u8]],
    pub variadic: bool,
    pub form: FunctionForm,
}

impl FuncTy<'_> {
    /// Calls convert fixed arguments to parameter types instead of default promotions.
    pub fn has_prototype(&self) -> bool {
        self.form == FunctionForm::Prototype
    }

    /// Compare parameter contracts without changing the parser's separate
    /// policy for unrelated return-type redeclarations.
    pub(crate) fn parameters_compatible(&self, other: &Self) -> bool {
        let (a, b) = (self, other);
        if a.microsoft_x64 != b.microsoft_x64 {
            return false;
        }
        if a.form != FunctionForm::Unspecified && b.form != FunctionForm::Unspecified {
            a.variadic == b.variadic
                && a.params.len() == b.params.len()
                && a.params.iter().zip(b.params).all(|(a, b)| a.compatible(b))
                && a.param_quals == b.param_quals
        } else {
            let known = if a.has_prototype() { a } else { b };
            !known.has_prototype()
                || (!known.variadic
                    && known
                        .params
                        .iter()
                        .all(|t| t.compatible(&t.default_promoted())))
        }
    }
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

    pub fn is_const(&self, defs: &[TypedefDef<'_>]) -> bool {
        match self {
            Self::Const(_) => true,
            Self::Volatile(inner) | Self::Restrict(inner) | Self::Array(inner) => {
                inner.is_const(defs)
            }
            Self::Typedef(i) => defs[*i].sugar.is_const(defs),
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

    /// Array typedef qualifiers belong to their elements. Preserve const
    /// during parser decay/indexing as well as volatile during memory lowering.
    pub fn array_element(&self, defs: &[TypedefDef<'a>], arena: &'a Bump) -> Sugar<'a> {
        let mut target = self.pointee(defs);
        if self.is_volatile(defs) {
            target = Self::Volatile(arena.alloc(target));
        }
        if self.is_const(defs) {
            Self::Const(arena.alloc(target))
        } else {
            target
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
        let constant = self.is_const(defs) || other.is_const(defs);
        let volatile = self.is_volatile(defs) || other.is_volatile(defs);
        let merged = match (self.unqualified(defs), other.unqualified(defs)) {
            (Self::Ptr(a), Self::Ptr(b)) => Self::Ptr(arena.alloc(a.merge(b, defs, arena))),
            (Self::Array(a), Self::Array(b)) => Self::Array(arena.alloc(a.merge(b, defs, arena))),
            (Self::Func(a), Self::Func(b)) => Self::Func(arena.alloc(a.merge(b, defs, arena))),
            (Self::Plain, b) => *b,
            (a, _) => *a,
        };
        let merged = if volatile {
            Self::Volatile(arena.alloc(merged))
        } else {
            merged
        };
        if constant {
            Self::Const(arena.alloc(merged))
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
    /// The selected integer storage representation.
    pub ty: Ty<'a>,
    pub values: Vec<(&'a str, i64)>,
}

#[derive(Clone, Debug)]
pub struct StructDef<'a> {
    /// Pointer-only GNU transparent union, passed using its first member ABI.
    pub transparent_union: bool,
    pub tag: Option<&'a str>,
    pub is_union: bool,
    pub members: Vec<Member<'a>>,
    pub size: u64,
    pub align: u64,
    pub complete: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct Member<'a> {
    pub nonstring: bool,
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
/// of its declared type at the member's offset, or a bounded byte span when packed.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct BitField {
    pub shift: u32,
    pub width: u32,
    /// Access only the bytes containing the field, rather than its declared unit.
    pub bytewise: bool,
}

pub const INT: Ty<'static> = Ty::Int(IntKind::Int);
pub const UINT: Ty<'static> = Ty::Int(IntKind::UInt);
pub const LONG: Ty<'static> = Ty::Int(IntKind::Long);
pub const ULONG: Ty<'static> = Ty::Int(IntKind::ULong);
pub const CHAR: Ty<'static> = Ty::Int(IntKind::Char);

/// Scalar nodes, shared by every compilation.
static SCALAR_NODES: [Ty<'static>; IntKind::UPlainChar as usize + 5] = [
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
    Ty::Int(IntKind::UPlainChar),
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
            Ty::Float => &SCALAR_NODES[IntKind::UPlainChar as usize + 2],
            Ty::Double => &SCALAR_NODES[IntKind::UPlainChar as usize + 3],
            Ty::Half => &SCALAR_NODES[IntKind::UPlainChar as usize + 4],
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
    /// Whether this type depends on a captured runtime array bound.
    pub fn variably_modified(&self) -> bool {
        match self {
            Ty::Array(a) => a.vla.is_some() || a.elem.variably_modified(),
            Ty::Ptr(t) | Ty::Atomic(t) => t.variably_modified(),
            Ty::Func(f) => f.ret.variably_modified() || f.params.iter().any(Ty::variably_modified),
            _ => false,
        }
    }

    /// Check numeric type identity without broadening the frontend's existing
    /// policy for unrelated pointer/scalar and old-style redeclarations.
    pub fn numeric_redeclaration_conflict(&self, other: &Self) -> bool {
        match (self, other) {
            (Ty::Ptr(a), Ty::Ptr(b)) | (Ty::Atomic(a), Ty::Atomic(b)) => {
                a.numeric_redeclaration_conflict(b)
            }
            (Ty::Array(a), Ty::Array(b)) => a.elem.numeric_redeclaration_conflict(&b.elem),
            (Ty::Func(a), Ty::Func(b)) => {
                a.ret.numeric_redeclaration_conflict(&b.ret)
                    || (a.has_prototype()
                        && b.has_prototype()
                        && a.params
                            .iter()
                            .zip(b.params)
                            .any(|(a, b)| a.numeric_redeclaration_conflict(b)))
            }
            (Ty::Vector(..), _) | (_, Ty::Vector(..)) => !self.compatible(other),
            (a, b) if a.numeric_identity() && b.numeric_identity() => !a.compatible(b),
            _ => false,
        }
    }

    // Value support and numeric identity are separate: opaque extended floats
    // still must not acquire a different numeric type through a redeclaration.
    fn numeric_identity(&self) -> bool {
        self.is_arith()
            || matches!(self, Ty::Opaque(o) if matches!(o.name,
                "__float128" | "_Complex __float128" | "long double" | "_Complex long double"))
    }

    /// Compatibility for redeclarations, permitting incomplete arrays/prototypes.
    pub fn compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Ty::Ptr(a), Ty::Ptr(b)) | (Ty::Atomic(a), Ty::Atomic(b)) => a.compatible(b),
            (Ty::Array(a), Ty::Array(b)) => {
                a.elem.compatible(&b.elem) && (a.len.is_none() || b.len.is_none() || a.len == b.len)
            }
            (Ty::Func(a), Ty::Func(b)) => {
                if !a.ret.compatible(&b.ret) {
                    return false;
                }
                a.parameters_compatible(b)
            }
            _ => self == other,
        }
    }

    /// Type used at an unprototyped call or old-style definition boundary.
    pub(crate) fn default_promoted(&self) -> Self {
        if *self == Ty::Float {
            Ty::Double
        } else {
            crate::expr::promoted(self)
        }
    }

    /// An array object whose storage size depends on runtime bounds.
    /// Pointers to VM types still have a fixed object size.
    pub fn runtime_sized(&self) -> bool {
        matches!(self, Ty::Array(a) if a.vla.is_some() || a.elem.runtime_sized())
    }

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

    pub fn is_vector(&self) -> bool {
        matches!(self, Ty::Vector(..))
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Ty::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Ty::Float | Ty::Double | Ty::Half)
    }

    pub fn is_complex(&self) -> bool {
        matches!(self, Ty::Complex(_))
    }

    /// Types represented by an address and classified through an ABI layout.
    pub fn is_pair_or_struct(&self) -> bool {
        self.is_complex() || self.is_struct()
    }

    pub fn is_arith(&self) -> bool {
        self.is_integer() || self.is_float() || self.is_complex()
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

    /// Compatibility query for native LP64, independent of any `Types` instance.
    pub fn ir(&self) -> Type {
        self.ir_with_layout(CDataLayout::NATIVE_LP64)
    }

    pub fn ir_with_layout(&self, layout: CDataLayout) -> Type {
        match self {
            Ty::Int(k) => match k.size_with_layout(layout) {
                1 => Type::I8,
                2 => Type::I16,
                4 => Type::I32,
                8 => Type::I64,
                _ => Type::I128,
            },
            Ty::Vector(_, shape) => Type::Vector(*shape),
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
pub struct Types<'a> {
    structs: &'a [StructDef<'a>],
    layout: CDataLayout,
}

impl<'a> Types<'a> {
    /// Completed records in `structs` must have been laid out with `layout`.
    /// Their stored sizes/offsets are authoritative; this does not relayout them.
    pub fn new(structs: &'a [StructDef<'a>], layout: CDataLayout) -> Self {
        Self { structs, layout }
    }
    /// Size queries after completed-type validation cannot overflow.
    pub fn size(&self, ty: &Ty) -> u64 {
        self.checked_size(ty)
            .expect("completed type size was validated")
    }

    /// Storage size before a completed array or record enters the type table.
    pub fn checked_size(&self, ty: &Ty) -> Result<u64, &'static str> {
        Ok(match ty {
            Ty::Void => 1,
            Ty::Int(k) => k.size_with_layout(self.layout),
            Ty::Vector(_, shape) => u64::from(shape.bytes()),
            Ty::Complex(t) => self
                .checked_size(t)?
                .checked_mul(2)
                .ok_or("type size overflow")?,
            Ty::Float => 4,
            Ty::Double => 8,
            Ty::Ptr(_) => self.layout.pointer_bytes(),
            Ty::Half => 2,
            Ty::Opaque(o) => o.size,
            Ty::Array(a) => self
                .checked_size(&a.elem)?
                .checked_mul(a.len.unwrap_or(0))
                .ok_or("type size overflow")?,
            Ty::Func(_) => 1,
            Ty::Struct(id) => self.structs[*id].size,
            Ty::Atomic(t) => self.checked_size(t)?,
        })
    }

    /// Native atomic representations shared by the two supported targets.
    /// The pointer's declared/proven alignment must cover the whole object.
    pub fn atomic_size(&self, ty: &Ty, align: u64) -> Option<u8> {
        if self.layout != CDataLayout::NATIVE_LP64 {
            return None;
        }
        let size = self.size(ty);
        if matches!(ty, Ty::Void | Ty::Func(_) | Ty::Array(_))
            || !matches!(size, 1 | 2 | 4 | 8)
            || align < size
        {
            None
        } else {
            Some(size as u8)
        }
    }

    pub fn align(&self, ty: &Ty) -> u64 {
        match ty {
            Ty::Ptr(_) => self.layout.pointer_align(),
            Ty::Complex(t) => self.align(t),
            Ty::Opaque(o) => o.align,
            Ty::Array(a) => self.align(&a.elem),
            Ty::Struct(id) => self.structs[*id].align,
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
    pub fn abi_layout(&self, ty: &Ty) -> Result<Layout, &'static str> {
        assert_eq!(
            self.layout,
            CDataLayout::NATIVE_LP64,
            "native ABI layout only"
        );
        self.abi_layout_with_alignment(ty, self.align(ty))
    }

    /// AAPCS64 ignores an aggregate's top-level over-alignment when
    /// allocating argument registers, while retaining member alignment.
    pub fn aapcs_layout(&self, ty: &Ty) -> Result<Layout, &'static str> {
        assert_eq!(
            self.layout,
            CDataLayout::NATIVE_LP64,
            "native ABI layout only"
        );
        let alignment = match ty {
            Ty::Struct(id) => self.structs[*id]
                .members
                .iter()
                .map(|m| m.align)
                .max()
                .unwrap_or(1),
            _ => self.align(ty),
        };
        self.abi_layout_with_alignment(ty, alignment)
    }

    fn abi_layout_with_alignment(&self, ty: &Ty, align: u64) -> Result<Layout, &'static str> {
        if let Some((float, n)) = self.hfa(ty)
            && n <= 4
        {
            return Ok(Layout::Array(Box::new(Layout::Scalar(float)), n as u32));
        }
        let unit = match align {
            1 => Type::I8,
            2 => Type::I16,
            4 => Type::I32,
            8 => Type::I64,
            _ => Type::I128,
        };
        let n = self.checked_size(ty)? / align;
        let n = u32::try_from(n)
            .map_err(|_| "aggregate ABI layout element count exceeds the IR limit")?;
        Ok(Layout::Array(Box::new(Layout::Scalar(unit)), n))
    }

    /// SysV classifies aggregates with unaligned scalar fields as MEMORY.
    /// The current IR layouts cannot express those explicit field offsets.
    pub fn has_unaligned_fields(&self, ty: &Ty, base: u64) -> bool {
        match ty {
            Ty::Struct(id) => self.structs[*id]
                .members
                .iter()
                .any(|m| m.bits.is_none() && self.has_unaligned_fields(&m.ty, base + m.offset)),
            Ty::Array(a) => self.has_unaligned_fields(&a.elem, base),
            _ => !base.is_multiple_of(self.align(ty)),
        }
    }

    /// Unsupported values contained in an object, stopping at pointers and
    /// function types. Declarations/layout queries do not materialize values.
    pub fn unsupported_value(&self, ty: &Ty) -> Option<&'static str> {
        fn find(types: &Types<'_>, ty: &Ty, seen: &mut Vec<usize>) -> Option<&'static str> {
            if let Some(name) = ty.unusable() {
                return Some(name);
            }
            match ty {
                Ty::Struct(id) if !seen.contains(id) => {
                    seen.push(*id);
                    types.structs[*id]
                        .members
                        .iter()
                        .find_map(|m| find(types, &m.ty, seen))
                }
                Ty::Array(a) => find(types, &a.elem, seen),
                Ty::Atomic(t) | Ty::Complex(t) => find(types, t, seen),
                _ => None,
            }
        }
        find(self, ty, &mut Vec::new())
    }

    pub fn contains_vector_union(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Struct(id) => {
                let def = &self.structs[*id];
                (def.is_union && self.contains_vector(ty))
                    || def
                        .members
                        .iter()
                        .any(|m| self.contains_vector_union(&m.ty))
            }
            Ty::Array(a) => self.contains_vector_union(&a.elem),
            _ => false,
        }
    }

    pub fn contains_vector(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Vector(..) => true,
            Ty::Struct(id) => self.structs[*id]
                .members
                .iter()
                .any(|m| self.contains_vector(&m.ty)),
            Ty::Array(a) => self.contains_vector(&a.elem),
            _ => false,
        }
    }

    pub fn contains_complex(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Complex(_) => true,
            Ty::Opaque(o) => o.name == "_Complex long double",
            Ty::Struct(id) => self.structs[*id]
                .members
                .iter()
                .any(|m| self.contains_complex(&m.ty)),
            Ty::Array(a) => self.contains_complex(&a.elem),
            _ => false,
        }
    }

    /// Exact natural layout for complex-containing call types. In particular,
    /// SysV merges SSE/INTEGER classes per eightbyte, not per entire struct.
    /// Fail closed for unions, bit-fields and explicit/packed layout changes.
    pub fn complex_abi_layout(&self, ty: &Ty) -> Option<Layout> {
        if self.layout != CDataLayout::NATIVE_LP64 {
            return None;
        }
        let layout = match ty {
            Ty::Atomic(elem) => self.complex_abi_layout(elem)?,
            Ty::Complex(elem) => Layout::Array(Box::new(Layout::Scalar(elem.ir())), 2),
            Ty::Struct(id) => {
                let def = &self.structs[*id];
                if def.is_union {
                    return None;
                }
                let mut fields = Vec::new();
                let mut offset: u64 = 0;
                for m in &def.members {
                    if m.bits.is_some() {
                        return None;
                    }
                    let field = self.complex_abi_layout(&m.ty)?;
                    offset = offset.next_multiple_of(field.align());
                    if m.offset != offset {
                        return None;
                    }
                    offset += field.size();
                    fields.push(field);
                }
                Layout::Struct(fields)
            }
            Ty::Array(a) => Layout::Array(
                Box::new(self.complex_abi_layout(&a.elem)?),
                a.len?.try_into().ok()?,
            ),
            Ty::Int(_) | Ty::Ptr(_) | Ty::Float | Ty::Double | Ty::Vector(..) => {
                Layout::Scalar(ty.ir())
            }
            _ => return None,
        };
        (layout.size() == self.size(ty) && layout.align() == self.align(ty)).then_some(layout)
    }

    pub fn contains_atomic(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Atomic(_) => true,
            Ty::Struct(id) => self.structs[*id]
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
            Ty::Vector(_, shape) => Some((Type::Vector(*shape), 1)),
            Ty::Complex(t) => self.hfa(t).map(|(t, _)| (t, 2)),
            Ty::Float => Some((Type::F32, 1)),
            Ty::Double => Some((Type::F64, 1)),
            Ty::Array(ArrayTy {
                elem, len: Some(n), ..
            }) => self
                .hfa(elem)
                .and_then(|(t, k)| Some((t, k.checked_mul(*n)?))),
            Ty::Struct(id) => {
                let def = &self.structs[*id];
                if def.is_union {
                    return None;
                }
                let mut float = None;
                let mut count: u64 = 0;
                for m in &def.members {
                    if m.bits.is_some() {
                        return None;
                    }
                    let (t, k) = self.hfa(&m.ty)?;
                    if float.is_some_and(|f: Type| {
                        f != t && !(f.is_vector() && t.is_vector() && f.bytes() == t.bytes())
                    }) {
                        return None;
                    }
                    float = Some(t);
                    count = count.checked_add(k)?;
                }
                // No padding: the floats must be all there is.
                let float = float?;
                (count.checked_mul(u64::from(float.bytes().unwrap()))? == def.size)
                    .then_some((float, count))
            }
            _ => None,
        }
    }
}

pub(crate) fn stack_storage_size(size: u64, padding: u64) -> Result<u32, &'static str> {
    size.max(1)
        .checked_add(padding)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("object exceeds the IR stack-slot size limit (including alignment padding)")
}

pub type StructField<'a> = (
    Option<&'a str>,
    Ty<'a>,
    Sugar<'a>,
    Option<u32>,
    Option<u64>,
    bool,
    bool,
);

/// Lays out a struct or union's members, including alignment and packing,
/// returning the members with their offsets, the size and the alignment.
/// `max_align` caps member alignment for `#pragma pack(n)`.
/// Ordinary bit-fields stay within a declared-type container. Packed fields
/// may cross containers; zero-width fields align the next field to their type.
pub fn layout_struct<'a>(
    types: &Types,
    typedefs: &[TypedefDef<'a>],
    is_union: bool,
    fields: Vec<StructField<'a>>,
    max_align: Option<u64>,
) -> Result<(Vec<Member<'a>>, u64, u64), &'static str> {
    if types.layout != CDataLayout::NATIVE_LP64
        && (max_align.is_some() || fields.iter().any(|f| f.3.is_some() || f.5))
    {
        return Err("non-native packed records and bit-fields are not supported");
    }
    let mut members = Vec::new();
    // A representable u64 byte size can require more than u64 bits.
    let mut bit: u128 = 0;
    let mut size: u64 = 0;
    let mut align: u64 = 1;
    for (name, ty, sugar, width, requested_align, packed, nonstring) in fields {
        let natural = sugar
            .alignment(typedefs)
            .unwrap_or_else(|| types.align(&ty));
        // Explicit GNU alignment overrides packed layout, but pragma pack
        // caps even an explicitly aligned member.
        let falign = if packed { 1 } else { natural }
            .max(requested_align.unwrap_or(1))
            .min(max_align.unwrap_or(u64::MAX));
        let fsize = if types.layout != CDataLayout::NATIVE_LP64 {
            if falign > u32::MAX as u64 {
                return Err("non-native record alignment exceeds wasm32 bounds");
            }
            let size = types.checked_size(&ty)?;
            if size > u32::MAX as u64 {
                return Err("non-native record member storage exceeds wasm32 bounds");
            }
            size
        } else {
            types.checked_size(&ty)?
        };
        if is_union {
            bit = 0;
        }
        match width {
            Some(0) => {
                bit = bit.next_multiple_of(u128::from(natural) * 8);
            }
            Some(w) => {
                if requested_align.is_some() {
                    bit = bit.next_multiple_of(u128::from(falign) * 8);
                }
                let unit_bits = u128::from(fsize) * 8;
                let bytewise = packed || max_align.is_some();
                if !bytewise && bit / unit_bits != (bit + u128::from(w) - 1) / unit_bits {
                    bit = bit.next_multiple_of(unit_bits);
                }
                let offset = if bytewise {
                    bit / 8
                } else {
                    bit / unit_bits * u128::from(fsize)
                };
                members.push(Member {
                    nonstring,
                    name,
                    ty,
                    sugar,
                    offset: u64::try_from(offset).map_err(|_| "record size overflow")?,
                    align: falign,
                    bits: Some(BitField {
                        shift: (bit - offset * 8) as u32,
                        width: w,
                        bytewise,
                    }),
                });
                bit += u128::from(w);
                align = align.max(falign);
            }
            None => {
                bit = bit.next_multiple_of(u128::from(falign) * 8);
                members.push(Member {
                    nonstring,
                    name,
                    ty,
                    sugar,
                    offset: u64::try_from(bit / 8).map_err(|_| "record size overflow")?,
                    align: falign,
                    bits: None,
                });
                bit += u128::from(fsize) * 8;
                align = align.max(falign);
            }
        }
        size = size.max(u64::try_from(bit.div_ceil(8)).map_err(|_| "record size overflow")?);
        if types.layout != CDataLayout::NATIVE_LP64 && size > u32::MAX as u64 {
            return Err("non-native record storage exceeds wasm32 bounds");
        }
    }
    let size = size
        .checked_next_multiple_of(align)
        .ok_or("record size overflow")?;
    if types.layout != CDataLayout::NATIVE_LP64 && size > u32::MAX as u64 {
        return Err("non-native record storage exceeds wasm32 bounds");
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
            Ty::Int(IntKind::UPlainChar),
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
            vla: None,
        }));
        assert_eq!(*nodes.node(composite), composite);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    const MODELS: [CDataLayout; 2] = [CDataLayout::NATIVE_LP64, CDataLayout::WASM32];

    #[test]
    fn integer_storage_preserves_nominal_identity_and_rank() {
        use IntKind::*;
        let kinds = [
            Bool, Char, SChar, UChar, UPlainChar, Short, UShort, Int, UInt, Long, ULong, LLong,
            ULLong, Int128, UInt128,
        ];
        for layout in MODELS {
            let types = Types::new(&[], layout);
            for k in kinds {
                let ty = Ty::Int(k);
                let expected = match k {
                    Bool | Char | SChar | UChar | UPlainChar => 1,
                    Short | UShort => 2,
                    Int | UInt => 4,
                    Long | ULong => layout.long_bytes,
                    LLong | ULLong => 8,
                    Int128 | UInt128 => 16,
                };
                assert_eq!(k.size_with_layout(layout), expected);
                assert_eq!(types.size(&ty), expected);
                assert_eq!(types.align(&ty), expected);
                assert_eq!(
                    u64::from(ty.ir_with_layout(layout).bytes().unwrap()),
                    expected
                );
                assert_eq!(k.size(), k.size_with_layout(CDataLayout::NATIVE_LP64));
                assert_eq!(ty.ir(), ty.ir_with_layout(CDataLayout::NATIVE_LP64));
                assert_eq!(k.to_unsigned().size_with_layout(layout), expected);
            }
            assert!(Int.rank() < Long.rank() && Long.rank() < LLong.rank());
            assert!(!LONG.compatible(&Ty::Int(LLong)));
            assert!(!INT.compatible(&LONG));
        }
        assert_eq!(LONG.ir_with_layout(CDataLayout::WASM32), Type::I32);
        assert_eq!(
            Ty::Int(LLong).ir_with_layout(CDataLayout::WASM32),
            Type::I64
        );
    }

    fn field<'a>(ty: Ty<'a>) -> StructField<'a> {
        (None, ty, Sugar::Plain, None, None, false, false)
    }

    #[test]
    fn pointer_record_union_and_nested_array_storage() {
        let ptr = Ty::Ptr(&CHAR);
        for layout in MODELS {
            let types = Types::new(&[], layout);
            let p = layout.pointer_bytes();
            assert_eq!(types.size(&ptr), p);
            assert_eq!(types.align(&ptr), layout.pointer_align());
            // IR pointer identity is independent of its storage representation.
            assert_eq!(ptr.ir_with_layout(layout), Type::Ptr);
            let (members, size, align) = layout_struct(
                &types,
                &[],
                false,
                vec![field(CHAR), field(ptr), field(LONG)],
                None,
            )
            .unwrap();
            assert_eq!(
                members.iter().map(|m| m.offset).collect::<Vec<_>>(),
                vec![0, p, 2 * p]
            );
            assert_eq!((size, align), (3 * p, p));
            let record = StructDef {
                transparent_union: false,
                tag: None,
                is_union: false,
                members,
                size,
                align,
                complete: true,
            };
            let defs = [record];
            let types = Types::new(&defs, layout);
            let array = Ty::Array(&ArrayTy {
                elem: Ty::Struct(0),
                len: Some(3),
                vla: None,
            });
            let nested = Ty::Array(&ArrayTy {
                elem: array,
                len: Some(2),
                vla: None,
            });
            assert_eq!((types.size(&nested), types.align(&nested)), (18 * p, p));
            let (_, union_size, union_align) = layout_struct(
                &types,
                &[],
                true,
                vec![field(ptr), field(Ty::Int(IntKind::LLong))],
                None,
            )
            .unwrap();
            assert_eq!((union_size, union_align), (8, 8));
            let (_, outer_size, outer_align) =
                layout_struct(&types, &[], false, vec![field(CHAR), field(nested)], None).unwrap();
            assert_eq!((outer_size, outer_align), (19 * p, p));
            assert_eq!(types.size(&Ty::Atomic(&LONG)), layout.long_bytes);
        }
    }

    #[test]
    fn interleaved_models_do_not_change_shared_type_nodes() {
        let arena = Bump::new();
        let mut nodes = TypeNodes::new(&arena);
        let long = nodes.node(LONG);
        let ptr = nodes.node(Ty::Ptr(long));
        let array = Ty::Array(arena.alloc(ArrayTy {
            elem: Ty::Atomic(ptr),
            len: Some(3),
            vla: None,
        }));
        let native = Types::new(&[], CDataLayout::NATIVE_LP64);
        let wasm = Types::new(&[], CDataLayout::WASM32);
        // Reusing the same nodes must not cache either model's widths globally.
        for (types, bytes) in [(&native, 8), (&wasm, 4), (&native, 8), (&wasm, 4)] {
            assert_eq!(
                (types.size(&array), types.align(&array)),
                (3 * bytes, bytes)
            );
            assert_eq!(types.size(long), bytes);
            assert_eq!(ptr.ir_with_layout(types.layout), Type::Ptr);
            assert_eq!(long.ir(), Type::I64);
            assert_eq!(IntKind::Long.size(), 8);
        }
        assert!(!long.compatible(&Ty::Int(IntKind::LLong)));
    }

    #[test]
    fn aligned_record_keeps_model_stride_and_stops_at_recursive_pointer() {
        let longs = Ty::Array(&ArrayTy {
            elem: LONG,
            len: Some(2),
            vla: None,
        });
        let self_pointer = Ty::Ptr(&Ty::Struct(0));
        for (layout, offsets, expected) in [
            (CDataLayout::NATIVE_LP64, vec![0, 16, 32], (48, 16)),
            (CDataLayout::WASM32, vec![0, 16, 24], (32, 16)),
        ] {
            let mut aligned = field(longs);
            aligned.4 = Some(16);
            // The referenced record need not be complete to lay out its pointer.
            let (members, size, align) = layout_struct(
                &Types::new(&[], layout),
                &[],
                false,
                vec![field(CHAR), aligned, field(self_pointer)],
                None,
            )
            .unwrap();
            assert_eq!(
                members.iter().map(|m| m.offset).collect::<Vec<_>>(),
                offsets
            );
            assert_eq!((size, align), expected);
            let defs = [StructDef {
                transparent_union: false,
                tag: None,
                is_union: false,
                members,
                size,
                align,
                complete: true,
            }];
            let types = Types::new(&defs, layout);
            let records = Ty::Array(&ArrayTy {
                elem: Ty::Struct(0),
                len: Some(2),
                vla: None,
            });
            assert_eq!(
                (types.size(&records), types.align(&records)),
                (2 * size, 16)
            );
            assert!(!types.has_unaligned_fields(&records, 0));
            assert_eq!(types.unsupported_value(&Ty::Struct(0)), None);
        }
    }

    #[test]
    fn wasm_storage_does_not_authorize_native_abi_or_bitfields() {
        let types = Types::new(&[], CDataLayout::WASM32);
        assert_eq!(types.atomic_size(&INT, 4), None);
        assert_eq!(types.complex_abi_layout(&Ty::Complex(&Ty::Double)), None);
        let mut bitfield = field(INT);
        bitfield.3 = Some(3);
        assert!(layout_struct(&types, &[], false, vec![bitfield], None).is_err());
        let mut packed = field(INT);
        packed.5 = true;
        assert!(layout_struct(&types, &[], false, vec![packed], None).is_err());
        assert!(layout_struct(&types, &[], false, vec![field(INT)], Some(1)).is_err());
        // Native support remains available.
        let native = Types::new(&[], CDataLayout::NATIVE_LP64);
        assert!(layout_struct(&native, &[], false, vec![bitfield], None).is_ok());
        assert_eq!(native.atomic_size(&INT, 4), Some(4));
    }

    #[test]
    #[should_panic(expected = "native ABI layout only")]
    fn wasm_cannot_use_native_abi_classifier() {
        let _ = Types::new(&[], CDataLayout::WASM32).abi_layout(&LONG);
    }

    #[test]
    #[should_panic(expected = "native ABI layout only")]
    fn wasm_cannot_use_aapcs_classifier() {
        let _ = Types::new(&[], CDataLayout::WASM32).aapcs_layout(&LONG);
    }
}
