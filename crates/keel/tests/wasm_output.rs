use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "keel-wasm-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
    fn no_temporary(&self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            assert!(
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".keel-wasm-")
            );
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn command(kind: &str) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_keel"));
    c.args([kind, "--target", "wasm32-unknown-unknown", "--emit-wasm"]);
    c
}
fn compile(kind: &str, input: &Path, output: &Path, flags: &[&str]) -> Output {
    command(kind)
        .args(flags)
        .arg(input)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap()
}
fn success(result: Output) {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
}
fn failure(result: Output) -> String {
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    String::from_utf8(result.stderr).unwrap()
}
fn node(path: &Path, script: &str) {
    let out = Command::new("node")
        .args(["-e", script])
        .arg(path)
        .output()
        .expect("existing Node");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
const C: &str = r#"
#include <stdint.h>
#include <stddef.h>
_Static_assert(sizeof(long)==4 && sizeof(void*)==4 && sizeof(size_t)==4, "ILP32");
#if defined(__APPLE__) || defined(__linux__) || defined(__x86_64__) || defined(__aarch64__) || __STDC_HOSTED__
#error host contamination
#endif
static int numbers[3] = {11,22,33};
static int *link = numbers+1;
static unsigned long helper(unsigned long x) { return x*3+1; }
unsigned long checksum(unsigned int address, int length) {
    unsigned char *input = (unsigned char *)(uintptr_t)address;
    unsigned char local[16];
    unsigned long sum=7;
    for(int i=0;i<16;i++) local[i]=(unsigned char)(i^0x5a);
    for(int i=0;i<length;i++) sum=(sum*33)^input[i];
    for(int i=0;i<16;i++) sum^=(unsigned long)local[i]<<(i&7);
    return sum+helper(length)+*link;
}
int local_array(void) { int a[4]={7,0,3,4}; return a[0]+a[1]+a[2]+a[3]; }
int recurse(int n) { int a[2]={n,3}; if(n<=0) return a[1]; return a[0]+recurse(n-1); }
"#;
const EXPORTS: &[&str] = &[
    "--export",
    "checksum",
    "--export",
    "local_array",
    "--export",
    "recurse",
];
const RUNTIME: &str = r#"
const assert=require('assert/strict'),fs=require('fs');
const b=fs.readFileSync(process.argv[1]); assert(WebAssembly.validate(b));
const m=new WebAssembly.Module(b); assert.deepEqual(WebAssembly.Module.imports(m),[]);
assert.deepEqual(WebAssembly.Module.exports(m).map(e=>e.name).sort(),['checksum','local_array','memory','recurse']);
const e=new WebAssembly.Instance(m).exports, a=new Uint8Array(e.memory.buffer);
assert.equal(a.length,65536); assert.throws(()=>e.memory.grow(1),RangeError);
a.fill(0xcc,8191,8225); let want=7;
for(let i=0;i<32;i++){a[8192+i]=(i*17+129)&255;want=(Math.imul(want,33)^a[8192+i])>>>0;}
for(let i=0;i<16;i++)want=(want^((i^0x5a)<<(i&7)))>>>0;
want=(want+97+22)>>>0;
for(let i=0;i<100;i++){assert.equal(e.checksum(8192,32)>>>0,want);assert.equal(e.local_array(),14);assert.equal(e.recurse(20),213);}
assert.equal(a[8191],0xcc);assert.equal(a[8224],0xcc);
"#;

#[test]
fn raw_c_preprocessed_c_and_equivalent_raw_ir_execute() {
    let t = Temp::new();
    let c = t.write("input.c", C);
    let out = t.0.join("out.wasm");
    success(compile("cc", &c, &out, EXPORTS));
    node(&out, RUNTIME);
    let expected = std::fs::read(&out).unwrap();
    let pp = keel_c::preprocess(
        C,
        c.to_str().unwrap(),
        &keel_c::PreprocessOptions {
            target: keel_ir::Target::Wasm32UnknownUnknown,
            ..Default::default()
        },
    )
    .unwrap();
    let i = t.write("input.i", &pp);
    success(compile("cc", &i, &out, EXPORTS));
    assert_eq!(std::fs::read(&out).unwrap(), expected);
    let module = keel_c::compile(
        &pp,
        c.to_str().unwrap(),
        keel_c::Options {
            target: keel_ir::Target::Wasm32UnknownUnknown,
            debug_info: false,
        },
    )
    .unwrap();
    let ir = t.write("input.keel", &module.to_string());
    success(compile("compile", &ir, &out, EXPORTS));
    assert_eq!(std::fs::read(&out).unwrap(), expected);
    node(&out, RUNTIME);
    // Both mappings are dropped before atomic replacement of the input itself.
    success(compile("compile", &ir, &ir, EXPORTS));
    assert_eq!(std::fs::read(&ir).unwrap(), expected);
    success(compile("cc", &c, &c, EXPORTS));
    assert_eq!(std::fs::read(&c).unwrap(), expected);
    t.no_temporary();
}

#[test]
fn explicit_memory_and_no_implicit_function_exports() {
    let t = Temp::new();
    let c = t.write("a.c", "int answer(void){return 42;}");
    let out = t.0.join("a.wasm");
    success(compile("cc", &c, &out, &[]));
    node(
        &out,
        "const a=require('assert/strict'),m=new WebAssembly.Module(require('fs').readFileSync(process.argv[1]));a.deepEqual(WebAssembly.Module.exports(m),[{name:'memory',kind:'memory'}]);",
    );
    success(compile(
        "cc",
        &c,
        &out,
        &[
            "--export",
            "answer",
            "--wasm-memory-pages",
            "2",
            "--wasm-data-floor",
            "2048",
            "--wasm-stack-bottom",
            "65536",
            "--wasm-stack-top",
            "131072",
        ],
    ));
    node(
        &out,
        "const a=require('assert/strict'),e=new WebAssembly.Instance(new WebAssembly.Module(require('fs').readFileSync(process.argv[1]))).exports;a.equal(e.memory.buffer.byteLength,131072);a.equal(e.answer(),42);a.throws(()=>e.memory.grow(1),RangeError);",
    );
    t.no_temporary();
}

#[test]
fn rejected_options_never_load_inputs_or_create_reports() {
    let t = Temp::new();
    let missing = t.0.join("missing");
    let out = t.write("keep", "sentinel");
    let report = t.0.join("report.json");
    let flags: &[&[&str]] = &[
        &["-O"],
        &["-O0"],
        &["-p", "dce"],
        &["--opt-tier", "none"],
        &["--opt-tier", "full"],
        &["--regalloc", "fast"],
        &["--isel-tier", "baseline"],
        &["--isel", "none"],
        &["--tune", "generic"],
        &["--cpu-level", "legacy"],
        &["--target-features", "none"],
        &["--emit-object"],
        &["-g"],
        &["--verify-each"],
        &["--no-verify-each"],
        &["--system"],
        &["--emit-asm"],
        &["--import", "env.f"],
    ];
    for kind in ["cc", "compile"] {
        for flags in flags {
            for before in [false, true] {
                let mut c = Command::new(env!("CARGO_BIN_EXE_keel"));
                c.arg(kind);
                if before {
                    c.args(*flags);
                }
                c.args(["--target", "wasm32-unknown-unknown", "--emit-wasm"]);
                if !before {
                    c.args(*flags);
                }
                let error = failure(c.arg(&missing).arg("-o").arg(&out).output().unwrap());
                assert!(!error.contains("No such file"), "{kind} {flags:?}: {error}");
                assert_eq!(std::fs::read(&out).unwrap(), b"sentinel");
            }
        }
    }
    for flags in [
        &["-E"][..],
        &["-dM"],
        &["--emit-ir"],
        &["--sysroot", "/missing"],
        &["--sysroot=/missing"],
        &["-isysroot", "/missing"],
        &["-mmacosx-version-min=99"],
    ] {
        let error = failure(compile("cc", &missing, &out, flags));
        assert!(!error.contains("No such file"), "{error}");
    }
    failure(
        command("compile")
            .arg("--opt-report")
            .arg(&report)
            .arg(&missing)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap(),
    );
    assert!(!report.exists());
    t.no_temporary();
}

#[test]
fn wasm_flags_require_explicit_wasm_output_and_target() {
    let t = Temp::new();
    let missing = t.0.join("missing");
    let out = t.write("keep", "sentinel");
    for kind in ["cc", "compile"] {
        for flags in [
            vec!["--emit-wasm"],
            vec!["--emit-wasm", "--target", "aarch64-apple-darwin"],
            vec!["--export", "answer"],
            vec!["--wasm-memory-pages", "1"],
            vec!["--target", "wasm32-unknown-unknown", "--export", "answer"],
            vec![
                "--target",
                "wasm32-unknown-unknown",
                "--wasm-stack-top",
                "65536",
            ],
        ] {
            failure(
                Command::new(env!("CARGO_BIN_EXE_keel"))
                    .arg(kind)
                    .args(flags)
                    .arg(&missing)
                    .arg("-o")
                    .arg(&out)
                    .output()
                    .unwrap(),
            );
            assert_eq!(std::fs::read(&out).unwrap(), b"sentinel");
        }
        for flags in [vec![], vec!["-o", "-"], vec!["-o", ""]] {
            failure(command(kind).args(flags).arg(&missing).output().unwrap());
        }
    }
    t.no_temporary();
}

#[test]
fn frontend_backend_and_memory_errors_preserve_outputs() {
    let t = Temp::new();
    let c = t.write("a.c", "int answer(void){return 42;}");
    let out = t.write("keep", "sentinel");
    let cases: &[&[&str]] = &[
        &["--export", "missing"],
        &["--export", "answer", "--export", "answer"],
        &["--export", "memory"],
        &["--export", ""],
        &["--wasm-memory-pages", "0"],
        &["--wasm-memory-pages", "65537"],
        &["--wasm-data-floor", "0"],
        &["--wasm-data-floor", "32769"],
        &["--wasm-stack-bottom", "32769"],
        &["--wasm-stack-bottom", "65552"],
        &["--wasm-stack-top", "65537"],
        &["--wasm-stack-top", "32752"],
        &["--wasm-memory-pages", "4294967296"],
        &["--wasm-memory-pages", "-1"],
        &["--wasm-memory-pages", "+1"],
        &["--wasm-memory-pages", "0x1"],
        &["--wasm-memory-pages", ""],
    ];
    for flags in cases {
        failure(compile("cc", &c, &out, flags));
        assert_eq!(std::fs::read(&out).unwrap(), b"sentinel");
        t.no_temporary();
    }
    let collision = t.write("memory.c", "int memory(void){return 0;}");
    assert!(failure(compile("cc", &collision, &out, &["--export", "memory"])).contains("collides"));
    for text in [
        "float f(void){return 1.0f;}",
        "extern int external(void);int answer(void){return external();}",
        "int broken(",
        "#include <stdio.h>\nint answer(void){return 0;}",
    ] {
        let c = t.write("bad.c", text);
        failure(compile("cc", &c, &out, &[]));
        assert_eq!(std::fs::read(&out).unwrap(), b"sentinel");
        t.no_temporary();
    }
    for text in [
        "func @bad() -> f32 {\nblock0:\n v0 = fconst.f32 1.0\n return v0\n}\n",
        "KEEL-IR-UNIT 4\ntarget aarch64-apple-darwin\nbytes 0\n\n",
        "target wasm32-unknown-unknown\n",
        "not IR",
    ] {
        let ir = t.write("bad.keel", text);
        failure(compile("compile", &ir, &out, &[]));
        assert_eq!(std::fs::read(&out).unwrap(), b"sentinel");
        t.no_temporary();
    }
}

#[test]
fn rename_and_create_failures_leave_destinations_and_clean_temporary() {
    let t = Temp::new();
    let c = t.write("a.c", "int answer(void){return 42;}");
    let dir = t.0.join("destination");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("sentinel"), b"keep").unwrap();
    failure(compile("cc", &c, &dir, &[]));
    assert_eq!(std::fs::read(dir.join("sentinel")).unwrap(), b"keep");
    t.no_temporary();
    failure(compile("cc", &c, &t.0.join("absent/out.wasm"), &[]));
    t.no_temporary();
}

#[test]
fn verified_unsupported_ir_never_publishes() {
    let t = Temp::new();
    let text = "func @f(f32) {\nblock0(v0: f32):\n return\n}\n";
    let module = keel_ir::parse(text).unwrap();
    keel_ir::verify_module_with_layout(&module, keel_ir::DataLayout::WASM32).unwrap();
    let ir = t.write("unsupported.keel", text);
    let out = t.write("keep", "sentinel");
    assert!(failure(compile("compile", &ir, &out, &[])).contains("unsupported scalar signature"));
    assert_eq!(std::fs::read(out).unwrap(), b"sentinel");
    t.no_temporary();
}

#[cfg(unix)]
#[test]
fn wasm_compilation_does_not_discover_or_spawn_native_tools() {
    use std::os::unix::fs::PermissionsExt;
    let t = Temp::new();
    let marker = t.0.join("driver-invoked");
    for name in ["clang", "cc", "xcrun", "ld", "wasm-ld"] {
        let tool = t.write(
            name,
            "#!/bin/sh\n: > \"$KEEL_WASM_DRIVER_MARKER\"\nexit 97\n",
        );
        std::fs::set_permissions(tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let c = t.write(
        "a.c",
        "#include <stdint.h>\nint answer(void){return sizeof(uintptr_t);}\n",
    );
    let ir = t.write(
        "a.keel",
        "func @answer() -> i32 {\nblock0:\n v0 = iconst.i32 4\n return v0\n}\n",
    );
    let out = t.0.join("a.wasm");
    for (kind, path) in [("cc", &c), ("compile", &ir)] {
        success(
            command(kind)
                .env("PATH", &t.0)
                .env("SDKROOT", "/missing")
                .env("KEEL_WASM_DRIVER_MARKER", &marker)
                .args(["--export", "answer"])
                .arg(path)
                .arg("-o")
                .arg(&out)
                .output()
                .unwrap(),
        );
        assert!(!marker.exists());
    }
    // Positive marker control establishes that the test tools are executable.
    assert!(
        !Command::new(t.0.join("clang"))
            .env("KEEL_WASM_DRIVER_MARKER", &marker)
            .status()
            .unwrap()
            .success()
    );
    assert!(marker.exists());
    t.no_temporary();
}

#[test]
fn explicit_headers_macros_and_ambient_host_isolation() {
    let t = Temp::new();
    let out = t.write("keep", "sentinel");
    t.write("custom.h", "#define CUSTOM 9\n");
    let c=t.write("a.c","#include <custom.h>\n#ifndef VALUE\n#error missing VALUE\n#endif\n#ifdef REMOVED\n#error override order\n#endif\nint answer(void){return CUSTOM+VALUE;}\n");
    success(
        command("cc")
            .arg("-I")
            .arg(&t.0)
            .args([
                "-DVALUE=1",
                "-UVALUE",
                "-DVALUE=33",
                "-DREMOVED=1",
                "-UREMOVED",
                "--export",
                "answer",
            ])
            .arg(&c)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap(),
    );
    node(
        &out,
        "require('assert/strict').equal(new WebAssembly.Instance(new WebAssembly.Module(require('fs').readFileSync(process.argv[1]))).exports.answer(),42);",
    );
    t.write("host-only.h", "#define HOST_ONLY 1\n");
    let host = t.write(
        "host.c",
        "#include <host-only.h>\nint answer(void){return HOST_ONLY;}\n",
    );
    std::fs::write(&out, b"sentinel").unwrap();
    failure(
        command("cc")
            .env("CPATH", &t.0)
            .env("C_INCLUDE_PATH", &t.0)
            .env("SDKROOT", &t.0)
            .env("MACOSX_DEPLOYMENT_TARGET", "99")
            .arg(&host)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap(),
    );
    assert_eq!(std::fs::read(&out).unwrap(), b"sentinel");
    let portable = t.write("portable.c", "int answer(void){return sizeof(long);}");
    success(
        command("cc")
            .env("SDKROOT", "/missing")
            .env("MACOSX_DEPLOYMENT_TARGET", "99")
            .arg(&portable)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap(),
    );
    for header in ["stdatomic.h", "immintrin.h", "keel/x86_intrinsics.h"] {
        let bad = t.write("bad.c", &format!("#include <{header}>\n"));
        failure(compile("cc", &bad, &out, &[]));
    }
    t.no_temporary();
}
