//! Read-only declaration tables after parsing. Arena owners stay with the parser.
use crate::{
    ast::Global,
    parse::{Parser, StrLit},
    types::*,
};
use keel_ir::Target;

pub(crate) struct Unit<'a> {
    pub globals: &'a [Global<'a>],
    pub structs: &'a [StructDef<'a>],
    pub typedefs: &'a [TypedefDef<'a>],
    pub enums: &'a [EnumDef<'a>],
    pub strings: &'a [StrLit<'a>],
    pub assembly: &'a [String],
    pub target: Target,
    pub c_layout: CDataLayout,
}

impl<'a> Unit<'a> {
    pub fn new(p: &'a Parser<'a>) -> Self {
        Self {
            globals: &p.globals,
            structs: &p.structs,
            typedefs: &p.typedefs,
            enums: &p.enums,
            strings: &p.strings,
            assembly: &p.assembly,
            target: p.target,
            c_layout: p.c_layout,
        }
    }
    pub fn types(&self) -> Types<'_> {
        Types::new(self.structs, self.c_layout)
    }
    pub fn type_alignment(&self, ty: &Ty<'a>, sugar: &Sugar<'a>) -> u64 {
        sugar
            .alignment(self.typedefs)
            .unwrap_or_else(|| self.types().align(ty))
    }
}

const _: () = {
    fn assert_sync<T: Sync>() {}
    let _ = assert_sync::<Unit<'static>>;
};
