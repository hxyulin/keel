//! Driver regressions: fingerprints captured from the recursive lowerer at 6d974b4d.
#[path = "../src/vector.rs"]
mod vector;

use keel_ir::{EntityRef, Function, InstData, SourceFile, SourceLoc, Value};
use std::{borrow::Cow, cell::RefCell, fmt::Write};

fn compound() -> keel_ir::Module {
    let mut source = String::new();
    for op in ["vpadd", "vpsmin", "vpumax", "vfpadd", "vfpmin", "vfpmax"] {
        let ty = if op.starts_with("vf") {
            "f32x4"
        } else {
            "i32x4"
        };
        source += &format!(
            "func @{op}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n v2 = load.{ty} align 1 v0\n v3 = {op} v2, v2\n store align 1 v3,v1\n return\n}}\n"
        );
    }
    for op in [
        "snarrow_sat_low",
        "unarrow_sat_low",
        "sunarrow_sat_low",
        "spairwise_add_long",
        "upairwise_add_long",
    ] {
        let (a, b) = if op.contains("narrow") {
            ("i32x4", "i16x8")
        } else {
            ("i16x8", "i32x4")
        };
        source += &format!(
            "func @{op}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n v2 = load.{a} align 1 v0\n v3 = {op}.{b} v2\n store align 1 v3,v1\n return\n}}\n"
        );
    }
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for f in &mut module.functions {
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        for (line, inst) in insts.into_iter().enumerate() {
            f.set_inst_loc(
                inst,
                Some(SourceLoc {
                    file: SourceFile::new(0),
                    line: line as u32 + 1,
                    col: 3,
                    scope: None,
                }),
            );
        }
    }
    module
}

// Include removed instruction IDs, aliases, slots and locations as well as layout.
fn snapshot(f: &Function) -> String {
    let mut s = f.to_string();
    write!(
        s,
        "\nslots={:?}; values={}; insts={}\n",
        f.stack_slots().collect::<Vec<_>>(),
        f.num_values(),
        f.num_insts()
    )
    .unwrap();
    for i in 0..f.num_values() {
        let v = Value::new(i);
        write!(s, "{v}:{:?}=>{};", f.value_type(v), f.resolve(v)).unwrap();
    }
    for (_, i) in f.layout_insts() {
        write!(s, "{i}:{:?};", f.inst_loc(i)).unwrap();
    }
    s
}

// Stable FNV-1a fingerprints keep the full IR and callback snapshots compact.
fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

#[test]
fn compound_rounds_preserve_ir_and_current_ir_callback_order() {
    let module = compound();
    let mut results = Vec::new();
    for policy in 0..3 {
        let trace = RefCell::new(String::new());
        let mut lowered = String::new();
        for f in &module.functions {
            let result = vector::lower(f, |current, data| {
                write!(trace.borrow_mut(), "{}:{data:?};", hash(&snapshot(current))).unwrap();
                match policy {
                    0 => false,
                    // Keep generated primitives, including the final no-change scan.
                    1 => matches!(
                        data,
                        InstData::VectorShuffle { .. }
                            | InstData::VectorBinary {
                                op: keel_ir::VectorBinaryOp::Iadd,
                                ..
                            }
                    ),
                    // Consult current IR after replacements rather than caching answers.
                    _ => {
                        current.num_stack_slots() > 0
                            && matches!(data, InstData::VectorShuffle { .. })
                    }
                }
            });
            assert!(matches!(result, Cow::Owned(_)));
            keel_ir::verify(&result).unwrap();
            lowered += &snapshot(&result);
            assert!(matches!(
                vector::lower(&result, |_, _| true),
                Cow::Borrowed(_)
            ));
        }
        results.push((hash(&lowered), hash(&trace.into_inner())));
    }
    // Updated only for an intentional change in legalization, including callback order.
    assert_eq!(
        results,
        vec![
            (12886462691823731048, 7454847585204559076),
            (16625842418772809980, 14656814968899380797),
            (3323899452263261746, 15704128218566998812)
        ]
    );
}

#[test]
fn empty_scalar_and_native_functions_stay_borrowed() {
    let module = keel_ir::parse("decl @empty()\nfunc @scalar(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}\nfunc @native(i32x4,i32x4) -> i32x4 {\nblock0(v0: i32x4,v1: i32x4):\n v2 = viadd v0,v1\n return v2\n}").unwrap();
    let empty = Function::new("empty", keel_ir::Signature::new(&[], &[]));
    assert!(
        matches!(vector::lower(&empty, |_, _| panic!("empty callback")), Cow::Borrowed(f) if std::ptr::eq(f, &empty))
    );
    for f in &module.functions {
        let calls = RefCell::new(0);
        let result = vector::lower(f, |_, _| {
            *calls.borrow_mut() += 1;
            true
        });
        assert!(matches!(result, Cow::Borrowed(g) if std::ptr::eq(g, f)));
        assert_eq!(*calls.borrow(), usize::from(f.name == "native"));
    }
}

#[test]
fn compound_corpus_has_text_typed_chunked_object_parity() {
    use keel_asm_ir::x86::{CpuLevel, Features};
    use keel_codegen::{
        CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier,
        x86_features::FeatureSelector,
    };
    let module = compound();
    for target in [Target::X86_64LinuxGnu, Target::Aarch64AppleDarwin] {
        let levels: &[_] = if target == Target::X86_64LinuxGnu {
            &[CpuLevel::V1, CpuLevel::V2, CpuLevel::V3]
        } else {
            &[CpuLevel::V1]
        };
        for &level in levels {
            let features = if target == Target::X86_64LinuxGnu {
                Features::for_level(level)
            } else {
                Features::default()
            };
            for selection in [
                SelectionTier::Baseline,
                SelectionTier::Trees,
                SelectionTier::Full,
            ] {
                for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                    let selector = FeatureSelector {
                        inner: &selection,
                        features,
                    };
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocation,
                    };
                    let text =
                        keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
                    let program =
                        keel_codegen::compile_program_with_pipeline(&module, target, &pipeline)
                            .unwrap();
                    let mut chunks = keel_asm_ir::Program::default();
                    keel_codegen::compile_program_chunks_with_pipeline(
                        &module,
                        target,
                        &pipeline,
                        &mut |part| {
                            chunks.items.extend(part.items);
                        },
                    )
                    .unwrap();
                    assert_eq!(program.stats().text_bytes, 0);
                    assert_eq!(chunks.stats().text_bytes, 0);
                    let (parsed, typed, chunked) = if target == Target::X86_64LinuxGnu {
                        (
                            keel_asm::x86::assemble_object_with_features(&text, features).unwrap(),
                            keel_asm::x86::assemble_program_object_with_features(
                                &program, features,
                            )
                            .unwrap(),
                            keel_asm::x86::assemble_program_object_with_features(&chunks, features)
                                .unwrap(),
                        )
                    } else {
                        (
                            keel_asm::assemble_object(&text).unwrap(),
                            keel_asm::assemble_program_object(&program, Default::default())
                                .unwrap(),
                            keel_asm::assemble_program_object(&chunks, Default::default()).unwrap(),
                        )
                    };
                    assert_eq!(
                        parsed, typed,
                        "{target:?} {level:?} {selection:?} {allocation:?}"
                    );
                    assert_eq!(typed, chunked);
                }
            }
        }
    }
}

#[test]
fn generated_scalar_fma_is_not_reconsidered_as_vector_work() {
    let module = keel_ir::parse("func @fallback(ptr,ptr) {\nblock0(v0: ptr,v1: ptr):\n v2 = load.f32x4 align 1 v0\n v3 = fma v2,v2,v2\n v4 = vfpmin v3,v2\n store align 1 v4,v1\n return\n}").unwrap();
    let calls = RefCell::new(0);
    let lowered = vector::lower(&module.functions[0], |f, data| {
        if let InstData::Fma { args } = data {
            assert!(f.value_type(f.resolve(args[0])).is_vector());
            *calls.borrow_mut() += 1;
        }
        false
    });
    keel_ir::verify(&lowered).unwrap();
    assert_eq!(*calls.borrow(), 2); // Entry scan, then the original round snapshot.
    assert_eq!(
        lowered
            .layout_insts()
            .filter(|(_, i)| matches!(lowered.inst_data(*i), InstData::Fma { .. }))
            .count(),
        4
    );
    assert!(matches!(
        vector::lower(&lowered, |_, _| false),
        Cow::Borrowed(_)
    ));
}
