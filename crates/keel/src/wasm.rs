//! Bounded Wasm output. Never enters native codegen or linking.
use super::{Options as DriverOptions, input};
use keel_ir::{Module, Target};
use keel_wasm::{Export, Wasm32MemoryOptions, Wasm32RuntimeOptions};
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) struct Options {
    pub exports: Vec<String>,
    pub basic: bool,
    pages: u32,
    data_floor: u32,
    stack_bottom: u32,
    stack_top: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            exports: Vec::new(),
            basic: false,
            pages: 1,
            data_floor: 1024,
            stack_bottom: 32768,
            stack_top: 65536,
        }
    }
}
impl Options {
    pub fn set_number(&mut self, flag: &str, value: &str) -> Result<(), String> {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("`{flag}` needs a decimal u32"));
        }
        let number = value
            .parse::<u32>()
            .map_err(|_| format!("`{flag}` needs a decimal u32"))?;
        match flag {
            "--wasm-memory-pages" => self.pages = number,
            "--wasm-data-floor" => self.data_floor = number,
            "--wasm-stack-bottom" => self.stack_bottom = number,
            "--wasm-stack-top" => self.stack_top = number,
            _ => unreachable!(),
        }
        Ok(())
    }
    fn runtime(&self) -> Wasm32RuntimeOptions {
        Wasm32RuntimeOptions {
            memory: Wasm32MemoryOptions {
                scalar: keel_wasm::Options {
                    exports: self
                        .exports
                        .iter()
                        .map(|name| Export {
                            name: name.clone(),
                            function: name.clone(),
                        })
                        .collect(),
                },
                pages: self.pages,
                export_name: "memory".into(),
            },
            data_floor: self.data_floor,
            stack_bottom: self.stack_bottom,
            stack_top: self.stack_top,
        }
    }
}

pub(super) fn validate_options(opts: &DriverOptions, supplied: &[&str]) -> Result<(), String> {
    let wasm_flag = supplied
        .iter()
        .any(|f| *f == "--export" || f.starts_with("--wasm-"));
    if !opts.emit_wasm && !wasm_flag {
        return Ok(());
    }
    if !opts.emit_wasm || opts.target != Some(Target::Wasm32UnknownUnknown) {
        return Err("Wasm flags require `--emit-wasm --target wasm32-unknown-unknown`".into());
    }
    // Presence, rather than final values, matters: even explicit native defaults
    // and overridden native optimization requests have no Wasm contract.
    for flag in supplied {
        if !matches!(
            *flag,
            "--emit-wasm"
                | "--wasm-opt"
                | "--target"
                | "--export"
                | "-o"
                | "-I"
                | "-D"
                | "-U"
                | "-isystem"
                | "--wasm-memory-pages"
                | "--wasm-data-floor"
                | "--wasm-stack-bottom"
                | "--wasm-stack-top"
        ) {
            return Err(format!(
                "`{flag}` is unsupported with --emit-wasm: native optimization/output, debug and hosted configuration are unavailable"
            ));
        }
    }
    if opts
        .output
        .as_deref()
        .is_none_or(|p| p.is_empty() || p == "-")
    {
        return Err("`--emit-wasm` requires `-o <output.wasm>` (not stdout)".into());
    }
    Ok(())
}

pub(super) fn compile_ir(opts: &DriverOptions) -> Result<(), String> {
    let path = opts.input.as_deref().ok_or(super::USAGE)?;
    let source = input::Source::open(path).map_err(|e| format!("{path}: {e}"))?;
    let text = source.text().map_err(|e| format!("{path}: {e}"))?;
    // Raw IR has caller-owned wasm32 storage provenance. Target-tagged artifacts
    // are a different format and cannot be relabelled through this interface.
    if text.trim_start().starts_with("KEEL-IR-UNIT") {
        return Err("target-tagged IR artifacts cannot be relabelled as raw wasm32 IR".into());
    }
    let module = keel_ir::parse(text).map_err(|e| format!("{path}: {e}"))?;
    drop(source);
    emit(opts, &module)
}

pub(super) fn compile_c(opts: &DriverOptions) -> Result<(), String> {
    let path = opts.input.as_deref().ok_or(super::USAGE)?;
    let source = input::Source::open(path).map_err(|e| format!("{path}: {e}"))?;
    let text = source.text().map_err(|e| format!("{path}: {e}"))?;
    let preprocessed = if path.ends_with(".i") {
        None
    } else {
        Some(
            keel_c::preprocess(
                text,
                path,
                &keel_c::PreprocessOptions {
                    target: Target::Wasm32UnknownUnknown,
                    include_dirs: opts.include_dirs.clone(),
                    system_include_dirs: opts.system_include_dirs.clone(),
                    defines: opts.defines.clone(),
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?,
        )
    };
    let module = keel_c::compile(
        preprocessed.as_deref().unwrap_or(text),
        path,
        keel_c::Options {
            target: Target::Wasm32UnknownUnknown,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())?;
    drop(source);
    emit(opts, &module)
}

fn emit(opts: &DriverOptions, module: &Module) -> Result<(), String> {
    // This API verifies with WASM32, preflights the complete module, and validates
    // the complete bytes as MVP before any output file is opened.
    let runtime = opts.wasm.runtime();
    let optimized;
    let module = if opts.wasm.basic {
        // Preserve the complete ORIGINAL unsupported-input contract before any
        // constant branch pruning or DCE can hide instructions or definitions.
        keel_wasm::validate_wasm32_runtime_module(module, &runtime).map_err(|e| e.to_string())?;
        optimized = {
            let mut module = module.clone();
            keel_opt::optimize_wasm32_basic(&mut module).map_err(|e| e.to_string())?;
            module
        };
        &optimized
    } else {
        module
    };
    let bytes =
        keel_wasm::emit_wasm32_runtime_module(module, &runtime).map_err(|e| e.to_string())?;
    publish(
        Path::new(opts.output.as_deref().expect("validated output")),
        &bytes,
    )
}

fn publish(output: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // create_new never overwrites another invocation's temporary file. Keep the
    // temporary beside the destination so rename stays on the same filesystem.
    let (temporary, mut file) = loop {
        let temporary = output.with_file_name(format!(
            ".keel-wasm-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", output.display())),
        }
    };
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&temporary, output));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("{}: {e}", output.display()))
}
