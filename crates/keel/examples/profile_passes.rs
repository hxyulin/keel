//! Separate pass transformations from the verifier calls in the default pipeline.
//! Mirrors run_pipeline, checks its final IR, and changes no compiler policy.
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let rounds: usize = args
        .get(1)
        .ok_or("usage: profile_passes ROUNDS INPUT.i ...")?
        .parse()?;
    if rounds < 3 || args.len() < 3 {
        return Err("provide at least three rounds and one input".into());
    }
    let passes: Vec<_> = keel_opt::DEFAULT_PIPELINE
        .iter()
        .map(|n| keel_opt::find_pass(n).unwrap())
        .collect();
    let canonicalize = keel_opt::find_pass("canonicalize").unwrap();
    for path in &args[2..] {
        let source = std::fs::read_to_string(path)?;
        let original = keel_c::compile(&source, path, keel_c::Options::default())?;
        keel_ir::verify_module(&original).map_err(|e| format!("invalid IR: {e:?}"))?;
        let mut reference = original.clone();
        keel_opt::optimize(&mut reference).map_err(|e| e.to_string())?;
        let expected = reference.to_string();
        let mut samples = Vec::new();
        for round in 0..rounds + 2 {
            let mut module = original.clone();
            // The final entry records any canonical prerequisite restoration.
            let mut times = vec![[0., 0.]; passes.len() + 1];
            for func in &mut module.functions {
                let mut canonical = false;
                for (i, pass) in passes.iter().enumerate() {
                    if pass.requires_canonical_form() && !canonical {
                        let start = Instant::now();
                        canonicalize.run(func);
                        times[passes.len()][0] += start.elapsed().as_secs_f64() * 1000.;
                        let start = Instant::now();
                        keel_ir::verify(func).map_err(|e| format!("invalid IR: {e:?}"))?;
                        times[passes.len()][1] += start.elapsed().as_secs_f64() * 1000.;
                        canonical = true;
                    }
                    let start = Instant::now();
                    let changed = pass.run(func);
                    times[i][0] += start.elapsed().as_secs_f64() * 1000.;
                    canonical = pass.produces_canonical_form() || (canonical && !changed);
                    let start = Instant::now();
                    keel_ir::verify(func).map_err(|e| format!("invalid IR: {e:?}"))?;
                    times[i][1] += start.elapsed().as_secs_f64() * 1000.;
                }
            }
            if round == 0 {
                assert_eq!(
                    module.to_string(),
                    expected,
                    "instrumentation differs: {path}"
                );
            }
            black_box(module);
            if round >= 2 {
                samples.push(times);
            }
        }
        println!("{{\"input\":{path:?},\"pass_transform_verify_ms\":{samples:?}}}");
    }
    Ok(())
}
