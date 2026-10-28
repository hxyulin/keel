//! Same-IR backend comparison harness. Every configuration compiles the same
//! parsed and verified Keel module in-process; no IR passes run here.
//!
//!   objects --out-dir DIR [--label L] IN.keel       write one object per config
//!   time --warmup W --reps R --out FILE.json [--emit-dir DIR] L=IN.keel...
//!   support IN.keel                                  per-function coverage
//!   subset IN.keel OUT.keel                          keep supported functions;
//!                                                    the rest become declarations
//!
//! Configurations:
//!   keel-baseline  native backend, baseline selection (what `keel compile` uses)
//!   keel-full      native backend, all selection passes (what `-O` adds)
//!   clif-none      Cranelift opt_level=none
//!   clif-speed     Cranelift opt_level=speed
//! Native stages: codegen (IR -> typed assembly program, incl. allocation) and
//! object (encoding/assembly + Mach-O). Cranelift stages: setup (ISA/object
//! module), translate (Keel -> CLIF), codegen (incl. machine-code emission)
//! and object (definitions + Mach-O). Stage boundaries differ between the two
//! backends; only per-config totals are directly comparable.
use keel_cranelift::{OptLevel, Options, TargetProfile};
use std::time::Instant;

const CONFIGS: [&str; 4] = ["keel-baseline", "keel-full", "clif-none", "clif-speed"];

struct Run {
    bytes: Vec<u8>,
    stages: Vec<(&'static str, u128)>,
}

fn native(module: &keel_ir::Module, full: bool) -> Result<Run, String> {
    let selector = keel_codegen::tuning::TunedSelector {
        options: if full {
            keel_codegen::SelectionOptions::ALL
        } else {
            keel_codegen::SelectionOptions::default()
        },
        cpu: keel_codegen::tuning::TuningCpu::Generic,
    };
    // Identical composition to the driver's `--emit-object` path.
    let feature_selector = keel_codegen::x86_features::FeatureSelector {
        inner: &selector,
        features: keel_asm::x86::Features::default(),
    };
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &feature_selector,
        allocator: &keel_codegen::regalloc::AllocationTier::Full,
    };
    let start = Instant::now();
    let program = keel_codegen::compile_program_with_pipeline(
        module,
        keel_codegen::Target::Aarch64AppleDarwin,
        &pipeline,
    )
    .map_err(|e| e.to_string())?;
    let codegen = start.elapsed().as_nanos();
    let start = Instant::now();
    let bytes = keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
        .map_err(|e| e.to_string())?;
    let object = start.elapsed().as_nanos();
    Ok(Run {
        bytes,
        stages: vec![("codegen", codegen), ("object", object)],
    })
}

fn cranelift(module: &keel_ir::Module, opt: OptLevel, verify_clif: bool) -> Result<Run, String> {
    let options = Options {
        target: TargetProfile::AppleArm64,
        opt,
        verify_clif,
    };
    let (bytes, t, _) = keel_cranelift::compile_verified(module, &options)?;
    Ok(Run {
        bytes,
        stages: vec![
            ("setup", t.setup.as_nanos()),
            ("translate", t.translate.as_nanos()),
            ("codegen", t.codegen.as_nanos()),
            ("object", t.object.as_nanos()),
        ],
    })
}

fn run_config(module: &keel_ir::Module, config: &str, verify_clif: bool) -> Result<Run, String> {
    match config {
        "keel-baseline" => native(module, false),
        "keel-full" => native(module, true),
        "clif-none" => cranelift(module, OptLevel::None, verify_clif),
        "clif-speed" => cranelift(module, OptLevel::Speed, verify_clif),
        _ => unreachable!(),
    }
}

fn load(path: &str) -> Result<(String, keel_ir::Module), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let module = keel_ir::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    keel_ir::verify_module(&module).map_err(|e| format!("{path}: invalid IR: {e:?}"))?;
    Ok((text, module))
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn objects(args: &[String]) -> Result<(), String> {
    let mut out_dir = None;
    let mut label = None;
    let mut input = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out-dir" => out_dir = it.next().cloned(),
            "--label" => label = it.next().cloned(),
            _ => input = Some(a.clone()),
        }
    }
    let out_dir = out_dir.ok_or("--out-dir required")?;
    let input = input.ok_or("input required")?;
    let label = label.unwrap_or_else(|| "out".into());
    let (_, module) = load(&input)?;
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    for config in CONFIGS {
        // Objects for correctness runs are built with the CLIF verifier on.
        let run = run_config(&module, config, true).map_err(|e| format!("{config}: {e}"))?;
        let path = format!("{out_dir}/{label}.{config}.o");
        std::fs::write(&path, &run.bytes).map_err(|e| format!("{path}: {e}"))?;
        println!(
            "{{\"config\":{},\"path\":{},\"bytes\":{}}}",
            json_str(config),
            json_str(&path),
            run.bytes.len()
        );
    }
    Ok(())
}

fn time(args: &[String]) -> Result<(), String> {
    let mut warmup = 3usize;
    let mut reps = 15usize;
    let mut out = None;
    let mut emit_dir = None;
    let mut inputs = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--warmup" => warmup = it.next().and_then(|s| s.parse().ok()).ok_or("--warmup N")?,
            "--reps" => reps = it.next().and_then(|s| s.parse().ok()).ok_or("--reps N")?,
            "--out" => out = it.next().cloned(),
            "--emit-dir" => emit_dir = it.next().cloned(),
            s => {
                let (label, path) = s.split_once('=').ok_or("inputs are LABEL=PATH")?;
                inputs.push((label.to_string(), path.to_string()));
            }
        }
    }
    let out = out.ok_or("--out FILE.json required")?;
    if std::path::Path::new(&out).exists() {
        return Err(format!("{out} exists; refusing to overwrite evidence"));
    }
    let mut entries = Vec::new();
    for (label, path) in &inputs {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        // Correctness gate: the verified build must equal the timed build.
        let (_, reference_module) = load(path)?;
        let mut reference = Vec::new();
        for config in CONFIGS {
            let run = run_config(&reference_module, config, true)
                .map_err(|e| format!("{label} {config}: {e}"))?;
            if let Some(dir) = &emit_dir {
                let p = format!("{dir}/{label}.{config}.o");
                std::fs::write(&p, &run.bytes).map_err(|e| format!("{p}: {e}"))?;
            }
            reference.push(fnv(&run.bytes));
        }
        let mut parse_ns = Vec::new();
        let mut samples: Vec<Vec<Vec<(&'static str, u128)>>> = vec![Vec::new(); CONFIGS.len()];
        let mut sizes = vec![0usize; CONFIGS.len()];
        for rep in 0..warmup + reps {
            let start = Instant::now();
            let module = keel_ir::parse(&text).map_err(|e| e.to_string())?;
            keel_ir::verify_module(&module).map_err(|e| format!("{e:?}"))?;
            let parsed = start.elapsed().as_nanos();
            // Rotate configuration order so drift is not attributed to one.
            for k in 0..CONFIGS.len() {
                let c = (rep + k) % CONFIGS.len();
                let run = run_config(&module, CONFIGS[c], false)?;
                if fnv(&run.bytes) != reference[c] {
                    return Err(format!(
                        "{label} {}: nondeterministic or verifier-dependent output",
                        CONFIGS[c]
                    ));
                }
                sizes[c] = run.bytes.len();
                if rep >= warmup {
                    samples[c].push(run.stages);
                }
            }
            if rep >= warmup {
                parse_ns.push(parsed);
            }
        }
        let functions = reference_module.functions.len();
        let insts: usize = reference_module
            .functions
            .iter()
            .map(|f| f.layout_insts().count())
            .sum();
        let mut configs = Vec::new();
        for (c, config) in CONFIGS.iter().enumerate() {
            let stage_names: Vec<_> = samples[c][0].iter().map(|(n, _)| *n).collect();
            let mut stages = Vec::new();
            for (s, name) in stage_names.iter().enumerate() {
                let values: Vec<String> = samples[c].iter().map(|r| r[s].1.to_string()).collect();
                stages.push(format!("{}:[{}]", json_str(name), values.join(",")));
            }
            let totals: Vec<String> = samples[c]
                .iter()
                .map(|r| r.iter().map(|(_, v)| v).sum::<u128>().to_string())
                .collect();
            configs.push(format!(
                "{}:{{\"stages_ns\":{{{}}},\"total_ns\":[{}],\"object_bytes\":{},\"fnv64\":\"{:016x}\"}}",
                json_str(config),
                stages.join(","),
                totals.join(","),
                sizes[c],
                reference[c]
            ));
        }
        let parse: Vec<String> = parse_ns.iter().map(u128::to_string).collect();
        entries.push(format!(
            "{{\"label\":{},\"path\":{},\"functions\":{functions},\"keel_instructions\":{insts},\"parse_verify_ns\":[{}],\"configs\":{{{}}}}}",
            json_str(label),
            json_str(path),
            parse.join(","),
            configs.join(",")
        ));
    }
    let settings: Vec<String> = [OptLevel::None, OptLevel::Speed]
        .iter()
        .map(|&opt| {
            let s = keel_cranelift::explicit_settings(&Options {
                target: TargetProfile::AppleArm64,
                opt,
                verify_clif: false,
            });
            let kv: Vec<String> = s
                .iter()
                .map(|(k, v)| format!("{}:{}", json_str(k), json_str(v)))
                .collect();
            format!("{{{}}}", kv.join(","))
        })
        .collect();
    let doc = format!(
        "{{\"schema\":1,\"warmup\":{warmup},\"reps\":{reps},\"timed_cranelift_settings\":[{}],\"inputs\":[{}]}}\n",
        settings.join(","),
        entries.join(",")
    );
    std::fs::write(&out, doc).map_err(|e| format!("{out}: {e}"))
}

fn support(args: &[String]) -> Result<(), String> {
    for path in args {
        let (_, module) = load(path)?;
        for (name, result) in keel_cranelift::function_support(&module) {
            let error = match result {
                Ok(()) => "null".to_string(),
                Err(e) => json_str(&e),
            };
            println!(
                "{{\"input\":{},\"function\":{},\"error\":{error}}}",
                json_str(path),
                json_str(&name)
            );
        }
    }
    Ok(())
}

/// Compile-time-only inputs from real code: functions the bridge rejects are
/// replaced by external declarations, so both backends see identical IR.
fn subset(args: &[String]) -> Result<(), String> {
    let [input, output] = args else {
        return Err("subset IN.keel OUT.keel".into());
    };
    let (_, mut module) = load(input)?;
    let rejected: Vec<String> = keel_cranelift::function_support(&module)
        .into_iter()
        .filter_map(|(name, r)| r.err().map(|_| name))
        .collect();
    for f in module
        .functions
        .iter()
        .filter(|f| rejected.contains(&f.name))
    {
        if !module.decls.iter().any(|d| d.name == f.name) {
            module.decls.push(keel_ir::FuncDecl {
                attrs: f.attrs.clone(),
                name: f.name.clone(),
                sig: f.sig.clone(),
            });
        }
    }
    module.functions.retain(|f| !rejected.contains(&f.name));
    keel_ir::verify_module(&module).map_err(|e| format!("subset invalid: {e:?}"))?;
    let text = module.to_string();
    let reparsed = keel_ir::parse(&text).map_err(|e| format!("subset reparse: {e}"))?;
    keel_ir::verify_module(&reparsed).map_err(|e| format!("subset reparse: {e:?}"))?;
    std::fs::write(output, text).map_err(|e| format!("{output}: {e}"))?;
    println!(
        "{{\"input\":{},\"kept\":{},\"rejected\":{}}}",
        json_str(input),
        module.functions.len(),
        rejected.len()
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("objects") => objects(&args[1..]),
        Some("time") => time(&args[1..]),
        Some("support") => support(&args[1..]),
        Some("subset") => subset(&args[1..]),
        _ => Err("usage: keel-cranelift-bench objects|time|support ...; see source header".into()),
    };
    if let Err(e) = result {
        eprintln!("keel-cranelift-bench: {e}");
        std::process::exit(1);
    }
}
