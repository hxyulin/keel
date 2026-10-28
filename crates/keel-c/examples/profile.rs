//! Resident-input frontend profiling. Usage: profile staged|baseline|sample ROUNDS INPUT.i... [--debug].
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 4 {
        return Err("usage: profile staged|baseline|sample ROUNDS INPUT.i... [--debug]".into());
    }
    let mode = &args[1];
    if !["staged", "baseline", "sample", "prepare"].contains(&mode.as_str()) {
        return Err("unknown mode".into());
    }
    let rounds: usize = args[2].parse()?;
    if rounds == 0 {
        return Err("rounds must be positive".into());
    }
    let opts = keel_c::Options {
        debug_info: args.iter().any(|a| a == "--debug"),
        ..Default::default()
    };
    let first_input = if mode == "prepare" { 4 } else { 3 };
    if args.len() <= first_input {
        return Err("prepare requires OUTPUT_DIR followed by input files".into());
    }
    let inputs: Vec<_> = args[first_input..]
        .iter()
        .filter(|a| a.as_str() != "--debug")
        .map(|p| Ok((p.clone(), std::fs::read_to_string(p)?)))
        .collect::<Result<_, std::io::Error>>()?;
    if mode == "prepare" {
        std::fs::create_dir_all(&args[3])?;
    }
    // Validate each translation unit before timing. Warm both paths and verify IR.
    for (path, src) in &inputs {
        let ordinary = keel_c::compile(src, path, opts)?;
        let (staged, _) = keel_c::profile::compile(src, path, opts)?;
        keel_ir::verify_module(&ordinary).map_err(|e| format!("{path}: {e:?}"))?;
        keel_ir::verify_module(&staged).map_err(|e| format!("{path}: {e:?}"))?;
        assert_eq!(
            format!("{ordinary:?}"),
            format!("{staged:?}"),
            "stage helper changed IR: {path}"
        );
        if mode == "prepare" {
            let output = std::path::Path::new(&args[3])
                .join(std::path::Path::new(path).file_name().unwrap())
                .with_extension("ir");
            std::fs::write(output, format!("{ordinary:?}"))?;
        }
    }
    eprintln!("ready: {} inputs, {rounds} rounds, {mode}", inputs.len());
    if mode == "prepare" {
        return Ok(());
    }
    for round in 0..rounds {
        for offset in 0..inputs.len() {
            let (path, src) = &inputs[(offset + round) % inputs.len()];
            let start = Instant::now();
            let (module, stages) = if mode == "staged" {
                let (module, times) = keel_c::profile::compile(black_box(src), path, opts)?;
                (module, Some(times))
            } else {
                (keel_c::compile(black_box(src), path, opts)?, None)
            };
            let total_ms = start.elapsed().as_secs_f64() * 1000.;
            let functions = module.functions.len();
            let start = Instant::now();
            drop(black_box(module));
            let drop_ms = start.elapsed().as_secs_f64() * 1000.;
            if mode == "sample" {
                continue;
            }
            // Paths in this suite are ordinary UTF-8; Debug quotes escape JSON strings.
            print!(
                "{{\"path\":{path:?},\"round\":{round},\"mode\":{mode:?},\"debug\":{},\"bytes\":{},\"functions\":{functions},\"total_ms\":{total_ms},\"module_drop_ms\":{drop_ms}",
                opts.debug_info,
                src.len()
            );
            if let Some(t) = stages {
                print!(
                    ",\"lex_ms\":{},\"parse_ms\":{},\"irgen_ms\":{},\"cleanup_ms\":{},\"tokens\":{}",
                    t.lex_ms, t.parse_ms, t.irgen_ms, t.cleanup_ms, t.tokens
                );
            }
            println!("}}");
        }
    }
    Ok(())
}
