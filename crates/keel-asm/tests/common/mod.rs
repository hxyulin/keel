use object::{Object, ObjectSection};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "keel-asm-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub fn run(command: &mut Command) {
    let output = command
        .output()
        .expect("clang is required for assembler differential tests");
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
pub fn compare_clang(source: &str) {
    compare_clang_with_features(source, keel_asm::TargetFeatures::default(), &[]);
}

/// Optional ISA flags apply only to the reference assembly in this comparison.
/// They do not change the default target or enable host execution.
pub fn compare_clang_with_features(
    source: &str,
    features: keel_asm::TargetFeatures,
    flags: &[&str],
) {
    let temp = Temp::new();
    let input = temp.0.join("input.s");
    let output = temp.0.join("clang.o");
    std::fs::write(&input, source).unwrap();
    run(Command::new("clang")
        .args(["-target", "arm64-apple-macos11", "-c"])
        .args(flags)
        .arg(input)
        .arg("-o")
        .arg(&output));
    let bytes = std::fs::read(output).unwrap();
    let reference = object::File::parse(bytes.as_slice()).unwrap();
    let reference = reference.section_by_name("__text").unwrap();
    let assembly = keel_asm::assemble_with_features(source, features).unwrap();
    assert_eq!(
        assembly.text,
        reference.data().unwrap(),
        "instruction bytes differ from clang"
    );
    assert!((1u64 << assembly.text_alignment) >= reference.align());
}

/// Generated loops must fail a test rather than leave its process running.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn execute(path: &std::path::Path) -> std::process::ExitStatus {
    use std::time::{Duration, Instant};
    let mut child = Command::new(path).spawn().unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if start.elapsed() >= Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{} still running after 10 seconds", path.display());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
