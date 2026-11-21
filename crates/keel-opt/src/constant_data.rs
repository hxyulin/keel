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
        if !module.assembly.iter().all(|text| data_and_linkage(text))
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
                    || module.assembly.iter().any(|text| text.contains(&d.name))
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

/// Recognize ELF export records and symbol linkage. Keep the module-wide
/// barrier for code, macros and unknown sections or directives.
fn data_and_linkage(text: &str) -> bool {
    let mut in_data = false;
    for statement in text
        .split([';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(label) = statement.strip_suffix(':') {
            if !in_data || !asm_symbol(label) {
                return false;
            }
            continue;
        }
        let (directive, args) = statement
            .split_once(char::is_whitespace)
            .map_or((statement, ""), |(directive, args)| {
                (directive, args.trim())
            });
        match (in_data, directive) {
            (false, ".section")
                if args.split_once(',').is_some_and(|(name, flags)| {
                    name.trim() == "\".export_symbol\"" && flags.trim() == "\"a\""
                }) =>
            {
                in_data = true;
            }
            (true, ".previous") if args.is_empty() => in_data = false,
            (true, ".asciz")
                if args
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .is_some_and(|s| {
                        s.bytes().all(|c| {
                            c.is_ascii_graphic() && !matches!(c, b'"' | b'\\') || c == b' '
                        })
                    }) => {}
            (true, ".balign") if !args.is_empty() && args.bytes().all(|c| c.is_ascii_digit()) => {}
            (true, ".quad") | (false, ".weak" | ".globl") if asm_symbol(args) => {}
            (false, ".type")
                if args.split_once(',').is_some_and(|(name, ty)| {
                    asm_symbol(name.trim()) && ty.trim() == "@function"
                }) => {}
            (false, ".set")
                if args.split_once(',').is_some_and(|(name, target)| {
                    asm_symbol(name.trim()) && asm_symbol(target.trim())
                }) => {}
            _ => return false,
        }
    }
    !in_data
}

fn asm_symbol(text: &str) -> bool {
    text != "."
        && text
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, b'_' | b'.'))
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.'))
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
