//! Single-function CLIF importer and independent CLIF execution oracle.
use cranelift_module::{Linkage, Module as _};
use keel_cranelift::{OptLevel, Options, TargetProfile};
use std::{
    io::{Read, Write},
    path::Path,
};

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut name = None;
    let mut input = None;
    let mut target = None;
    let mut object = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => target = args.next().and_then(|s| TargetProfile::from_cli(&s)),
            "--name" => name = args.next(),
            "--object" => object = true,
            "--help" => {
                println!(
                    "keel-clif-import --target arm64-apple-macosx11.0.0 --name SYMBOL [--object] INPUT|-\nWrites verified Keel text or an independently compiled CLIF object to stdout."
                );
                return Ok(());
            }
            s if s.starts_with('-') && s != "-" => return Err(format!("unknown option {s}")),
            _ => {
                if input.replace(arg).is_some() {
                    return Err("multiple inputs".into());
                }
            }
        }
    }
    let options = Options {
        target: target.ok_or("explicit supported --target required")?,
        opt: OptLevel::None,
        verify_clif: true,
    };
    let name = name.ok_or("explicit --name required")?;
    let input = input.ok_or("input required")?;
    let mut source = String::new();
    if input == "-" {
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| e.to_string())?;
    } else {
        source = std::fs::read_to_string(Path::new(&input)).map_err(|e| e.to_string())?;
    }
    let bytes = if object {
        let f = keel_cranelift::import::parse(&source, &options)?;
        let isa = keel_cranelift::build_isa(&options)?;
        let builder = cranelift_object::ObjectBuilder::new(
            isa,
            "clif-oracle",
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| e.to_string())?;
        let mut module = cranelift_object::ObjectModule::new(builder);
        let id = module
            .declare_function(&name, Linkage::Export, &f.signature)
            .map_err(|e| e.to_string())?;
        let mut ctx = cranelift_codegen::Context::for_function(f);
        module
            .define_function(id, &mut ctx)
            .map_err(|e| e.to_string())?;
        module.finish().emit().map_err(|e| e.to_string())?
    } else {
        keel_cranelift::import::to_keel(&source, &name, &options)?.into_bytes()
    };
    std::io::stdout()
        .write_all(&bytes)
        .map_err(|e| e.to_string())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
