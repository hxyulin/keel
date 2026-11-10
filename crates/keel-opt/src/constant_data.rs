//! Fold scalar reads of bounded local immutable initializers.
use crate::{ModuleContext, OptimizationContext, Pass};
use keel_ir::fxhash::FxHashMap;
use keel_ir::{BinaryOp, DataItem, Function, InstData, Module, Symbol, Type, Value, ValueDef};

pub struct ConstantData;
struct Object {
    align: u32,
    bytes: Vec<u8>,
}

impl Pass for ConstantData {
    fn name(&self) -> &'static str {
        "constant-data"
    }
    fn description(&self) -> &'static str {
        "fold scalar reads from local immutable initialized data"
    }
    fn run(&self, _: &mut Function, _: &OptimizationContext) -> bool {
        false
    }
    fn run_module(&self, module: &mut Module, context: &mut ModuleContext<'_>) -> Option<bool> {
        let Some(layout) = context.layout else {
            return Some(false);
        };
        if !module.assembly.is_empty()
            || module.functions.iter().any(|f| {
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::InlineAsm { .. }))
            })
        {
            return Some(false);
        }
        let objects: FxHashMap<_, _> = module
            .data
            .iter()
            .filter_map(|d| {
                let init = d.init.as_ref()?;
                if !d.local
                    || d.weak
                    || d.weak_odr
                    || d.thread_local
                    || d.section.is_some()
                    || !init.constant
                    || init.has_relocations()
                {
                    return None;
                }
                let size = init.checked_size_with_layout(layout)?;
                if size > 4096 {
                    return None;
                }
                let mut bytes = Vec::with_capacity(size as usize);
                for item in &init.items {
                    match item {
                        DataItem::Bytes(data) => bytes.extend_from_slice(data),
                        DataItem::Zero(n) => bytes.resize(bytes.len() + *n as usize, 0),
                        DataItem::Int { ty, value } => {
                            let n = ty.bytes()? as usize;
                            let mut data = [if *value < 0 { 255 } else { 0 }; 16];
                            data[..8].copy_from_slice(&value.to_le_bytes());
                            bytes.extend_from_slice(data.get(..n)?);
                        }
                        _ => return None,
                    }
                }
                Some((
                    d.name.clone(),
                    Object {
                        align: init.align,
                        bytes,
                    },
                ))
            })
            .collect();
        if objects.is_empty() {
            return Some(false);
        }
        let mut changed = false;
        for f in &mut module.functions {
            let mut dirty = false;
            if context.enabled(f)
                && !f.layout_insts().any(|(_, i)| match f.inst_data(i) {
                    InstData::X86RflagsRegion { .. } => true,
                    InstData::Intrinsic { id, .. } => {
                        matches!(
                            id,
                            keel_ir::Intrinsic::X86RflagsRead | keel_ir::Intrinsic::X86RflagsSwap
                        ) || id.x86_legacy_x87_image().is_some()
                            || id.x86_owned_x87_stack().is_some()
                            || id
                                .x86_string_port_io()
                                .is_some_and(|(_, _, _, owned)| owned)
                    }
                    _ => false,
                })
            {
                let plans: Vec<_> = f
                    .layout_insts()
                    .filter_map(|(_, i)| {
                        let InstData::Load { flags, ty, addr } = *f.inst_data(i) else {
                            return None;
                        };
                        if flags.volatile
                            || !matches!(
                                ty,
                                Type::I8
                                    | Type::I16
                                    | Type::I32
                                    | Type::I64
                                    | Type::F32
                                    | Type::F64
                            )
                        {
                            return None;
                        }
                        let (name, offset) = address(f, addr)?;
                        let object = objects.get(name)?;
                        let offset = usize::try_from(offset).ok()?;
                        if object.align < flags.align
                            || !offset.is_multiple_of(flags.align as usize)
                        {
                            return None;
                        }
                        let n = ty.bytes()? as usize;
                        let data = object.bytes.get(offset..offset.checked_add(n)?)?;
                        let mut bytes = [0; 8];
                        bytes[..n].copy_from_slice(data);
                        let bits = u64::from_le_bytes(bytes);
                        let data = if ty.is_float() {
                            InstData::Fconst { ty, bits }
                        } else {
                            InstData::Iconst {
                                ty,
                                imm: ty.canonical_imm(bits as i64),
                            }
                        };
                        Some((i, data))
                    })
                    .collect();
                for (i, data) in plans {
                    *f.inst_data_mut(i) = data;
                    f.clear_inst_facts(i);
                    dirty = true;
                }
            }
            context.changed.push(dirty);
            changed |= dirty;
        }
        Some(changed)
    }
}

fn address(f: &Function, mut value: Value) -> Option<(&str, i64)> {
    let mut offset = 0i64;
    for _ in 0..16 {
        let ValueDef::Result(i, _) = f.value_def(f.resolve(value)) else {
            return None;
        };
        match *f.inst_data(i) {
            InstData::SymbolAddr {
                symbol: symbol @ Symbol::Data(_),
            } => return Some((f.symbol_name(symbol), offset)),
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args,
            } => {
                let ValueDef::Result(k, _) = f.value_def(f.resolve(args[1])) else {
                    return None;
                };
                let InstData::Iconst { ty: Type::I64, imm } = *f.inst_data(k) else {
                    return None;
                };
                offset = offset.checked_add(imm)?;
                value = args[0];
            }
            _ => return None,
        }
    }
    None
}
