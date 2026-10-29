use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
pub const EMPTY: keel_codegen::regalloc::Pools = keel_codegen::regalloc::Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
pub fn compile(
    m: &keel_ir::Module,
    selector: &dyn keel_codegen::InstructionSelector,
    allocator: &dyn keel_codegen::regalloc::RegisterAllocator,
    pools: &keel_codegen::regalloc::Pools,
) -> (String, Vec<u8>) {
    let pipeline = keel_codegen::CodegenPipeline {
        selector,
        allocator,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let p = keel_codegen::aarch64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    assert_eq!(p.stats().text_bytes, 0);
    let obj = keel_asm::assemble_program_object(&p, Default::default()).unwrap();
    assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
    (text, obj)
}
/// One test's scratch directory. Each check compares `__text` with clang's
/// assembly of the same text, then links the object with its C oracle. Each
/// distinct oracle source compiles once, at `-O2 -Wall -Wextra -Werror`.
pub struct Oracle {
    dir: PathBuf,
    compiled: HashMap<String, PathBuf>,
}

impl Oracle {
    pub fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "keel-arm-narrow-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            compiled: HashMap::new(),
        }
    }

    pub fn check(&mut self, context: &str, text: &str, obj: &[u8], c: &str) {
        use object::{Object, ObjectSection};
        let dir = &self.dir;
        fs::write(dir.join("code.s"), text).unwrap();
        fs::write(dir.join("code.o"), obj).unwrap();
        run(
            Command::new("clang")
                .args(["-target", "arm64-apple-macos11", "-c"])
                .arg(dir.join("code.s"))
                .arg("-o")
                .arg(dir.join("clang.o")),
            context,
        );
        let reference = fs::read(dir.join("clang.o")).unwrap();
        let actual = object::File::parse(obj).unwrap();
        let expected = object::File::parse(reference.as_slice()).unwrap();
        assert_eq!(
            actual.section_by_name("__text").unwrap().data().unwrap(),
            expected.section_by_name("__text").unwrap().data().unwrap(),
            "{context}"
        );
        if !cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            return;
        }
        let next = self.compiled.len();
        let oracle = self.compiled.entry(c.to_owned()).or_insert_with(|| {
            let source = dir.join(format!("oracle{next}.c"));
            let object = dir.join(format!("oracle{next}.o"));
            fs::write(&source, c).unwrap();
            run(
                Command::new("clang")
                    .args(["-O2", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(&source)
                    .arg("-o")
                    .arg(&object),
                context,
            );
            object
        });
        run(
            Command::new("clang")
                .arg(&*oracle)
                .arg(dir.join("code.o"))
                .arg("-o")
                .arg(dir.join("native")),
            context,
        );
        run(&mut Command::new(dir.join("native")), context);
    }
}

impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn run(cmd: &mut Command, context: &str) {
    let o = cmd.output().unwrap();
    assert!(o.status.success(), "{context}: {cmd:?}: {o:?}");
}
