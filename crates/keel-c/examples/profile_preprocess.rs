//! Native preprocessor profiling and preparation of the frontend corpus.
//! profile_preprocess prepare|time|sample ROUNDS APP SDK OUTPUT_DIR INPUT.c...
use std::{hint::black_box, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 7 {
        return Err("expected MODE ROUNDS APP SDK OUTPUT_DIR INPUT.c...".into());
    }
    let mode = &args[1];
    if !["prepare", "time", "sample", "stats"].contains(&mode.as_str()) {
        return Err("invalid mode".into());
    }
    let rounds: usize = args[2].parse()?;
    if rounds == 0 {
        return Err("rounds must be positive".into());
    }
    let mut opts = keel_c::PreprocessOptions {
        sysroot: Some(PathBuf::from(&args[4])),
        ..Default::default()
    };
    let definitions: &[(&str, &str)] = match args[3].as_str() {
        "lua" => &[("LUA_USE_MACOSX", "1")],
        "quickjs" => &[
            ("_GNU_SOURCE", "1"),
            ("CONFIG_VERSION", "\"2024-01-13\""),
            ("CONFIG_BIGNUM", "1"),
        ],
        "sqlite" => &[("SQLITE_THREADSAFE", "1")],
        _ => return Err("expected lua|quickjs|sqlite".into()),
    };
    opts.defines = definitions
        .iter()
        .map(|(k, v)| (k.to_string(), Some(v.to_string())))
        .collect();
    if let Some(p) = args[6..].iter().find(|p| p.ends_with("/quickjs.c")) {
        opts.include_dirs
            .push(std::path::Path::new(p).parent().unwrap().into());
    }
    let inputs: Vec<_> = args[6..]
        .iter()
        .map(|p| Ok((p, std::fs::read_to_string(p)?)))
        .collect::<Result<_, std::io::Error>>()?;
    std::fs::create_dir_all(&args[5])?;
    for (path, src) in &inputs {
        let pp = keel_c::preprocess(src, path, &opts)?;
        if mode == "prepare" {
            let output = PathBuf::from(&args[5])
                .join(std::path::Path::new(path).file_name().unwrap())
                .with_extension("i");
            let module = keel_c::compile(&pp, path, keel_c::Options::default())?;
            keel_ir::verify_module(&module).map_err(|e| format!("{path}: {e:?}"))?;
            std::fs::write(output, &pp)?;
        }
    }
    eprintln!("ready: {} inputs, {rounds} rounds, {mode}", inputs.len());
    if mode == "prepare" {
        return Ok(());
    }
    for round in 0..rounds {
        for offset in 0..inputs.len() {
            let (path, src) = &inputs[(offset + round) % inputs.len()];
            if mode == "stats" {
                #[cfg(feature = "profile")]
                {
                    let (pp, s) = keel_c::preprocess_with_stats(src, path, &opts)?;
                    println!(
                        "{{\"path\":{path:?},\"include_requests\":{},\"file_opens\":{},\"mapped_files\":{},\"source_cache_hits\":{},\"token_cache_hits\":{},\"scanned_files\":{},\"guard_skips\":{},\"once_skips\":{},\"source_bytes\":{},\"arena_bytes\":{},\"token_bytes\":{},\"output_bytes\":{}}}",
                        s.include_requests,
                        s.file_opens,
                        s.mapped_files,
                        s.source_cache_hits,
                        s.token_cache_hits,
                        s.scanned_files,
                        s.guard_skips,
                        s.once_skips,
                        s.source_bytes,
                        s.arena_bytes,
                        s.token_bytes,
                        pp.len()
                    );
                    continue;
                }
                #[cfg(not(feature = "profile"))]
                return Err("stats mode requires the profile feature".into());
            }
            let start = Instant::now();
            let pp = keel_c::preprocess(black_box(src), path, &opts)?;
            let preprocess_ms = start.elapsed().as_secs_f64() * 1000.;
            let output_bytes = pp.len();
            let start = Instant::now();
            drop(black_box(pp));
            let output_drop_ms = start.elapsed().as_secs_f64() * 1000.;
            if mode != "sample" {
                println!(
                    "{{\"path\":{path:?},\"round\":{round},\"bytes\":{},\"output_bytes\":{output_bytes},\"preprocess_ms\":{preprocess_ms},\"output_drop_ms\":{output_drop_ms}}}",
                    src.len()
                );
            }
        }
    }
    Ok(())
}
