//! The C ABI: how C parameter and result types map to IR signatures.

use super::*;

impl<'p> Gen<'p> {
    /// How a value of C type `ty` crosses a call; the name of the type if
    /// keel cannot pass it.
    fn abi_param(&self, ty: &Ty) -> Result<AbiParam, &'static str> {
        if let Some(name) = self.types().unsupported_value(ty) {
            return Err(name);
        }
        if self.p.c_layout != CDataLayout::NATIVE_LP64 {
            // Private preflight admits only plain i32/i64 signatures.
            return Ok(AbiParam::new(ty.ir_with_layout(self.p.c_layout)));
        }
        Ok(match ty {
            Ty::Struct(_) | Ty::Complex(_) => AbiParam {
                ty: Type::Ptr,
                ext: ArgExt::None,
                purpose: Purpose::ByVal(self.aggregate_layout(ty)?),
            },
            Ty::Int(k) if k.size_with_layout(self.p.c_layout) < 4 => AbiParam {
                ty: ty.ir_with_layout(self.p.c_layout),
                ext: if k.signed() {
                    ArgExt::Sext
                } else {
                    ArgExt::Uext
                },
                purpose: Purpose::Normal,
            },
            _ => AbiParam::new(ty.ir_with_layout(self.p.c_layout)),
        })
    }

    pub(super) fn aggregate_layout(&self, ty: &Ty) -> Result<keel_ir::Layout, &'static str> {
        if let Some(name) = self.types().unsupported_value(ty) {
            return Err(name);
        }
        if self.types().size(ty) == 0 {
            return Err("zero-size aggregate ABI value");
        }
        if self.types().contains_vector_union(ty) {
            return Err("unions containing vectors passed by value");
        }
        if self.p.target == keel_ir::Target::X86_64LinuxGnu && self.types().contains_vector(ty) {
            return Err(
                "vector fields in by-value x86-64 aggregates require unsupported SSEUP classification",
            );
        }
        if self.types().align(ty) > 16 {
            return Err("aggregates aligned above 16 bytes passed by value");
        }
        if self.p.target == keel_ir::Target::X86_64LinuxGnu
            && self.types().has_unaligned_fields(ty, 0)
        {
            return Err("aggregates with unaligned fields passed by value on x86-64");
        }
        if self.types().contains_complex(ty) {
            return self.types().complex_abi_layout(ty).ok_or(
                "complex-containing by-value aggregate with union, bit-field or non-natural layout",
            );
        }
        if self.p.target == keel_ir::Target::Aarch64LinuxGnu {
            self.types().aapcs_layout(ty)
        } else if self.p.target == keel_ir::Target::X86_64LinuxGnu {
            self.types().sysv_layout(ty)
        } else {
            self.types().abi_layout(ty)
        }
    }

    /// Qualify omission independently of the IR's nonempty aggregate layouts.
    /// Initially only fieldless GNU records on x86-64 SysV are admitted.
    fn empty_abi_value(&self, ty: &Ty) -> bool {
        let Ty::Struct(id) = ty else { return false };
        let record = &self.p.structs[*id];
        self.p.target == keel_ir::Target::X86_64LinuxGnu
            && self.p.c_layout == CDataLayout::NATIVE_LP64
            && self.types().unsupported_value(ty).is_none()
            && record.complete
            && record.size == 0
            && record.align == 1
            && record.members.is_empty()
    }

    pub(super) fn abi_mapping(&self, fty: &FuncTy, g: usize, extra: &[Ty]) -> R<CAbiMapping> {
        let mut sig = Signature::new(&[], &[]);
        sig.params.reserve(fty.params.len() + extra.len() + 1);
        let result = if self.empty_abi_value(&fty.ret) {
            ResultMapping::Empty
        } else if fty.ret.is_pair_or_struct() {
            sig.params.push(AbiParam {
                ty: Type::Ptr,
                ext: ArgExt::None,
                purpose: Purpose::Sret(
                    self.aggregate_layout(&fty.ret)
                        .map_err(|t| self.unsupported(g, t))?,
                ),
            });
            ResultMapping::Indirect
        } else if fty.ret != Ty::Void {
            sig.returns.push(
                self.abi_param(&fty.ret)
                    .map_err(|t| self.unsupported(g, t))?,
            );
            ResultMapping::Direct
        } else {
            ResultMapping::Void
        };
        let mut params = Vec::with_capacity(fty.params.len() + extra.len());
        for ty in fty.params.iter().chain(extra) {
            if self.empty_abi_value(ty) {
                params.push(ParamMapping::Omitted);
            } else {
                params.push(ParamMapping::Operand(sig.params.len()));
                sig.params
                    .push(self.abi_param(ty).map_err(|t| self.unsupported(g, t))?);
            }
        }
        sig.variadic = fty.variadic;
        sig.returns_twice = g != usize::MAX && self.p.globals[g].returns_twice;
        Ok(CAbiMapping {
            signature: sig,
            params,
            result,
        })
    }

    pub(super) fn signature(&self, fty: &FuncTy, g: usize, extra: &[Ty]) -> R<Signature> {
        Ok(self.abi_mapping(fty, g, extra)?.signature)
    }

    /// The error for a call to or definition of function `g` (if known)
    /// that passes a value of type `name`.
    pub(super) fn unsupported(&self, g: usize, name: &str) -> Error {
        let what = match self.p.globals.get(g) {
            Some(global) => format!("`{}` passes a `{name}`", global.name),
            None => format!("a call passes a `{name}`"),
        };
        Error::new(Loc::default(), format!("{what}, which is not supported"))
    }
}
