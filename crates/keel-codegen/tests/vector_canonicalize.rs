//! Optimizer-to-selector contract: bit transport, arbitrary nonzero masks and
//! low-lane extension, with text/direct object parity on both targets.
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};
use object::{Object, ObjectSection};
use std::{fs, process::Command};

const SOURCE: &str = "
func @PREFIXtransport(ptr,ptr) {
block0(v0: ptr,v1: ptr):
 v2 = load.f32x4 align 1 v0
 v3 = vextract v2,3
 v4 = vinsert v2,v3,3
 v5 = bitcast.i32x4 v4
 v6 = bitcast.f32x4 v5
 store align 1 v6,v1
 return
}
func @PREFIXshuffle(ptr,ptr,ptr) {
block0(v0: ptr,v1: ptr,v2: ptr):
 v3 = load.i64x2 align 1 v0
 v4 = load.i64x2 align 1 v1
 v5 = vshuffle v3,v4,[1,2]
 v6 = vshuffle v5,v3,[1,0]
 store align 1 v6,v2
 return
}
func @PREFIXmask(ptr,ptr,ptr) {
block0(v0: ptr,v1: ptr,v2: ptr):
 v3 = load.f32x4 align 1 v0
 v4 = load.f32x4 align 1 v1
 v5 = iconst.i32 1
 v6 = vsplat.i32x4 v5
 v7 = vselect v6,v3,v4
 store align 1 v7,v2
 return
}
func @PREFIXextend(ptr,ptr) {
block0(v0: ptr,v1: ptr):
 v2 = load.i8x16 align 1 v0
 v3 = uwiden_low.i16x8 v2
 v4 = swiden_low.i64x2 v3
 store align 1 v4,v1
 return
}";

const ORACLE: &str = r#"
extern void before_transport(const void*,void*),after_transport(const void*,void*);
extern void before_shuffle(const void*,const void*,void*),after_shuffle(const void*,const void*,void*);
extern void before_mask(const void*,const void*,void*),after_mask(const void*,const void*,void*);
extern void before_extend(const void*,void*),after_extend(const void*,void*);
int main(void) {
 // Zero, signed zero, infinities, quiet/signaling NaNs and payloads: no FP math.
 const unsigned patterns[]={0,0x80000000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,0xffffffffu,1};
 for(unsigned t=0;t<256;t++) {
  unsigned a[4],b[4],x[4],y[4];
  for(unsigned i=0;i<4;i++) {a[i]=patterns[(t+i)%8];b[i]=patterns[(t/8+i+2)%8];}
  before_transport(a,x);after_transport(a,y);
  for(unsigned i=0;i<4;i++) if(x[i]!=a[i]||y[i]!=a[i]) return 1;
  before_mask(a,b,x);after_mask(a,b,y);
  for(unsigned i=0;i<4;i++) if(x[i]!=a[i]||y[i]!=a[i]) return 2;
  before_shuffle(a,b,x);after_shuffle(a,b,y);
  for(unsigned i=0;i<4;i++) {unsigned e=i<2?b[i]:a[i];if(x[i]!=e||y[i]!=e)return 3;}
  unsigned char bytes[16]; unsigned long long wide_x[2],wide_y[2];
  for(unsigned i=0;i<16;i++)bytes[i]=(unsigned char)(t+i);
  before_extend(bytes,wide_x);after_extend(bytes,wide_y);
  for(unsigned i=0;i<2;i++)if(wide_x[i]!=bytes[i]||wide_y[i]!=bytes[i])return 4;
 }
 return 0;
}
"#;

fn command(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(out.status.success(), "{c:?}: {out:?}");
}

#[test]
fn canonical_forms_compile_and_preserve_bits() {
    check_forms(SOURCE, ORACLE, "canonical", true);
}

// Nontrivial composition must preserve these packed native recipes.
#[test]
fn nested_shuffle_selector_boundaries_preserve_native_patterns() {
    check_forms(
        "func @PREFIXbytes(ptr,ptr,ptr) {
block0(v0: ptr,v1: ptr,v2: ptr):
 v3 = load.i8x16 align 1 v0
 v4 = load.i8x16 align 1 v1
 v5 = vshuffle v3,v4,[0,16,1,17,2,18,3,19,4,20,5,21,6,22,7,23]
 v6 = vshuffle v5,v3,[0,16,1,17,2,18,3,19,4,20,5,21,6,22,7,23]
 store align 1 v6,v2
 return
}
func @PREFIXdwords(ptr,ptr,ptr) {
block0(v0: ptr,v1: ptr,v2: ptr):
 v3 = load.i32x4 align 1 v0
 v4 = load.i32x4 align 1 v1
 v5 = vshuffle v3,v4,[0,1,4,5]
 v6 = vshuffle v3,v4,[2,3,6,7]
 v7 = vshuffle v5,v6,[0,2,4,6]
 store align 1 v7,v2
 return
}",
        r#"
extern void before_bytes(const void*,const void*,void*),after_bytes(const void*,const void*,void*);
extern void before_dwords(const void*,const void*,void*),after_dwords(const void*,const void*,void*);
int main(void) {
 for(unsigned t=0;t<256;t++) {
  unsigned char a[16],b[16],x[16],y[16];
  for(unsigned i=0;i<16;i++){a[i]=(unsigned char)(t+i);b[i]=(unsigned char)(t-i-1);}
  before_bytes(a,b,x);after_bytes(a,b,y);
  for(unsigned i=0;i<16;i++) {
   unsigned j=i/2;
   unsigned char e=(i%2)?a[j]:((j%2)?b[j/2]:a[j/2]);
   if(x[i]!=e||y[i]!=e)return 1;
  }
  unsigned c[4],d[4],u[4],v[4];
  for(unsigned i=0;i<4;i++){c[i]=t*65537+i;d[i]=~c[i];}
  before_dwords(c,d,u);after_dwords(c,d,v);
  for(unsigned i=0;i<4;i++) {
   unsigned e=(i%2)?d[(i/2)*2]:c[(i/2)*2];
   if(u[i]!=e||v[i]!=e)return 2;
  }
 }
 return 0;
}
"#,
        "boundaries",
        false,
    );
}

fn check_forms(source: &str, oracle: &str, evidence: &str, shrinks: bool) {
    let before = keel_ir::parse(&source.replace("PREFIX", "before_")).unwrap();
    let mut after = keel_ir::parse(&source.replace("PREFIX", "after_")).unwrap();
    keel_opt::run_passes(&mut after, &["instcombine", "gvn", "instcombine", "dce"]).unwrap();
    keel_ir::verify_module(&after).unwrap();
    if evidence == "boundaries" {
        assert_eq!(
            before.to_string().replace("before_", "after_"),
            after.to_string()
        );
    }
    let root = std::env::var_os("KEEL_VECTOR_CANONICAL_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-vector-canonical-{}", std::process::id()))
        })
        .join(evidence);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("before.keel"), before.to_string()).unwrap();
    fs::write(root.join("after.keel"), after.to_string()).unwrap();
    fs::write(root.join("oracle.c"), oracle).unwrap();
    for arm in [false, true] {
        for selector in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let stem = format!(
                    "{}-{selector:?}-{allocator:?}",
                    if arm { "arm" } else { "x86" }
                );
                let mut objects = Vec::new();
                let mut sizes = Vec::new();
                for (label, m) in [("before", &before), ("after", &after)] {
                    let (text, obj) = if arm {
                        let text = keel_codegen::aarch64::compile_with_pipeline(
                            m,
                            &keel_codegen::aarch64::REGISTERS,
                            &pipeline,
                        )
                        .unwrap();
                        let p = keel_codegen::aarch64::compile_program_with_pipeline(
                            m,
                            &keel_codegen::aarch64::REGISTERS,
                            &pipeline,
                        )
                        .unwrap();
                        let obj =
                            keel_asm::assemble_program_object(&p, Default::default()).unwrap();
                        assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
                        (text, obj)
                    } else {
                        let text = keel_codegen::x86_64::compile_with_pipeline(
                            m,
                            &keel_codegen::x86_64::REGISTERS,
                            &pipeline,
                        )
                        .unwrap();
                        let p = keel_codegen::x86_64::compile_program_with_pipeline(
                            m,
                            &keel_codegen::x86_64::REGISTERS,
                            &pipeline,
                        )
                        .unwrap();
                        let obj = keel_asm::x86::assemble_program_object(&p).unwrap();
                        assert_eq!(obj, keel_asm::x86::assemble_object(&text).unwrap());
                        (text, obj)
                    };
                    if !arm && evidence == "canonical" && label == "after" {
                        assert!(text.contains("shufpd"), "combined qword selection: {text}");
                    }
                    if evidence == "boundaries" {
                        assert!(text.contains(if arm { "zip1" } else { "shufps" }));
                    }
                    let parsed = object::File::parse(obj.as_slice()).unwrap();
                    let section = parsed
                        .section_by_name(if arm { "__text" } else { ".text" })
                        .unwrap();
                    println!("{evidence} {stem} {label} text_bytes={}", section.size());
                    sizes.push(section.size());
                    let name = root.join(format!("{stem}-{label}"));
                    fs::write(name.with_extension("s"), text).unwrap();
                    fs::write(name.with_extension("o"), obj).unwrap();
                    // An independent assembler must accept both emitted forms.
                    command(
                        Command::new("clang")
                            .args([
                                "-target",
                                if arm {
                                    "arm64-apple-macos11"
                                } else {
                                    "x86_64-unknown-linux-gnu"
                                },
                                "-c",
                            ])
                            .arg(name.with_extension("s"))
                            .arg("-o")
                            .arg(name.with_extension("clang.o")),
                    );
                    objects.push(name.with_extension("o"));
                }
                if evidence == "boundaries" {
                    assert_eq!(sizes[0], sizes[1], "native recipes must retain their size");
                }
                assert!(
                    !shrinks || sizes[1] < sizes[0],
                    "representative forms should shrink: {stem} {sizes:?}"
                );
                let native = arm;
                if native {
                    let exe = root.join(format!("{stem}.native"));
                    command(
                        Command::new("clang")
                            .args(["-O2", "-Wall", "-Wextra", "-Werror"])
                            .arg(root.join("oracle.c"))
                            .args(&objects)
                            .arg("-o")
                            .arg(&exe),
                    );
                    command(&mut Command::new(exe));
                }
            }
        }
    }
}
