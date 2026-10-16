use keel_ir::{EntityRef, InstData, MemoryFlags, Module, Type};

fn source(op: &str, length: &str) -> String {
    let second = if op == "memset" { "i8" } else { "ptr" };
    let length = if length == "variable" {
        String::new()
    } else {
        format!("v2 = iconst.i64 {length}")
    };
    let params = if length.is_empty() { ", i64" } else { "" };
    let block = if length.is_empty() { ", v2: i64" } else { "" };
    format!(
        "func @f(ptr, {second}{params}) {{\nblock0(v0: ptr, v1: {second}{block}):\n{length}\nintrinsic {op}(v0, v1, v2)\nreturn\n}}"
    )
}

fn expand(source: &str) -> Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["small-memory"]).unwrap();
    assert!(
        !keel_opt::find_pass("small-memory")
            .unwrap()
            .run(&mut m.functions[0])
    );
    m
}

#[test]
fn all_small_lengths_have_exact_bounded_accesses_and_snapshot_order() {
    for op in ["memcpy", "memmove", "memset"] {
        for size in 0..=32 {
            let m = expand(&source(op, &size.to_string()));
            let f = &m.functions[0];
            let mut read = 0;
            let mut written = 0;
            let mut stores = 0;
            for (_, i) in f.layout_insts() {
                match f.inst_data(i) {
                    InstData::Load { ty, flags, .. } => {
                        assert_eq!(stores, 0, "all loads must precede stores");
                        assert_eq!(*flags, MemoryFlags::default());
                        read += bytes(*ty);
                    }
                    InstData::Store { value, flags, .. } => {
                        assert_eq!(*flags, MemoryFlags::default());
                        written += bytes(f.value_type(*value));
                        stores += 1;
                    }
                    InstData::Intrinsic { .. } => panic!("intrinsic remains"),
                    _ => {}
                }
            }
            assert_eq!(written, size);
            assert_eq!(read, if op == "memset" { 0 } else { size });
            assert!(stores <= 6);
            assert!(f.layout_insts().count() <= 50);
            assert_eq!(
                m.to_string(),
                expand(&source(op, &size.to_string())).to_string()
            );
        }
    }
}

fn bytes(ty: Type) -> usize {
    match ty {
        Type::I8 => 1,
        Type::I16 => 2,
        Type::I32 => 4,
        Type::I64 => 8,
        _ => panic!("unexpected type"),
    }
}

#[test]
fn variable_large_negative_and_wrapping_lengths_are_unchanged() {
    for op in ["memcpy", "memmove", "memset"] {
        for length in [
            "variable",
            "33",
            "64",
            "4096",
            "9223372036854775807",
            "-1",
            "-9223372036854775808",
        ] {
            let input = source(op, length);
            let original = keel_ir::parse(&input).unwrap();
            assert_eq!(expand(&input).to_string(), original.to_string());
        }
    }
}

#[test]
fn zero_preserves_operand_producers() {
    let input = "decl @dest() -> ptr\ndecl @src() -> ptr\ndecl @effect()\nfunc @f() {\nblock0:\nv0 = call @dest()\nv1 = call @src()\ncall @effect()\nv2 = iconst.i64 0\nintrinsic memmove(v0, v1, v2)\nreturn\n}";
    let mut m = expand(input);
    keel_opt::run_passes(&mut m, &["dce"]).unwrap();
    assert_eq!(
        m.functions[0]
            .layout_insts()
            .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Call { .. }))
            .count(),
        3
    );
}

#[test]
fn every_generated_instruction_preserves_source_location() {
    let mut m = keel_ir::parse(&source("memmove", "31")).unwrap();
    let loc = keel_ir::SourceLoc {
        file: keel_ir::SourceFile::new(0),
        line: 17,
        col: 4,
        scope: None,
    };
    let f = &mut m.functions[0];
    let at = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), InstData::Intrinsic { .. }))
        .unwrap()
        .1;
    f.set_inst_loc(at, Some(loc));
    let old: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    keel_opt::run_passes(&mut m, &["small-memory"]).unwrap();
    let f = &m.functions[0];
    for (_, i) in f.layout_insts().filter(|(_, i)| !old.contains(i)) {
        assert_eq!(f.inst_loc(i), Some(loc));
    }
}

/// Explicitly opt in after building the CLI; runs no nested cargo/build jobs.
/// KEEL_SMALL_MEMORY_BIN=/absolute/path/to/keel cargo test -p keel-opt
/// --test small_memory native_clang_differential -- --ignored
#[test]
#[ignore = "requires built KEEL_SMALL_MEMORY_BIN, native Clang and exclusive native lease"]
fn native_clang_differential() {
    use std::{fs, process::Command};
    let keel = std::env::var("KEEL_SMALL_MEMORY_BIN").expect("set KEEL_SMALL_MEMORY_BIN");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("small-memory-native-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let run = |cmd: &mut Command| {
        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "{cmd:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    let mut implementation = String::new();
    let mut declarations = String::new();
    let mut checks = String::new();
    let mut guards = String::new();
    for n in 0..=33 {
        for (op, tag, arg) in [
            ("memcpy", "cpy", "s"),
            ("memmove", "mov", "s"),
            ("memset", "set", "fill"),
        ] {
            implementation.push_str(&format!("void k{tag}{n}(unsigned char *d, unsigned char *s, int fill) {{ __builtin_{op}(d,{arg},{n}); }}\n"));
            for prefix in ["k", "r"] {
                declarations.push_str(&format!(
                    "void {prefix}{tag}{n}(unsigned char*,unsigned char*,int);\n"
                ));
            }
            checks.push_str(&format!(
                "check(k{tag}{n},r{tag}{n},{n},{});\n",
                if tag == "cpy" {
                    0
                } else if tag == "mov" {
                    1
                } else {
                    2
                }
            ));
            guards.push_str(&format!(
                "guard(k{tag}{n},{n},{});\n",
                if tag == "set" { 1 } else { 0 }
            ));
        }
    }
    // Side effects in all zero-length argument positions and low-byte fill
    // truncation are exercised through the C frontend, not assumed by IR tests.
    implementation.push_str("void kvariable(unsigned char *d,unsigned char *s,int fill,unsigned long n){__builtin_memmove(d,s,n);__builtin_memset(s,fill,n);__builtin_memcpy(d,s,n);}\n");
    declarations.push_str("void kvariable(unsigned char*,unsigned char*,int,unsigned long); void rvariable(unsigned char*,unsigned char*,int,unsigned long);\n");
    implementation.push_str("int count; unsigned char dummy[2]; unsigned char *dp(void){count=count*10+1;return dummy;} unsigned char *sp(void){count=count*10+2;return dummy+1;} unsigned long len(void){count=count*10+3;return 0;} int fl(void){count=count*10+2;return 511;} int zero(void){count=0;__builtin_memmove(dp(),sp(),len());if(count!=123)return 1;count=0;__builtin_memset(dp(),fl(),len());if(count!=123)return 2;count=0;__builtin_memcpy(dp(),sp(),len());if(count!=123)return 3;count=0;__builtin_memmove(dp(),sp(),0);if(count!=12)return 4;count=0;__builtin_memset(dp(),fl(),0);if(count!=12)return 5;count=0;__builtin_memcpy(dp(),sp(),0);return count!=12;}\n");
    fs::write(dir.join("impl.c"), &implementation).unwrap();
    let reference = implementation.replace("void k", "void r");
    // Only memory wrappers belong to the reference TU; side-effect helpers
    // belong to Keel and are independently checked by the driver.
    fs::write(
        dir.join("ref.c"),
        reference.split("int count;").next().unwrap(),
    )
    .unwrap();
    let driver = format!(
        r#"
#include <string.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
{declarations}
int zero(void);
typedef void (*Fn)(unsigned char*,unsigned char*,int);
static void check(Fn k, Fn r, int n, int mode) {{
 unsigned char a[192],b[192],s[192],t[192];
 int fills[]={{0,1,127,128,255,256,257,511,-1,-257}};
 for(int da=0;da<8;da++) for(int sa=0;sa<8;sa++)
 for(int fi=0;fi<10;fi++) {{
  for(int i=0;i<192;i++) a[i]=b[i]=(unsigned char)(i*37+19),s[i]=t[i]=(unsigned char)(i*23+7);
  k(a+64+da,s+64+sa,fills[fi]);r(b+64+da,t+64+sa,fills[fi]);
  if(memcmp(a,b,192)||memcmp(s,t,192))abort();
 }}
 if(mode==1) for(int delta=-33;delta<=33;delta++) for(int sa=0;sa<8;sa++) {{
  for(int i=0;i<192;i++) a[i]=b[i]=(unsigned char)(i*37+19);
  k(a+64+sa+delta,a+64+sa,0);r(b+64+sa+delta,b+64+sa,0);
  if(memcmp(a,b,192))abort();
 }}
 (void)n;
}}
static unsigned char *area(long p) {{
 unsigned char *a=mmap(0,(size_t)p*3,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(a==MAP_FAILED||mprotect(a+p,(size_t)p,PROT_READ|PROT_WRITE))abort();
 memset(a+p,73,(size_t)p);return a;
}}
static void guard(Fn k,int n,int set) {{
 long p=sysconf(_SC_PAGESIZE);unsigned char *a=area(p),*b=area(p);
 // Place the EXACT source and destination extents next to protected pages,
 // independently at each end. For n=0 pointers remain within valid objects.
 for(int end=0;end<2;end++) {{
  unsigned char *d=a+p+(end?p-(n?n:1):0),*s=b+p+(end?p-(n?n:1):0);
  k(d,s,511);
  for(int i=0;i<n;i++)if(d[i]!=(set?255:73))abort();
 }}
 munmap(a,(size_t)p*3);munmap(b,(size_t)p*3);
}}
int main(void) {{
 {checks} {guards}
 for(int n=0;n<=33;n++) {{
  unsigned char a[96],b[96],s[96],t[96];
  for(int i=0;i<96;i++)a[i]=b[i]=(unsigned char)i,s[i]=t[i]=(unsigned char)(i*13);
  kvariable(a+3,s+5,511,(unsigned long)n);rvariable(b+3,t+5,511,(unsigned long)n);
  if(memcmp(a,b,96)||memcmp(s,t,96))abort();
 }}
 return zero();
}}
"#
    );
    fs::write(dir.join("driver.c"), driver).unwrap();
    run(Command::new(&keel).current_dir(&dir).args([
        "cc",
        "-O",
        "-p",
        "canonicalize,instcombine,small-memory,dce",
        "impl.c",
        "-o",
        "impl.s",
    ]));
    for optimization in ["-O0", "-O2"] {
        run(Command::new("clang").current_dir(&dir).args([
            optimization,
            "-fno-builtin",
            "-c",
            "ref.c",
            "-o",
            "ref.o",
        ]));
        run(Command::new("clang").current_dir(&dir).args([
            optimization,
            "-fno-builtin",
            "driver.c",
            "impl.s",
            "ref.o",
            "-o",
            "check",
        ]));
        run(&mut Command::new(dir.join("check")));
    }
    // Both backend smoke routes use the same explicit named pass.
    run(Command::new(&keel).current_dir(&dir).args([
        "cc",
        "-O",
        "-p",
        "canonicalize,instcombine,small-memory,dce",
        "--target",
        "x86_64-unknown-linux-gnu",
        "impl.c",
        "-o",
        "x86.s",
    ]));
    let asm = fs::read_to_string(dir.join("impl.s")).unwrap();
    // Keep 33-byte fallbacks elsewhere, but require the mandatory 16B route
    // to contain no libc call. Labels delimit the next generated function.
    let body = asm
        .split("_kcpy16:")
        .nth(1)
        .expect("16B wrapper label")
        .split("_kmov16:")
        .next()
        .unwrap();
    assert!(
        !body.contains("bl\t_memcpy") && !body.contains("bl _memcpy"),
        "{body}"
    );
    eprintln!("native evidence retained at {}", dir.display());
}

#[test]
fn full_expands_small_memory_before_stack_forward_only() {
    use keel_opt::{DEFAULT_PIPELINE, OptimizationTier};
    assert_eq!(OptimizationTier::Full.pipeline(), DEFAULT_PIPELINE);
    let at = DEFAULT_PIPELINE
        .iter()
        .position(|p| *p == "small-memory")
        .unwrap();
    assert_eq!(DEFAULT_PIPELINE[at + 1], "stack-forward");
    assert_eq!(
        DEFAULT_PIPELINE
            .iter()
            .filter(|p| **p == "small-memory")
            .count(),
        1
    );
    for pass in ["memory-forward", "loop-memory", "loop-unroll"] {
        assert!(!DEFAULT_PIPELINE.contains(&pass));
    }
    assert!(OptimizationTier::None.pipeline().is_empty());
    assert_eq!(
        OptimizationTier::Basic.pipeline(),
        &[
            "simplify-cfg",
            "canonicalize",
            "instcombine",
            "call-contracts",
            "simplify-cfg",
            "dce",
        ]
    );
    for tier in [
        OptimizationTier::None,
        OptimizationTier::Basic,
        OptimizationTier::Full,
    ] {
        for length in ["16", "33", "variable"] {
            let mut module = keel_ir::parse(&source("memcpy", length)).unwrap();
            keel_opt::optimize_with_tier(&mut module, tier).unwrap();
            let retains = module.functions[0].layout_insts().any(|(_, i)| {
                matches!(module.functions[0].inst_data(i), InstData::Intrinsic { .. })
            });
            assert_eq!(retains, tier != OptimizationTier::Full || length != "16");
        }
    }
}
