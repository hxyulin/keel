//! Exhaust the declared intrinsic signature/immediate space and record the
//! actual selected opcodes. This is an emission inventory, not a native value
//! oracle or a claim that every pinned baseline encoding is selected.
use keel_asm_ir::Item;
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use keel_ir::{NeonIntrinsic as N, Type, VectorType};
use std::{collections::BTreeSet, fs};
struct Signature {
    op: N,
    ty: Type,
    inputs: Vec<Type>,
    range: Option<std::ops::RangeInclusive<i64>>,
    cases: usize,
    opcodes: BTreeSet<String>,
}
#[test]
fn enumerate_every_declared_neon_signature_and_immediate() {
    let mut ir = String::new();
    let mut signatures = Vec::new();
    let mut owners = Vec::new();
    let types: Vec<_> = [
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
        Type::F32,
        Type::F64,
    ]
    .into_iter()
    .chain(
        VectorType::ALL_64
            .into_iter()
            .chain(VectorType::ALL_128)
            .map(Type::Vector),
    )
    .collect();
    for &op in N::ALL {
        for &ty in &types {
            let Some(inputs) = op.operand_types(ty) else {
                continue;
            };
            let range = op.immediate_range(ty);
            let counts: Vec<_> = range.clone().map_or(vec![None], |r| r.map(Some).collect());
            let index = signatures.len();
            signatures.push(Signature {
                op,
                ty,
                inputs: inputs.clone(),
                range,
                cases: counts.len(),
                opcodes: BTreeSet::new(),
            });
            for imm in counts {
                let id = owners.len();
                owners.push(index);
                ir += &format!(
                    "func @inventory_{id}(ptr,ptr,ptr,ptr,ptr,ptr,ptr) {{\nblock0(v0: ptr,v1: ptr,v2: ptr,v3: ptr,v4: ptr,v5: ptr,v6: ptr):\n"
                );
                let mut args = Vec::new();
                for (i, &input) in inputs.iter().enumerate() {
                    let n = 10 + i;
                    if i + 1 == inputs.len() && imm.is_some() {
                        ir += &format!(" v{n} = iconst.i32 {}\n", imm.unwrap());
                        args.push(format!("v{n}"));
                    } else if input == Type::Ptr {
                        args.push(format!("v{i}"));
                    } else {
                        ir += &format!(" v{n} = load.{input} align 1 v{i}\n");
                        args.push(format!("v{n}"));
                    }
                }
                let results = (0..op.result_count())
                    .map(|i| format!("v{}", 30 + i))
                    .collect::<Vec<_>>();
                ir += &format!(
                    " {}{op}.{ty}({})\n",
                    if results.is_empty() {
                        String::new()
                    } else {
                        format!("{} = ", results.join(","))
                    },
                    args.join(",")
                );
                for result in results {
                    ir += &format!(" store align 1 {result},v6\n");
                }
                ir += " return\n}\n";
            }
        }
    }
    let module = keel_ir::parse(&ir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Baseline,
        allocator: &AllocationTier::Full,
    };
    let program = keel_codegen::aarch64::compile_program_with_pipeline(
        &module,
        &keel_codegen::aarch64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    let mut owner = None;
    for item in &program.items {
        match item {
            Item::Label(name) if name.starts_with("_inventory_") => {
                let id: usize = name.trim_start_matches("_inventory_").parse().unwrap();
                owner = Some(owners[id]);
            }
            Item::Instruction(i) => {
                if let Some(index) = owner {
                    signatures[index]
                        .opcodes
                        .insert(i.opcode.name().to_string());
                }
            }
            _ => {}
        }
    }
    for signature in &signatures {
        assert!(
            !signature.opcodes.is_empty(),
            "{} {}",
            signature.op,
            signature.ty
        );
    }
    eprintln!(
        "{} declared signatures, {} exhaustive immediate cases",
        signatures.len(),
        owners.len()
    );
    if let Some(path) = std::env::var_os("KEEL_NEON_SIGNATURE_INVENTORY") {
        let rows=signatures.iter().map(|s|format!(
            "{{\"intrinsic\":\"{}\",\"result\":\"{}\",\"inputs\":[{}],\"immediate_range\":{},\"emitted_cases\":{},\"observed_opcodes\":[{}]}}",
            s.op,s.ty,s.inputs.iter().map(|t|format!("\"{t}\"")).collect::<Vec<_>>().join(","),
            s.range.as_ref().map_or("null".to_string(),|r|format!("[{},{}]",r.start(),r.end())),s.cases,
            s.opcodes.iter().map(|o|format!("\"{o}\"")).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",\n");
        fs::write(path,format!("{{\"schema_version\":1,\"signature_count\":{},\"emitted_case_count\":{},\"validation\":\"All declared signatures and immediates parsed, verified, compiled and assembled with Baseline/Full. Observed opcodes include transport and recipes; native numerical validation is separate.\",\"signatures\":[\n{rows}\n]}}\n",signatures.len(),owners.len())).unwrap();
    }
}
