//! Three-input conversion and execution harness. See tools/ir-harness.md.
use keel_cranelift::{OptLevel, Options, TargetProfile};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

type Result<T> = std::result::Result<T, String>;
const TARGET: &str = "arm64-apple-macosx11.0.0";
fn options() -> Options {
    Options {
        target: TargetProfile::AppleArm64,
        opt: OptLevel::None,
        verify_clif: true,
    }
}
fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}
fn write(path: &Path, data: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}
fn module(text: &str) -> Result<keel_ir::Module> {
    let m = keel_ir::parse(text).map_err(|e| e.to_string())?;
    keel_ir::verify_module(&m).map_err(|e| format!("{e:?}"))?;
    Ok(m)
}
fn command(cmd: &mut Command, log: &mut Vec<Value>) -> Result<Vec<u8>> {
    let argv = format!("{cmd:?}");
    let start = Instant::now();
    let out = cmd.output().map_err(|e| format!("{argv}: {e}"))?;
    log.push(
        json!({"command": argv, "nanoseconds": start.elapsed().as_nanos() as u64,
        "status": out.status.code(), "stderr": String::from_utf8_lossy(&out.stderr),
        "stdout": String::from_utf8_lossy(&out.stdout)}),
    );
    if !out.status.success() {
        return Err(format!("{argv}: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(out.stdout)
}
fn native(m: &keel_ir::Module) -> Result<Vec<u8>> {
    let selector = keel_codegen::tuning::TunedSelector {
        options: keel_codegen::SelectionOptions::default(),
        cpu: keel_codegen::tuning::TuningCpu::Generic,
    };
    let features = keel_codegen::x86_features::FeatureSelector {
        inner: &selector,
        features: keel_asm::x86::Features::default(),
    };
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &features,
        allocator: &keel_codegen::regalloc::AllocationTier::Full,
    };
    let program = keel_codegen::compile_program_with_pipeline(
        m,
        keel_codegen::Target::Aarch64AppleDarwin,
        &pipeline,
    )
    .map_err(|e| e.to_string())?;
    keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
        .map_err(|e| e.to_string())
}
struct Harness {
    importer: Option<keel_llvm::import::Importer>,
    args: BTreeMap<String, String>,
    out: PathBuf,
    commands: Vec<Value>,
    steps: Vec<Value>,
    expected: Vec<u8>,
}
impl Harness {
    fn arg(&self, key: &str) -> Result<&str> {
        self.args
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing --{key}"))
    }
    fn import_llvm(&mut self, path: &Path) -> Result<String> {
        if self.importer.is_none() {
            self.importer = Some(
                // SAFETY: --libllvm explicitly selects the trusted LLVM 23 runtime.
                unsafe { keel_llvm::import::Importer::load(Path::new(self.arg("libllvm")?)) }
                    .map_err(|e| e.to_string())?,
            );
        }
        self.importer
            .as_ref()
            .ok_or("LLVM importer was not initialized")?
            .translate(
                &fs::read(path).map_err(|e| e.to_string())?,
                Default::default(),
            )
            .map_err(|e| e.to_string())
    }

    fn execute(&mut self, label: &str, bytes: Vec<u8>) -> Result<()> {
        let object = self.out.join(format!("{label}.o"));
        let executable = self.out.join(format!("{label}.exe"));
        write(&object, bytes)?;
        let mut cmd = Command::new(self.args.get("clang").map_or("clang", String::as_str));
        cmd.args(["-target", TARGET])
            .arg(self.arg("driver")?)
            .arg(&object)
            .arg("-o")
            .arg(&executable);
        command(&mut cmd, &mut self.commands)?;
        let actual = command(&mut Command::new(executable), &mut self.commands)?;
        write(&self.out.join(format!("{label}.stdout")), &actual)?;
        if actual != self.expected {
            return Err(format!("{label}: execution differs from expected output"));
        }
        Ok(())
    }
    fn llvm_object(&mut self, label: &str, text: &str) -> Result<Vec<u8>> {
        let path = self.out.join(format!("{label}.ll"));
        let obj = self.out.join(format!("{label}.o"));
        write(&path, text)?;
        let mut cmd = Command::new(self.args.get("clang").map_or("clang", String::as_str));
        cmd.args(["-target", TARGET, "-Wno-override-module", "-c"])
            .arg(path)
            .arg("-o")
            .arg(&obj);
        command(&mut cmd, &mut self.commands)?;
        fs::read(obj).map_err(|e| e.to_string())
    }
    fn step(&mut self, label: &str, f: impl FnOnce(&mut Self) -> Result<()>) {
        let start = Instant::now();
        let result = f(self);
        self.steps.push(
            json!({"path":label, "status":if result.is_ok() {"passed"} else {"failed"},
            "diagnostic":result.err(), "nanoseconds":start.elapsed().as_nanos() as u64,
            "fact_preservation":"not_measured"}),
        );
    }
    fn run(&mut self) -> Result<()> {
        let input = PathBuf::from(self.arg("input")?);
        let source = read(&input)?;
        write(&self.out.join("source.txt"), &source)?;
        let format = self.arg("format")?.to_string();
        let text = match format.as_str() {
            "keel" => {
                module(&source)?;
                source.clone()
            }
            "llvm" => self.import_llvm(&input)?,
            "clif" => keel_cranelift::import::to_keel(&source, self.arg("name")?, &options())?,
            _ => return Err("--format must be keel, llvm or clif".into()),
        };
        write(&self.out.join("import.keel"), &text)?;
        let m = module(&text)?;
        self.step("input-oracle", |h| {
            let bytes = match format.as_str() {
                "llvm" => h.llvm_object("original", &source)?,
                "clif" => {
                    // Compile original CLIF directly, without Keel import/export.
                    let mut cmd = Command::new(h.arg("clif-import")?);
                    cmd.args(["--target", TARGET, "--name", h.arg("name")?, "--object"])
                        .arg(&input);
                    command(&mut cmd, &mut h.commands)?
                }
                _ => native(&m)?,
            };
            h.execute("original", bytes)
        });
        self.step("input-keel-native", |h| {
            h.execute("import-native", native(&m)?)
        });
        self.step("input-keel-llvm-keel-input", |h| {
            let llvm = keel_llvm::export(&m, keel_llvm::TargetProfile::AppleArm64)?;
            let bytes = h.llvm_object("export", &llvm)?;
            h.execute("llvm", bytes)?;
            let round = h.import_llvm(&h.out.join("export.ll"))?;
            write(&h.out.join("llvm-roundtrip.keel"), &round)?;
            h.check_roundtrip("llvm-roundtrip", &module(&round)?, &format)
        });
        self.step("input-keel-clif-keel-input", |h| {
            let clif = keel_cranelift::translate_to_clif(&m, &options())?;
            write(&h.out.join("export.clif"), &clif)?;
            h.execute("clif", keel_cranelift::compile_object(&m, &options())?)?;
            let round = keel_cranelift::import::to_keel(&clif, h.arg("name")?, &options())?;
            write(&h.out.join("clif-roundtrip.keel"), &round)?;
            h.check_roundtrip("clif-roundtrip", &module(&round)?, &format)
        });
        Ok(())
    }
    fn check_roundtrip(&mut self, label: &str, m: &keel_ir::Module, format: &str) -> Result<()> {
        self.execute(label, native(m)?)?;
        // Close the cycle in the original input IR as well as in Keel.
        match format {
            "llvm" => {
                let text = keel_llvm::export(m, keel_llvm::TargetProfile::AppleArm64)?;
                let bytes = self.llvm_object(&format!("{label}-closed"), &text)?;
                self.execute(&format!("{label}-closed"), bytes)
            }
            "clif" => {
                write(
                    &self.out.join(format!("{label}-closed.clif")),
                    keel_cranelift::translate_to_clif(m, &options())?,
                )?;
                self.execute(
                    &format!("{label}-closed"),
                    keel_cranelift::compile_object(m, &options())?,
                )
            }
            _ => Ok(()),
        }
    }
}
fn run() -> Result<()> {
    let mut args = BTreeMap::new();
    let mut iter = std::env::args().skip(1);
    while let Some(key) = iter.next() {
        if key == "--help" {
            println!(
                "keel-ir-harness --target {TARGET} --format keel|llvm|clif --input FILE --name SYMBOL --driver FILE.c --expected FILE --out NEW_DIR --libllvm FILE --clif-import BIN [--clang BIN]"
            );
            return Ok(());
        }
        let key = key
            .strip_prefix("--")
            .ok_or("expected --option")?
            .to_string();
        if ![
            "target",
            "format",
            "input",
            "name",
            "driver",
            "expected",
            "out",
            "libllvm",
            "keel",
            "clif-import",
            "clang",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unknown option --{key}"));
        }
        let value = iter.next().ok_or("missing option value")?;
        if args.insert(key, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    if args.get("target").map(String::as_str) != Some(TARGET) {
        return Err(format!("explicit --target {TARGET} required"));
    }
    let out = PathBuf::from(args.get("out").ok_or("--out required")?);
    let expected =
        fs::read(args.get("expected").ok_or("--expected required")?).map_err(|e| e.to_string())?;
    fs::create_dir(&out).map_err(|e| format!("fresh output directory required: {e}"))?;
    let mut h = Harness {
        importer: None,
        args,
        out,
        commands: Vec::new(),
        steps: Vec::new(),
        expected,
    };
    let result = h.run();
    let report = json!({"schema":1, "target":TARGET, "arguments":h.args, "steps":h.steps, "commands":h.commands,
        "fatal":result.as_ref().err(), "loss_contracts":{
            "llvm_to_keel":"defined-input refinement; overflow, pointer, optimizer and debug facts may be dropped; see suite/llvm_import_README.md",
            "llvm_scalar_deferred_values":"poison/undef operands choose zero; freeze keeps the concretized operand; nondeterminism is discarded, not modeled",
            "keel_to_llvm":"notrap and restrict facts omitted; see crates/keel-llvm/README.md",
            "clif_to_keel":"explicit external symbol; scalar single-function profile; source locations not retained",
            "measurement":"execution against fixture oracle; fact preservation is not automatically inferred"}});
    write(
        &h.out.join("report.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )?;
    result?;
    if h.steps.iter().any(|s| s["status"] != "passed") {
        return Err("one or more paths failed; see report.json".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
