//! Every legal wide widening/narrowing pair and horizontal integer reduction.
use crate::common;
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{VectorReduceOp, VectorType};
use std::{fs, path::PathBuf, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
struct Case {
    from: VectorType,
    to: VectorType,
    op: &'static str,
    kind: u8,
}
fn cases() -> Vec<Case> {
    let shapes: Vec<_> = VectorType::ALL_128
        .into_iter()
        .chain(VectorType::ALL_256)
        .filter(|s| s.lane_type().is_int())
        .collect();
    let mut cases = Vec::new();
    for &from in &shapes {
        for &to in &shapes {
            if from.bytes() != 32 && to.bytes() != 32 {
                continue;
            }
            let a = from.lane_type().bytes().unwrap();
            let b = to.lane_type().bytes().unwrap();
            if a < b && from.lanes() >= to.lanes() {
                for (kind, op) in ["uwiden_low", "swiden_low", "uwiden_high", "swiden_high"]
                    .into_iter()
                    .enumerate()
                {
                    cases.push(Case {
                        from,
                        to,
                        op,
                        kind: kind as u8,
                    });
                }
            }
            if a > b {
                for (kind, op) in [
                    "narrow_low",
                    "snarrow_sat_low",
                    "unarrow_sat_low",
                    "sunarrow_sat_low",
                ]
                .into_iter()
                .enumerate()
                {
                    cases.push(Case {
                        from,
                        to,
                        op,
                        kind: kind as u8 + 4,
                    });
                }
            }
            if a * 2 == b && u16::from(from.lanes()) >= 2 * u16::from(to.lanes()) {
                for (kind, op) in ["spairwise_add_long", "upairwise_add_long"]
                    .into_iter()
                    .enumerate()
                {
                    cases.push(Case {
                        from,
                        to,
                        op,
                        kind: kind as u8 + 8,
                    });
                }
            }
        }
    }
    cases
}
fn source() -> String {
    let mut s = String::new();
    for (i, c) in cases().iter().enumerate() {
        let bytes = c.to.bytes();
        s += &format!(
            "func @convert{i}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{} align 1 v0\n v3 = {}.{} v2\n store align 1 v3,v1\n v4 = iconst.i64 {bytes}\n v5 = ptradd v1,v4\n store align 1 v2,v5\n return\n}}\n",
            c.from, c.op, c.to
        );
    }
    for shape in VectorType::ALL_256
        .into_iter()
        .filter(|s| s.lane_type().is_int())
    {
        for op in VectorReduceOp::ALL {
            s += &format!(
                "func @reduce_{shape}_{op}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{shape} align 1 v0\n v3 = {op} v2\n store align 1 v3,v1\n return\n}}\n"
            );
        }
    }
    s
}
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\nextern void *memcpy(void*,const void*,__SIZE_TYPE__);extern void *memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n#ifdef __linux__\nextern long sysconf(int);extern void *mmap(void*,__SIZE_TYPE__,int,int,int,long);extern int mprotect(void*,__SIZE_TYPE__,int);extern int munmap(void*,__SIZE_TYPE__);\n#endif\n",
    );
    let mut table = String::from(
        "static const struct Case {void(*run)(const void*,void*);unsigned from,to,in,out,kind;const char*name;} cases[]={\n",
    );
    for (i, case) in cases().iter().enumerate() {
        c += &format!("extern void convert{i}(const void*,void*);\n");
        table += &format!(
            "{{convert{i},{},{},{},{},{},\"{}:{}->{}\"}},\n",
            case.from.lane_type().bytes().unwrap() * 8,
            case.to.lane_type().bytes().unwrap() * 8,
            case.from.bytes(),
            case.to.bytes(),
            case.kind,
            case.op,
            case.from,
            case.to
        );
    }
    for shape in VectorType::ALL_256
        .into_iter()
        .filter(|s| s.lane_type().is_int())
    {
        for (kind, op) in VectorReduceOp::ALL.into_iter().enumerate() {
            let bytes = shape.lane_type().bytes().unwrap();
            c += &format!("extern void reduce_{shape}_{op}(const void*,void*);\n");
            table += &format!(
                "{{reduce_{shape}_{op},{},{},32,{}, {},\"{shape}:{op}\"}},\n",
                bytes * 8,
                bytes * 8,
                bytes,
                kind + 10
            );
        }
    }
    c += &table;
    c += "};\n";
    c += r#"
static uint64_t get(const unsigned char*p,unsigned bits){uint64_t x=0;memcpy(&x,p,bits/8);return x;}
static int64_t sx(uint64_t x,unsigned bits){if(bits<64&&((x>>(bits-1))&1))x|=(~0ULL)<<bits;return (int64_t)x;}
static void expected(const struct Case*c,const unsigned char*in,unsigned char*out){
 memset(out,0,64);unsigned lanes=c->in*8/c->from,n=c->out*8/c->to;
 if(c->kind>=10){uint64_t r=get(in,c->from);for(unsigned i=1;i<lanes;i++){uint64_t x=get(in+i*c->from/8,c->from);switch(c->kind){case 10:r+=x;break;case 11:r&=x;break;case 12:r|=x;break;case 13:r^=x;break;case 14:if(sx(x,c->from)<sx(r,c->from))r=x;break;case 15:if(sx(x,c->from)>sx(r,c->from))r=x;break;case 16:if(x<r)r=x;break;case 17:if(x>r)r=x;break;}}memcpy(out,&r,c->out);return;}
 for(unsigned i=0;i<n;i++){uint64_t v=0;if(c->kind<=3){unsigned j=(c->kind>=2?lanes-n:0)+i;v=get(in+j*c->from/8,c->from);if(c->kind&1)v=(uint64_t)sx(v,c->from);}
 else if(c->kind>=8){uint64_t a=get(in+2*i*c->from/8,c->from),b=get(in+(2*i+1)*c->from/8,c->from);v=c->kind==8?(uint64_t)(sx(a,c->from)+sx(b,c->from)):a+b;}
 else if(i<lanes){v=get(in+i*c->from/8,c->from);uint64_t max=(1ULL<<c->to)-1;if(c->kind==5){int64_t x=sx(v,c->from),lo=-(1LL<<(c->to-1)),hi=(1LL<<(c->to-1))-1;if(x<lo)x=lo;if(x>hi)x=hi;v=(uint64_t)x;}else if(c->kind==6){if(v>max)v=max;}else if(c->kind==7){int64_t x=sx(v,c->from);v=x<0?0:(uint64_t)x;if(v>max)v=max;}}
 memcpy(out+i*c->to/8,&v,c->to/8);}
 memcpy(out+c->out,in,c->in);
}
static int check(const struct Case*c,const unsigned char*in,unsigned char*out){unsigned char e[64];expected(c,in,e);c->run(in,out);unsigned size=c->kind>=10?c->out:c->out+c->in;if(memcmp(out,e,size)){printf("failed %s\n",c->name);return 1;}return 0;}
int main(void){
 const uint64_t edges[]={0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff,0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffffULL,0x8000000000000000ULL,~0ULL};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++)for(unsigned k=0;k<270;k++)for(unsigned offset=0;offset<16;offset++)for(unsigned alias=0;alias<2;alias++){
 unsigned char in[128],out[128];memset(in,0x5a,128);memset(out,0x5a,128);unsigned char*p=in+16+offset,*q=(alias?in:out)+16+offset;unsigned size=cases[c].kind>=10?cases[c].out:cases[c].out+cases[c].in;
 if(k<256){for(unsigned i=0;i<cases[c].in;i++)p[i]=k+31*i;}else{for(unsigned i=0;i<cases[c].in*8/cases[c].from;i++){uint64_t v=edges[(k-256+7*i)%14];memcpy(p+i*cases[c].from/8,&v,cases[c].from/8);}}
 if(check(&cases[c],p,q))return 1;if(q[-1]!=0x5a||((!alias||size>=cases[c].in)&&q[size]!=0x5a))return 2;
 }
#ifdef __linux__
 unsigned long page=(unsigned long)sysconf(30);unsigned char*a=mmap(0,page*2,3,0x22,-1,0),*b=mmap(0,page*2,3,0x22,-1,0);if(a==(void*)-1||b==(void*)-1)return 90;if(mprotect(a+page,page,0)||mprotect(b+page,page,0))return 91;
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){unsigned size=cases[c].kind>=10?cases[c].out:cases[c].out+cases[c].in;unsigned char*p=a+page-cases[c].in,*q=b+page-size;for(unsigned k=0;k<256;k++){for(unsigned i=0;i<cases[c].in;i++)p[i]=k+31*i;if(check(&cases[c],p,q))return 92;}}
 munmap(a,page*2);munmap(b,page*2);
#endif
 return 0;}
"#;
    c
}
fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    features: Features,
) -> (Vec<u8>, String) {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features,
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
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    assert_eq!(p.stats().text_bytes, 0);
    let b = keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
    assert_eq!(
        b,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (b, text)
}
#[test]
fn wide_conversion_reduction_matrix_emits_and_exports() {
    let mut fallback = Features::for_level(CpuLevel::V3);
    fallback.avx2 = false;
    object(SelectionTier::Full, AllocationTier::Full, true, fallback);
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join("conversions.ir"), source()).unwrap();
        fs::write(d.join("conversions.c"), harness()).unwrap();
    }
    let d = common::temp("wide-convert-syntax");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("conversions.c"), harness()).unwrap();
    common::command(
        Command::new("clang")
            .args(["-std=gnu11", "-fsyntax-only"])
            .arg(d.join("conversions.c")),
    );
    fs::remove_dir_all(d).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, text) = object(s, a, empty, Features::for_level(CpuLevel::V3));
                if let Some(d) = &export {
                    let name = format!("conversions-{s:?}-{a:?}-{empty}");
                    fs::write(d.join(format!("{name}.o")), b).unwrap();
                    fs::write(d.join(format!("{name}.s")), text).unwrap();
                }
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_wide_conversion_reduction_reference() {
    if !std::is_x86_feature_detected!("avx2") {
        return;
    }
    let d = common::temp("wide-convert-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("conversions.c"), harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for empty in [false, true] {
                let (b, _) = object(s, a, empty, Features::for_level(CpuLevel::V3));
                fs::write(d.join("conversions.o"), b).unwrap();
                common::command(
                    Command::new("clang")
                        .args(["-O2", "-mavx2"])
                        .arg(d.join("conversions.c"))
                        .arg(d.join("conversions.o"))
                        .arg("-o")
                        .arg(d.join("conversions")),
                );
                common::command(&mut Command::new(d.join("conversions")));
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
