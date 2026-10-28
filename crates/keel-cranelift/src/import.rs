//! Verified CLIF import for the scalar, single-function diagnostic profile.
use crate::{Options, Result};
use cranelift_codegen::ir::{self, ArgumentExtension, ArgumentPurpose, InstructionData, Opcode};
use cranelift_codegen::isa::CallConv;

fn scalar(ty: ir::Type) -> Result<String> {
    match ty {
        ir::types::I8 | ir::types::I16 | ir::types::I32 | ir::types::I64 => Ok(ty.to_string()),
        _ => Err(format!("unsupported CLIF type {ty}")),
    }
}

fn abi(p: &ir::AbiParam) -> Result<String> {
    if p.purpose != ArgumentPurpose::Normal {
        return Err(format!("unsupported ABI purpose {:?}", p.purpose));
    }
    let ext = match p.extension {
        ArgumentExtension::None => "",
        ArgumentExtension::Uext => " uext",
        ArgumentExtension::Sext => " sext",
    };
    Ok(format!("{}{ext}", scalar(p.value_type)?))
}

/// Parse and verify exactly one CLIF function for the explicit Apple arm64 profile.
///
/// CLIF text has no module linkage contract. The caller supplies its exported
/// symbol. Calls, memory, data, stack slots and non-scalar values are rejected.
///
/// # Errors
/// Returns a diagnostic for invalid CLIF or anything outside the import profile.
pub fn parse(source: &str, options: &Options) -> Result<ir::Function> {
    let mut functions = cranelift_reader::parse_functions(source).map_err(|e| e.to_string())?;
    if functions.len() != 1 {
        return Err("CLIF import requires exactly one function".into());
    }
    let f = functions.remove(0);
    let isa = crate::build_isa(options)?;
    cranelift_codegen::verify_function(&f, isa.as_ref()).map_err(|e| e.to_string())?;
    if f.signature.call_conv != CallConv::AppleAarch64 {
        return Err("CLIF import requires apple_aarch64 calling convention".into());
    }
    if f.signature.returns.len() > 1 {
        return Err("multiple returns are outside the scalar import profile".into());
    }
    for p in f.signature.params.iter().chain(&f.signature.returns) {
        abi(p)?;
    }
    if !f.sized_stack_slots.is_empty()
        || !f.dynamic_stack_slots.is_empty()
        || !f.global_values.is_empty()
        || !f.dfg.ext_funcs.is_empty()
    {
        return Err(
            "stack slots, global values and calls are outside the scalar import profile".into(),
        );
    }
    Ok(f)
}

fn value(f: &ir::Function, v: ir::Value) -> String {
    f.dfg.resolve_aliases(v).to_string()
}

fn edge(f: &ir::Function, edge: ir::BlockCall) -> Result<String> {
    let args = edge
        .args(&f.dfg.value_lists)
        .map(|a| match a {
            ir::BlockArg::Value(v) => Ok(value(f, v)),
            _ => Err("exceptional edge argument is unsupported".to_string()),
        })
        .collect::<Result<Vec<_>>>()?;
    if args.is_empty() {
        Ok(edge.block(&f.dfg.value_lists).to_string())
    } else {
        Ok(format!(
            "{}({})",
            edge.block(&f.dfg.value_lists),
            args.join(", ")
        ))
    }
}

/// Import a verified scalar CLIF function into verified Keel IR.
///
/// # Errors
/// Returns a diagnostic with the instruction when conversion is unsupported.
pub fn to_keel(source: &str, name: &str, options: &Options) -> Result<String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("symbol must contain only ASCII letters, digits and underscores".into());
    }
    let f = parse(source, options)?;
    let params = f
        .signature
        .params
        .iter()
        .map(abi)
        .collect::<Result<Vec<_>>>()?;
    let ret = f.signature.returns.first().map(abi).transpose()?;
    let mut out = format!(
        "func @{name}({}){} {{\n",
        params.join(", "),
        ret.map_or(String::new(), |r| format!(" -> {r}"))
    );
    for block in f.layout.blocks() {
        let params = f
            .dfg
            .block_params(block)
            .iter()
            .map(|&v| Ok(format!("{v}: {}", scalar(f.dfg.value_type(v))?)))
            .collect::<Result<Vec<_>>>()?;
        if params.is_empty() {
            out.push_str(&format!("{block}:\n"));
        } else {
            out.push_str(&format!("{block}({}):\n", params.join(", ")));
        }
        for inst in f.layout.block_insts(block) {
            let data = &f.dfg.insts[inst];
            let op = data.opcode();
            let args = f
                .dfg
                .inst_args(inst)
                .iter()
                .map(|&v| value(&f, v))
                .collect::<Vec<_>>();
            let results = f.dfg.inst_results(inst);
            let prefix = if let Some(&v) = results.first() {
                scalar(f.dfg.value_type(v))?;
                format!("{v} = ")
            } else {
                String::new()
            };
            let ty = results
                .first()
                .map(|&v| f.dfg.value_type(v).to_string())
                .unwrap_or_default();
            let body = match (op, data) {
                (Opcode::Iconst, InstructionData::UnaryImm { imm, .. }) => {
                    format!("iconst.{ty} {}", i64::from(*imm))
                }
                (
                    Opcode::Iadd
                    | Opcode::Isub
                    | Opcode::Imul
                    | Opcode::Band
                    | Opcode::Bor
                    | Opcode::Bxor
                    | Opcode::Ishl
                    | Opcode::Ushr
                    | Opcode::Sshr
                    | Opcode::Rotl
                    | Opcode::Rotr,
                    _,
                ) => {
                    let mapped = match op {
                        Opcode::Band => "and".to_string(),
                        Opcode::Bor => "or".to_string(),
                        Opcode::Bxor => "xor".to_string(),
                        Opcode::Ishl => "shl".to_string(),
                        _ => op.to_string(),
                    };
                    format!("{mapped} {}", args.join(", "))
                }
                (Opcode::Icmp, InstructionData::IntCompare { cond, .. }) => {
                    let cc = match cond.to_string().as_str() {
                        "slt" => "slt",
                        "sle" => "sle",
                        "sgt" => "sgt",
                        "sge" => "sge",
                        "ult" => "ult",
                        "ule" => "ule",
                        "ugt" => "ugt",
                        "uge" => "uge",
                        "eq" => "eq",
                        "ne" => "ne",
                        _ => return Err("unsupported comparison".into()),
                    };
                    format!("icmp.{cc} {}", args.join(", "))
                }
                (Opcode::Select, _) => format!("select {}", args.join(", ")),
                (Opcode::Ireduce | Opcode::Uextend | Opcode::Sextend, _) => {
                    let mapped = match op {
                        Opcode::Ireduce => "trunc",
                        Opcode::Uextend => "uext",
                        _ => "sext",
                    };
                    format!("{mapped}.{ty} {}", args[0])
                }
                (Opcode::Jump, InstructionData::Jump { destination, .. }) => {
                    format!("jump {}", edge(&f, *destination)?)
                }
                (Opcode::Brif, InstructionData::Brif { blocks, .. }) => format!(
                    "brif {}, {}, {}",
                    args[0],
                    edge(&f, blocks[0])?,
                    edge(&f, blocks[1])?
                ),
                (Opcode::Return, _) => format!("return {}", args.join(", ")),
                _ => {
                    return Err(format!(
                        "function @{name}: {block}, {inst} ({op}): unsupported by CLIF importer"
                    ));
                }
            };
            out.push_str(&format!("    {prefix}{body}\n"));
        }
    }
    out.push_str("}\n");
    let module = keel_ir::parse(&out).map_err(|e| format!("imported Keel parse: {e}"))?;
    keel_ir::verify_module(&module).map_err(|e| format!("imported Keel verify: {e:?}"))?;
    Ok(out)
}
