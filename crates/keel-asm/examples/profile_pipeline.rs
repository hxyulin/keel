//! Backend-only text round trip versus structured program transport.
//! C/IR reading and parsing are outside the measured region; output and
//! intermediate destruction are included. Run after release builds finish.
use keel_codegen::Target;
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: profile_pipeline INPUT.i|INPUT.keel ITERATIONS")?;
    let rounds: usize = args.get(2).ok_or("missing iterations")?.parse()?;
    if rounds < 3 {
        return Err("use at least 3 iterations".into());
    }
    let source = std::fs::read_to_string(path)?;
    let module = if path.ends_with(".keel") {
        keel_ir::parse(&source).map_err(|e| e.to_string())?
    } else {
        keel_c::compile(&source, path, keel_c::Options::default())?
    };
    keel_ir::verify_module(&module).map_err(|e| format!("invalid IR: {e:?}"))?;
    let target = Target::Aarch64AppleDarwin;
    let features = keel_asm::TargetFeatures::default();
    let text = keel_codegen::compile(&module, target)?;
    let program = keel_codegen::compile_program(&module, target)?;
    let (assembly, metrics) = keel_asm::assemble_with_metrics(&text)?;
    let expected = assembly.to_macho()?;
    let actual = keel_asm::assemble_program_object(&program, features)?;
    assert_eq!(actual, expected, "object mismatch: {path}");
    let stats = program.stats();
    let mut text_ms = Vec::new();
    let mut program_ms = Vec::new();
    for round in 0..rounds + 2 {
        let order = if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        };
        for structured in order {
            let start = Instant::now();
            if structured {
                let program = keel_codegen::compile_program(black_box(&module), target)?;
                let object = keel_asm::assemble_program_object(&program, features)?;
                black_box(object);
            } else {
                let text = keel_codegen::compile(black_box(&module), target)?;
                let object = keel_asm::assemble_object(&text)?;
                black_box(object);
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            if round >= 2 {
                if structured {
                    program_ms.push(elapsed)
                } else {
                    text_ms.push(elapsed)
                }
            }
        }
    }
    println!(
        "{{\"text_ms\":{text_ms:?},\"program_ms\":{program_ms:?},\"text_bytes\":{},\"remaining_text_bytes\":{},\"native_instructions\":{},\"total_instructions\":{},\"labels\":{},\"object_bytes\":{},\"objects_identical\":true}}",
        text.len(),
        stats.text_bytes,
        stats.instructions,
        metrics.len(),
        stats.labels,
        actual.len()
    );
    Ok(())
}
