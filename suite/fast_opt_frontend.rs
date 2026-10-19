//! Diagnostic only: copy to a frozen checkout's crates/keel/examples/, build
//! with --features keel-c/profile into a separate private target directory.
//! fast_opt_frontend REPS INPUT.i ...; invoke through fast_opt_locked.py.
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let reps: usize = args.get(1).ok_or("missing reps")?.parse()?;
    let opts = keel_c::Options {
        target: keel_ir::Target::Aarch64AppleDarwin,
        ..Default::default()
    };
    for path in &args[2..] {
        let text = std::fs::read_to_string(path)?;
        for round in 0..reps + 2 {
            let (module, t) = keel_c::profile::compile(black_box(&text), path, opts)?;
            if round == 0 {
                let public = keel_c::compile(&text, path, opts)?;
                assert_eq!(format!("{module:?}"), format!("{public:?}"));
                keel_ir::verify_module(&module).map_err(|e| format!("{e:?}"))?;
            }
            let start = Instant::now();
            drop(black_box(module));
            let destroy_ms = start.elapsed().as_secs_f64() * 1000.;
            if round >= 2 {
                println!(
                    "{{\"input\":{path:?},\"round\":{},\"lex_ms\":{},\"parse_ms\":{},\"irgen_ms\":{},\"cleanup_ms\":{},\"destroy_ir_ms\":{destroy_ms}}}",
                    round - 2,
                    t.lex_ms,
                    t.parse_ms,
                    t.irgen_ms,
                    t.cleanup_ms
                );
            }
        }
    }
    Ok(())
}
