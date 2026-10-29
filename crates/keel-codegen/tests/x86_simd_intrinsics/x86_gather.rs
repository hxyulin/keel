//! Every AVX2 gather form: sign masks, signed VSIB indices, merging/zeroing,
//! fault suppression, destructive operand staging and source preservation.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::Intrinsic;
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Case {
    id: Intrinsic,
    scale: u8,
    alias: u8,
}
impl Case {
    fn name(&self) -> String {
        format!("g_{}_{}_{}", self.id, self.scale, self.alias)
    }
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for id in Intrinsic::ALL.into_iter().filter(|id| id.is_x86_gather()) {
        let p = id.info().params;
        let mut aliases = vec![0];
        if p[2] == p[3] {
            aliases.push(1);
        }
        if p[1] == p[2] {
            aliases.push(2);
        }
        for alias in aliases {
            for scale in [1, 2, 4, 8] {
                cases.push(Case { id, scale, alias });
            }
        }
    }
    cases
}
fn source() -> String {
    let mut s = String::new();
    for c in cases() {
        let p = c.id.info().params;
        let bytes = p[2].bytes().unwrap();
        let name = c.name();
        s += &format!(
            "func @{name}(ptr,ptr,ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr):\n v5 = iconst.i32 {}\n v6 = load.{} align 1 v1\n v7 = load.{} align 1 v2\n v8 = load.{} align 1 v3\n v9 = intrinsic {}(v0,{},v7,{},v5)\n store align 1 v9,v4\n v10 = iconst.i64 {bytes}\n v11 = ptradd v4,v10\n store align 1 v7,v11\n v12 = ptradd v11,v10\n store align 1 v8,v12\n v13 = ptradd v12,v10\n store align 1 v6,v13\n return\n}}\n",
            c.scale,
            p[1],
            p[2],
            p[3],
            c.id,
            if c.alias == 2 { "v7" } else { "v6" },
            if c.alias == 1 { "v7" } else { "v8" }
        );
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n#ifdef __linux__\nextern long sysconf(int);extern void*mmap(void*,__SIZE_TYPE__,int,int,int,long);extern int mprotect(void*,__SIZE_TYPE__,int);extern int munmap(void*,__SIZE_TYPE__);extern int fork(void);extern int waitpid(int,int*,int);extern void _exit(int);struct Limit{unsigned long a,b;};extern int setrlimit(int,const struct Limit*);\n#endif\n",
    );
    let mut table = String::from(
        "struct Case{void(*run)(const void*,const void*,const void*,const void*,void*);unsigned bits,ibits,bytes,ibytes,n,scale,alias;const char*name;};static const struct Case cases[]={\n",
    );
    for case in cases() {
        let p = case.id.info().params;
        let d = p[2].vector().unwrap();
        let i = p[1].vector().unwrap();
        let name = case.name();
        c += &format!(
            "extern void {name}(const void*,const void*,const void*,const void*,void*);\n"
        );
        table += &format!(
            "{{{name},{},{},{},{},{},{},{},\"{name}\"}},\n",
            d.lane_type().bytes().unwrap() * 8,
            i.lane_type().bytes().unwrap() * 8,
            d.bytes(),
            i.bytes(),
            d.lanes().min(i.lanes()),
            case.scale,
            case.alias
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
static uint64_t get(const unsigned char*p,unsigned bits){uint64_t v=0;memcpy(&v,p,bits/8);return v;}
static void put(unsigned char*p,unsigned bits,uint64_t v){memcpy(p,&v,bits/8);}
static void expected(const struct Case*c,const unsigned char*base,const unsigned char*idx,const unsigned char*old,const unsigned char*mask,unsigned char*out){
 memset(out,0,128);memcpy(out,old,c->n*c->bits/8);const unsigned char*im=c->alias==2?old:idx,*mm=c->alias==1?old:mask;
 for(unsigned i=0;i<c->n;i++)if(get(mm+i*c->bits/8,c->bits)>>(c->bits-1)){uint64_t raw=get(im+i*c->ibits/8,c->ibits);int64_t index=c->ibits==32?(int64_t)(int32_t)raw:(int64_t)raw;memcpy(out+i*c->bits/8,base+index*c->scale,c->bits/8);}
 memcpy(out+c->bytes,old,c->bytes);memcpy(out+2*c->bytes,mask,c->bytes);memcpy(out+3*c->bytes,idx,c->ibytes);
}
static int check(const struct Case*c,const unsigned char*base,const unsigned char*idx,const unsigned char*old,const unsigned char*mask){unsigned char e[128],r[130];memset(r,0x5a,130);expected(c,base,idx,old,mask,e);c->run(base,idx,old,mask,r+1);unsigned size=3*c->bytes+c->ibytes;if(memcmp(r+1,e,size)||r[0]!=0x5a||r[1+size]!=0x5a){printf("failed %s\n",c->name);return 1;}return 0;}
int main(void){
#ifdef __linux__
 struct Limit no_core={0,0};setrlimit(4,&no_core);unsigned long page=(unsigned long)sysconf(30);unsigned char*a=mmap(0,3*page,3,0x22,-1,0);if(a==(void*)-1)return 90;if(mprotect(a,page,0)||mprotect(a+2*page,page,0))return 91;for(unsigned i=0;i<page;i++)a[page+i]=i*17+31;
 for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++){const struct Case*c=&cases[k];unsigned char idx[32],old[32],mask[32];for(unsigned offset=0;offset<16;offset++)for(unsigned pattern=0;pattern<(1u<<c->n);pattern++){
 for(unsigned i=0;i<32;i++){idx[i]=i*3;old[i]=i*31+19;mask[i]=i*7+1;}
 for(unsigned i=0;i<c->n;i++){uint64_t m=(0x123456789abcdefULL&(~0ULL>>(65-c->bits)))|((uint64_t)((pattern>>i)&1)<<(c->bits-1));put(mask+i*c->bits/8,c->bits,m);if(c->alias==1)put(old+i*c->bits/8,c->bits,m);int64_t index=(pattern>>i)&1?(int64_t)i*7-31:(int64_t)(page/c->scale);put(idx+i*c->ibits/8,c->ibits,(uint64_t)index);if(c->alias==2)put(old+i*c->ibits/8,c->ibits,(uint64_t)index);}
 if(check(c,a+page+page/2+offset,idx,old,mask))return 10;
 }
 // A completely protected base must be harmless when every active mask bit is0.
 memset(idx,0,32);memset(old,0,32);memset(mask,0,32);if(check(c,a,idx,old,mask))return 11;
 // Conversely, an active guard-page lane must fault rather than be dropped.
 put(mask,c->bits,1ULL<<(c->bits-1));if(c->alias==1)put(old,c->bits,1ULL<<(c->bits-1));
 int pid=fork();if(pid<0)return 92;if(pid==0){unsigned char out[128];c->run(a,idx,old,mask,out);_exit(0);}int status=0;if(waitpid(pid,&status,0)!=pid)return 93;int signal=status&127;if(signal!=11&&signal!=7){printf("missing active fault %s status%d\n",c->name,status);return 12;}
 }
 munmap(a,3*page);
#endif
 return 0;}
"#;
    c
}
use crate::matrix;
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    static PREPARED: std::sync::OnceLock<matrix::PreparedModule> = std::sync::OnceLock::new();
    let m = PREPARED
        .get_or_init(|| matrix::PreparedModule::parse(&source()))
        .for_selector(s);
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
#[test]
fn all_gather_forms_scale_alias_spill_matrix_and_exports() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("gather.ir"), source()).unwrap();
        fs::write(d.join("gather.c"), harness()).unwrap();
    }
    let d = common::temp("gather-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("gather.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-fsyntax-only",
            ])
            .arg(d.join("gather.c")),
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
                    let name = format!("gather-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[test]
fn gather_rejects_disabled_features_bad_operands_and_nonconstant_scales() {
    let mut f = Features::for_level(CpuLevel::V3);
    f.avx2 = false;
    assert!(object(SelectionTier::Baseline, AllocationTier::Full, false, f).is_err());
    for bad in [
        source().replace("iconst.i32 1", "iconst.i32 3"),
        source().replace("iconst.i32 1", "load.i32 v1"),
        source().replace("load.i32x4", "load.i64x2"),
    ] {
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    let narrow = source()
        .split("func @g_x86_avx2_vgatherdps256")
        .next()
        .unwrap()
        .to_string();
    let m = keel_ir::parse(&narrow).unwrap();
    assert!(
        keel_codegen::aarch64::compile(&m)
            .unwrap_err()
            .message
            .contains("x86 architectural")
    );
    let mut minimal = Features::NONE;
    minimal = minimal.with_features("avx").unwrap();
    minimal.avx2 = true;
    object(SelectionTier::Full, AllocationTier::Full, true, minimal).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_all_gather_forms_masked_faults_and_source_aliases() {
    if !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let d = common::temp("gather-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("gather.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V3)).unwrap();
                fs::write(d.join("gather.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2"])
                        .arg(d.join("gather.c"))
                        .arg(d.join("gather.o"))
                        .arg("-o")
                        .arg(d.join("gather")),
                );
                common::command(&mut Command::new(d.join("gather")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
