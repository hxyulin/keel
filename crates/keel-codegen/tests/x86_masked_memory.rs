//! AVX/AVX2 masked loads/stores read/write only lanes with a set mask sign bit.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{Intrinsic, Type};
use std::{fs, path::PathBuf, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Case {
    id: Intrinsic,
    alias: bool,
}
impl Case {
    fn ty(&self) -> Type {
        let i = self.id.info();
        if self.id.is_x86_masked_store() {
            i.params[1]
        } else {
            i.returns[0]
        }
    }
    fn name(&self) -> String {
        format!("masked_{}_{}", self.id, self.alias)
    }
}
fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for id in Intrinsic::ALL.into_iter().filter(|i| i.is_x86_masked_mem()) {
        v.push(Case { id, alias: false });
        if id.is_x86_masked_store() {
            v.push(Case { id, alias: true });
        }
    }
    v
}
fn source() -> String {
    let mut s = String::new();
    for c in cases() {
        let ty = c.ty();
        let mask = Type::Vector(ty.vector().unwrap().mask());
        let bytes = ty.bytes().unwrap();
        s += &format!(
            "func @{}(ptr,ptr,ptr,ptr){{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n v4 = load.{ty} align 1 v1\n v5 = load.{mask} align 1 v2\n v6 = bitcast.{mask} v4\n",
            c.name()
        );
        if c.id.is_x86_masked_store() {
            s += &format!(
                " intrinsic {}(v0,v4,{})\n store align 1 v4,v3\n v10 = iconst.i64 {bytes}\n v11 = ptradd v3,v10\n store align 1 v5,v11\n return\n}}\n",
                c.id,
                if c.alias { "v6" } else { "v5" }
            );
        } else {
            s += &format!(
                " v7 = intrinsic {}(v0,v5)\n store align 1 v7,v3\n v10 = iconst.i64 {bytes}\n v11 = ptradd v3,v10\n store align 1 v4,v11\n v12 = ptradd v11,v10\n store align 1 v5,v12\n return\n}}\n",
                c.id
            );
        }
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n#ifdef __linux__\nextern long sysconf(int);extern void*mmap(void*,__SIZE_TYPE__,int,int,int,long);extern int mprotect(void*,__SIZE_TYPE__,int);extern int munmap(void*,__SIZE_TYPE__);extern int fork(void);extern int waitpid(int,int*,int);extern void _exit(int);struct Limit{unsigned long a,b;};extern int setrlimit(int,const struct Limit*);\n#endif\n",
    );
    let mut table = String::from(
        "struct Case{void(*run)(void*,const void*,const void*,void*);unsigned bits,bytes,n,store,alias;const char*name;};static const struct Case cases[]={\n",
    );
    for case in cases() {
        let shape = case.ty().vector().unwrap();
        let name = case.name();
        c += &format!("extern void {name}(void*,const void*,const void*,void*);\n");
        table += &format!(
            "{{{name},{},{},{},{},{},\"{name}\"}},\n",
            shape.lane_type().bytes().unwrap() * 8,
            shape.bytes(),
            shape.lanes(),
            u8::from(case.id.is_x86_masked_store()),
            u8::from(case.alias)
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
static uint64_t get(const unsigned char*p,unsigned bits){uint64_t v=0;memcpy(&v,p,bits/8);return v;}
static void put(unsigned char*p,unsigned bits,uint64_t v){memcpy(p,&v,bits/8);}
int main(void){
#ifdef __linux__
 struct Limit no_core={0,0};setrlimit(4,&no_core);unsigned long page=(unsigned long)sysconf(30);unsigned char*a=mmap(0,3*page,3,0x22,-1,0);if(a==(void*)-1)return 90;if(mprotect(a,page,0)||mprotect(a+2*page,page,0))return 91;
 for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++){const struct Case*c=&cases[k];unsigned char data[32],mask[32],out[130],e[96],window[64];for(unsigned valid=0;valid<=c->n;valid++)for(unsigned offset=0;offset<c->bits/8;offset++)for(unsigned pattern=0;pattern<(1u<<c->n);pattern++){
 unsigned char*end=a+2*page,*base=end-valid*c->bits/8-offset,*start=end-64;for(unsigned i=0;i<64;i++)start[i]=i*17+31;memcpy(window,start,64);for(unsigned i=0;i<32;i++){data[i]=i*31+19;mask[i]=i*7+1;}
 for(unsigned i=0;i<c->n;i++){uint64_t m=(0x123456789abcdefULL&(~0ULL>>(65-c->bits)))|((uint64_t)(i<valid?((pattern>>i)&1):0)<<(c->bits-1));put(c->alias?data+i*c->bits/8:mask+i*c->bits/8,c->bits,m);}
 memset(e,0,96);if(c->store){memcpy(e,data,c->bytes);memcpy(e+c->bytes,mask,c->bytes);}else{memcpy(e+c->bytes,data,c->bytes);memcpy(e+2*c->bytes,mask,c->bytes);}
 const unsigned char*mm=c->alias?data:mask;for(unsigned i=0;i<c->n;i++)if(get(mm+i*c->bits/8,c->bits)>>(c->bits-1)){if(c->store)memcpy(window+(base-start)+i*c->bits/8,data+i*c->bits/8,c->bits/8);else memcpy(e+i*c->bits/8,base+i*c->bits/8,c->bits/8);}
 memset(out,0x5a,130);c->run(base,data,mask,out+1);unsigned size=(c->store?2:3)*c->bytes;if(memcmp(out+1,e,size)||out[0]!=0x5a||out[1+size]!=0x5a||memcmp(start,window,64)){printf("failed %s valid%u off%u pat%u\n",c->name,valid,offset,pattern);return 10;}
 }
 // All-zero masks must suppress access even when the entire base is protected.
 memset(data,0,32);memset(mask,0,32);c->run(a,data,mask,out);
 // A selected scalar crossing into protected memory must fault.
 put(c->alias?data:mask,c->bits,1ULL<<(c->bits-1));int pid=fork();if(pid<0)return 92;if(pid==0){c->run(a,data,mask,out);_exit(0);}int status=0;if(waitpid(pid,&status,0)!=pid)return 93;int signal=status&127;if(signal!=11&&signal!=7){printf("missing active fault %s status%d\n",c->name,status);return 11;}
 }
 munmap(a,3*page);
#endif
 return 0;}
"#;
    c
}
fn compile(
    m: &keel_ir::Module,
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let pools = if empty {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline)?;
    let p = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline)?;
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, f).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    Ok((b, text))
}
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    compile(&m, s, a, empty, f)
}
#[test]
fn masked_memory_typed_alias_spill_exports() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("masked-memory.ir"), source()).unwrap();
        fs::write(d.join("masked-memory.c"), harness()).unwrap();
    }
    let d = common::temp("masked-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("masked.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-fsyntax-only",
            ])
            .arg(d.join("masked.c")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, Features::for_level(CpuLevel::V3)).unwrap();
                if let Some(d) = &export {
                    let name = format!("masked-memory-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn masked_memory_feature_and_operand_validation() {
    let mut f = Features::NONE;
    f = f.with_features("avx").unwrap();
    assert!(object(SelectionTier::Baseline, AllocationTier::Full, false, f).is_err());
    let mut m = keel_ir::parse(&source()).unwrap();
    m.functions
        .retain(|f| f.name.starts_with("masked_x86_avx_"));
    compile(&m, SelectionTier::Full, AllocationTier::Full, true, f).unwrap();
    assert!(
        compile(
            &m,
            SelectionTier::Baseline,
            AllocationTier::Full,
            false,
            Features::NONE
        )
        .is_err()
    );
    assert!(
        keel_codegen::aarch64::compile(
            &keel_ir::parse(
                source()
                    .split("func @masked_x86_avx_vmaskmovps_load256")
                    .next()
                    .unwrap()
            )
            .unwrap()
        )
        .is_err()
    );
    let bad = source().replace("load.i32x4", "load.i64x2");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_masked_memory_fault_suppression_and_exact_lanes() {
    if !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let d = common::temp("masked-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("masked.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V3)).unwrap();
                fs::write(d.join("masked.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2"])
                        .arg(d.join("masked.c"))
                        .arg(d.join("masked.o"))
                        .arg("-o")
                        .arg(d.join("masked")),
                );
                common::command(&mut Command::new(d.join("masked")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
