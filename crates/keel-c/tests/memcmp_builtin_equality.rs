//! Explicit builtin equality expansion and ordinary-call fallback contracts.
use keel_ir::{InstData, Target};
use std::{fs, process::Command};

fn compile(source: &str, target: Target) -> keel_ir::Module {
    let mut m = keel_c::compile(
        source,
        "memcmp-equality.c",
        keel_c::Options {
            target,
            ..Default::default()
        },
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn run(c: &mut Command) {
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{c:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn builtin_equality_native_guard_pages_and_both_target_objects() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("memcmp-builtin-native");
    fs::create_dir_all(&dir).unwrap();
    let mut source = String::from(
        "int memcmp(const void*,const void*,unsigned long); int ordinary(const void *a,const void *b){return memcmp(a,b,16)==0;} int general(const void*a,const void*b){return __builtin_memcmp(a,b,16);} int order(const void*a,const void*b){return __builtin_memcmp(a,b,16)<0;} int variable(const void*a,const void*b,unsigned long n){return __builtin_memcmp(a,b,n)==0;}\n",
    );
    let mut declarations = String::new();
    let mut checks = String::new();
    for n in 0..=33 {
        for (name, op) in [("eq", "=="), ("ne", "!=")] {
            source.push_str(&format!("int {name}{n}(const void*a,const void*b){{return __builtin_memcmp(a,b,{n}){op}0;}}\n"));
            declarations.push_str(&format!("int {name}{n}(const void*,const void*);\n"));
        }
        checks.push_str(&format!("check(eq{n},ne{n},{n});\n"));
    }
    source.push_str("struct Value { double number; long tag; }; struct Entry { struct Value val; }; int comparator16(struct Entry *ap, struct Entry *bp) { return !__builtin_memcmp(&ap->val,&bp->val,sizeof(ap->val)); }\n");
    source.push_str("int effects; unsigned char z[1]; void *a(void){effects=effects*10+1;return z;} void *b(void){effects=effects*10+2;return z;} unsigned long zero(void){effects=effects*10+3;return 0;} int side(void){effects=0;int r=__builtin_memcmp(a(),b(),(zero(),0))==0;return r&&effects==123;}\n");
    fs::write(dir.join("impl.c"), &source).unwrap();
    for target in Target::NATIVE {
        let m = compile(&source, target);
        for n in 0..=33 {
            for name in ["eq", "ne"] {
                let f = m
                    .functions
                    .iter()
                    .find(|f| f.name == format!("{name}{n}"))
                    .unwrap();
                assert_eq!(f.layout_insts().any(|(_,i)| matches!(f.inst_data(i),InstData::Intrinsic{id,..} if id.info().name=="memcmp")),n>32,"{name}{n}");
                if n <= 32 {
                    let read: u32 = f
                        .layout_insts()
                        .filter_map(|(_, i)| match f.inst_data(i) {
                            InstData::Load { ty, .. } => Some(ty.bytes().unwrap()),
                            _ => None,
                        })
                        .sum();
                    assert_eq!(read, n * 2);
                }
            }
        }
        for name in ["general", "order", "variable"] {
            let f = m.functions.iter().find(|f| f.name == name).unwrap();
            assert!(f.layout_insts().any(|(_,i)|matches!(f.inst_data(i),InstData::Intrinsic{id,..} if id.info().name=="memcmp")),"{name}");
        }
        let f = m.functions.iter().find(|f| f.name == "ordinary").unwrap();
        assert!(f.layout_insts().any(|(_,i)|matches!(f.inst_data(i),InstData::Call{func,..} if f.callee(*func).name=="memcmp")));
        let assembly = dir.join(format!("{}.s", target.triple()));
        fs::write(&assembly, keel_codegen::compile(&m, target).unwrap()).unwrap();
        run(Command::new("clang")
            .args(["-target", target.triple(), "-c"])
            .arg(&assembly)
            .arg("-o")
            .arg(assembly.with_extension("o")));
        // Also exercise backend fallback without the optimizing pipeline.
        let raw = keel_c::compile(
            &source,
            "raw.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        fs::write(
            &assembly.with_extension("raw.s"),
            keel_codegen::compile(&raw, target).unwrap(),
        )
        .unwrap();
        run(Command::new("clang")
            .args(["-target", target.triple(), "-c"])
            .arg(assembly.with_extension("raw.s"))
            .arg("-o")
            .arg(assembly.with_extension("raw.o")));
    }
    let driver = format!(
        r#"
#include <sys/mman.h>
#include <unistd.h>
#include <stdlib.h>
#include <string.h>
{declarations}
int comparator16(void*,void*);int side(void);int general(const void*,const void*);int order(const void*,const void*);int variable(const void*,const void*,unsigned long);
typedef int (*Fn)(const void*,const void*);
static unsigned char *area(long p) {{
 unsigned char *a=mmap(0,p*3,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(a==MAP_FAILED||mprotect(a+p,p,PROT_READ|PROT_WRITE))abort();return a;
}}
static void pair(Fn eq,Fn ne,unsigned char *a,unsigned char *b,int n) {{
 if(eq(a,b)!=1||ne(a,b)!=0)abort();
 for(int i=0;i<n;i++)for(int value=0;value<=255;value+=255) {{
  unsigned char old=b[i];b[i]=(unsigned char)value;
  int same=memcmp(a,b,n)==0;
  if(eq(a,b)!=same||ne(a,b)==same||eq(b,a)!=same||ne(b,a)==same)abort();
  b[i]=old;
 }}
}}
static void check(Fn eq,Fn ne,int n) {{
 long p=sysconf(_SC_PAGESIZE);unsigned char *a=area(p),*b=area(p);
 for(int end=0;end<2;end++) {{
  unsigned char *x=a+p+(end?p-n:0),*y=b+p+(end?p-n:0);
  for(int i=0;i<n;i++)x[i]=y[i]=(unsigned char)(i*37+73);
  pair(eq,ne,x,y,n);
 }}
 for(int da=0;da<8;da++)for(int sa=0;sa<8;sa++) {{
  unsigned char *x=a+p+32+da,*y=b+p+32+sa;
  for(int i=0;i<n;i++)x[i]=y[i]=(unsigned char)(i*37+73);
  pair(eq,ne,x,y,n);
 }}
 for(int delta=-32;delta<=32;delta++) {{
  unsigned char *x=a+p+64,*y=x+delta;
  for(int i=0;i<128;i++)a[p+i]=(unsigned char)(i*37);
  int same=memcmp(x,y,n)==0;
  if(eq(x,y)!=same||ne(x,y)==same)abort();
 }}
 munmap(a,p*3);munmap(b,p*3);
}}
int main(void) {{
 {checks}
 unsigned char a[16]={{0}},b[16]={{255}};
 if(comparator16(a,a)!=1||comparator16(a,b)!=0)abort();
 if(general(a,b)>=0||general(b,a)<=0||order(a,b)!=1||order(b,a)!=0||variable(a,b,0)!=1||variable(a,b,16)!=0)abort();
 return !side();
}}
"#
    );
    let driver_path = dir.join("driver.c");
    fs::write(&driver_path, driver).unwrap();
    for suffix in ["o", "raw.o"] {
        let obj = dir.join(format!(
            "{}.{suffix}",
            keel_c::Options::default().target.triple()
        ));
        let exe = dir.join(format!("check-{suffix}"));
        run(Command::new("clang")
            .arg(&driver_path)
            .arg(&obj)
            .arg("-o")
            .arg(&exe));
        run(&mut Command::new(&exe));
    }
}
