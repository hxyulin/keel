//! Raw DQ floating specials retain DAZ ordering, scalar sources and SAE semantics.
use super::pclmul::parse;
use keel_codegen::x86_features::module_features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
const FORMS: &[(&str, &str, &str, &str, bool, bool)] = &[
    (
        "x86_avx512dq_rangeps128",
        "vrangeps",
        "Range",
        "f32x4",
        false,
        false,
    ),
    (
        "x86_avx512dq_rangepd128",
        "vrangepd",
        "Range",
        "f64x2",
        false,
        false,
    ),
    (
        "x86_avx512dq_reduceps128",
        "vreduceps",
        "Reduce",
        "f32x4",
        false,
        false,
    ),
    (
        "x86_avx512dq_reducepd128",
        "vreducepd",
        "Reduce",
        "f64x2",
        false,
        false,
    ),
    (
        "x86_avx512dq_fpclassps128",
        "vfpclassps",
        "Fpclass",
        "f32x4",
        false,
        false,
    ),
    (
        "x86_avx512dq_fpclasspd128",
        "vfpclasspd",
        "Fpclass",
        "f64x2",
        false,
        false,
    ),
    (
        "x86_avx512dq_rangeps256",
        "vrangeps",
        "Range",
        "f32x8",
        false,
        false,
    ),
    (
        "x86_avx512dq_rangepd256",
        "vrangepd",
        "Range",
        "f64x4",
        false,
        false,
    ),
    (
        "x86_avx512dq_reduceps256",
        "vreduceps",
        "Reduce",
        "f32x8",
        false,
        false,
    ),
    (
        "x86_avx512dq_reducepd256",
        "vreducepd",
        "Reduce",
        "f64x4",
        false,
        false,
    ),
    (
        "x86_avx512dq_fpclassps256",
        "vfpclassps",
        "Fpclass",
        "f32x8",
        false,
        false,
    ),
    (
        "x86_avx512dq_fpclasspd256",
        "vfpclasspd",
        "Fpclass",
        "f64x4",
        false,
        false,
    ),
    (
        "x86_avx512dq_rangeps512",
        "vrangeps",
        "Range",
        "f32x16",
        false,
        true,
    ),
    (
        "x86_avx512dq_rangepd512",
        "vrangepd",
        "Range",
        "f64x8",
        false,
        true,
    ),
    (
        "x86_avx512dq_reduceps512",
        "vreduceps",
        "Reduce",
        "f32x16",
        false,
        true,
    ),
    (
        "x86_avx512dq_reducepd512",
        "vreducepd",
        "Reduce",
        "f64x8",
        false,
        true,
    ),
    (
        "x86_avx512dq_fpclassps512",
        "vfpclassps",
        "Fpclass",
        "f32x16",
        false,
        false,
    ),
    (
        "x86_avx512dq_fpclasspd512",
        "vfpclasspd",
        "Fpclass",
        "f64x8",
        false,
        false,
    ),
    (
        "x86_avx512dq_rangess",
        "vrangess",
        "Range",
        "f32x4",
        true,
        true,
    ),
    (
        "x86_avx512dq_rangesd",
        "vrangesd",
        "Range",
        "f64x2",
        true,
        true,
    ),
    (
        "x86_avx512dq_reducess",
        "vreducess",
        "Reduce",
        "f32x4",
        true,
        true,
    ),
    (
        "x86_avx512dq_reducesd",
        "vreducesd",
        "Reduce",
        "f64x2",
        true,
        true,
    ),
    (
        "x86_avx512dq_fpclassss",
        "vfpclassss",
        "Fpclass",
        "f32x4",
        true,
        false,
    ),
    (
        "x86_avx512dq_fpclasssd",
        "vfpclasssd",
        "Fpclass",
        "f64x2",
        true,
        false,
    ),
];
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n#include <stdint.h>\n");
    let mut c = String::from(
        r#"
#include <immintrin.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
static const uint32_t f[]={0,0x80000000,1,0x80000001,0x7fc12345,0x7f800001,0x7f800000,0xff800000,0x3fa00000,0xc0300000,0x7f7fffff,0x00800000,0xffc54321,0xff800001,0x40500000,0xbf000000};
static const uint64_t d[]={0,0x8000000000000000ULL,1,0x8000000000000001ULL,0x7ff8123456789abcULL,0x7ff0000000000001ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x3ff4000000000000ULL,0xc006000000000000ULL,0x7fefffffffffffffULL,0x0010000000000000ULL,0xfff8123456789abcULL,0xfff0000000000001ULL,0x400a000000000000ULL,0xbfe0000000000000ULL};
static unsigned category(uint64_t x,unsigned bits,unsigned daz){unsigned neg=x>>(bits-1);uint64_t frac=x&(bits==32?0x7fffffULL:0xfffffffffffffULL);unsigned exp=(unsigned)((x>>(bits==32?23:52))&(bits==32?255:2047));unsigned all=exp==(bits==32?255:2047);unsigned zero=exp==0&&(frac==0||daz),den=exp==0&&frac!=0&&!daz;unsigned quiet=(x>>(bits==32?22:51))&1;return (all&&frac&&quiet?1:0)|(zero&&!neg?2:0)|(zero&&neg?4:0)|(all&&!frac&&!neg?8:0)|(all&&!frac&&neg?16:0)|(den?32:0)|(neg&&!all&&!zero?64:0)|(all&&frac&&!quiet?128:0);}
static uint64_t finite_range(uint64_t a,uint64_t b,unsigned imm,unsigned bits){uint64_t sign=UINT64_C(1)<<(bits-1),limit=bits==32?0xffffffffULL:~0ULL;uint64_t ka=(a&sign)?~a&limit:a^sign,kb=(b&sign)?~b&limit:b^sign;unsigned le=(imm&2)?(a&~sign)<=(b&~sign):ka<=kb;uint64_t v=(imm&1)?(le?b:a):(le?a:b);switch((imm>>2)&3){case 0:return(v&~sign)|(a&sign);case 1:return v;case 2:return v&~sign;default:return v|sign;}}
static uint64_t finite_reduce(unsigned imm,unsigned rc,unsigned bits){unsigned m=imm>>4;if(m>1)return~0ULL;if(!(imm&4))rc=imm&3;int n=13*(1<<m),q=n/4,r=n%4;if(rc==0&&(r>2||(r==2&&(q&1))))q++;if(rc==2&&r)q++;int residue=13-q*(4>>m);if(bits==32)return residue==1?0x3e800000:residue==-1?0xbe800000:0xbf400000;return residue==1?0x3fd0000000000000ULL:residue==-1?0xbfd0000000000000ULL:0xbfe8000000000000ULL;}
"#,
    );
    let mut checks = String::new();
    for &(_, opcode, family, shape, scalar, has_sae) in FORMS {
        let class = family == "Fpclass";
        let binary = scalar || family == "Range";
        let bits = if shape.starts_with("f32") { 32 } else { 64 };
        let bytes = shape[1..]
            .split_once('x')
            .map(|(b, n)| b.parse::<usize>().unwrap() * n.parse::<usize>().unwrap() / 8)
            .unwrap();
        let lanes = bytes * 8 / bits;
        let active = if scalar { 1 } else { lanes };
        let cmask = if lanes == 16 { "uint16_t" } else { "uint8_t" };
        let mbytes = if lanes == 16 { 2 } else { 1 };
        let vector = format!("__m{}{}", bytes * 8, if bits == 64 { "d" } else { "" });
        let uint = if bits == 32 { "uint32_t" } else { "uint64_t" };
        let imms: &[i32] = if class {
            &[0, 1, 2, 4, 8, 16, 32, 64, 128, 255]
        } else if family == "Range" {
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        } else {
            &[0, 4, 8, 12, 16, 19, 244, 255]
        };
        let saes: &[i32] = if has_sae { &[4, 8] } else { &[4] };
        for &imm in imms {
            for &sae in saes {
                let name = format!("{opcode}_{}_{}_{imm}_{sae}", bytes * 8, u8::from(scalar));
                let suffix = &opcode[1 + family.len()..];
                let prefix = if scalar || bytes == 16 {
                    "_mm".to_owned()
                } else {
                    format!("_mm{}", bytes * 8)
                };
                let api = if class {
                    format!("fpclass_{suffix}_mask")
                } else if has_sae && sae == 8 {
                    format!("{}_round_{suffix}", family.to_lowercase())
                } else {
                    format!("{}_{suffix}", family.to_lowercase())
                };
                let control = if has_sae && sae == 8 {
                    format!(",{sae}")
                } else {
                    String::new()
                };
                let aexpr = "(na++,a)";
                let bexpr = if binary && !class { ",(nb++,b)" } else { "" };
                let plainb = if binary && !class { ",b" } else { "" };
                let mtype = if class {
                    cmask.to_owned()
                } else {
                    vector.clone()
                };
                let masked = if class {
                    format!("{prefix}_mask_{api}((nk++,k),{aexpr},{imm})")
                } else {
                    format!(
                        "{prefix}_mask_{api}((ns++,merge),(nk++,k),{aexpr}{bexpr},{imm}{control})"
                    )
                };
                let unmasked = format!("{prefix}_{api}({aexpr}{bexpr},{imm}{control})");
                let loadb = if binary && !class {
                    format!("{vector} b=*(const {vector}*)bp;")
                } else {
                    String::new()
                };
                let storeb = if binary && !class {
                    format!("*({vector}*)((char*)o+192)=b;")
                } else {
                    String::new()
                };
                let flags = if scalar || bytes == 64 {
                    "avx512f,avx512dq"
                } else {
                    "avx512f,avx512dq,avx512vl"
                };
                let resultbytes = if class { mbytes } else { bytes };
                source += &format!(
                    "__attribute__((target(\"{flags}\"))) void {name}(const void*sp,const void*ap,const void*bp,void*o,{cmask} k){{{vector} merge=*(const {vector}*)sp,a=*(const {vector}*)ap;{loadb}unsigned ns=0,nk=0,na=0,nb=0;{mtype} result;if(k==({cmask})~0u)result={unmasked};else result={masked};if(na!=1||nb!={}||nk!=(k==({cmask})~0u?0:1)||ns!=({}&&k!=({cmask})~0u))return;*({mtype}*)o=result;*({vector}*)((char*)o+64)=merge;*({vector}*)((char*)o+128)=a;{storeb}*({cmask}*)((char*)o+256)=k;}}\n",
                    u8::from(binary && !class),
                    u8::from(!class)
                );
                let plainref = if class {
                    format!("{prefix}_mask_{api}(k,a,{imm})")
                } else {
                    format!("{prefix}_mask_{api}(merge,k,a{plainb},{imm}{control})")
                };
                c += &format!(
                    "__attribute__((noinline)) static void std_{name}(const void*sp,const void*ap,const void*bp,void*o,{cmask} k){{{vector} merge,a;memcpy(&merge,sp,{bytes});memcpy(&a,ap,{bytes});{}{mtype} r={plainref};memcpy(o,&r,{resultbytes});}}\n",
                    if binary && !class {
                        format!("{vector} b;memcpy(&b,bp,{bytes});")
                    } else {
                        String::new()
                    }
                );
                if !class {
                    source += &format!(
                        "__attribute__((target(\"{flags}\"))) void zero_{name}(const void*sp,const void*ap,const void*bp,void*o,{cmask} k){{{vector} a=*(const {vector}*)ap;{loadb}*({vector}*)o={prefix}_maskz_{api}(k,a{plainb},{imm}{control});}}\n"
                    );
                    c += &format!(
                        "extern void zero_{name}(const void*,const void*,const void*,void*,{cmask});\n"
                    );
                    if has_sae && sae == 4 {
                        let round = format!("{}_round_{suffix}", family.to_lowercase());
                        source += &format!(
                            "__attribute__((target(\"{flags}\"))) void explicit_{name}(const void*sp,const void*ap,const void*bp,void*o,{cmask} k){{{vector} merge=*(const {vector}*)sp,a=*(const {vector}*)ap;{loadb}*({vector}*)o={prefix}_mask_{round}(merge,k,a{plainb},{imm},4);}}\n__attribute__((target(\"{flags}\"))) void explicit_zero_{name}(const void*ap,const void*bp,void*o,{cmask} k){{{vector} a=*(const {vector}*)ap;{loadb}*({vector}*)o={prefix}_maskz_{round}(k,a{plainb},{imm},4);}}\n__attribute__((target(\"{flags}\"))) void explicit_all_{name}(const void*ap,const void*bp,void*o){{{vector} a=*(const {vector}*)ap;{loadb}*({vector}*)o={prefix}_{round}(a{plainb},{imm},4);}}\n"
                        );
                    }
                }
                let decorator = if sae == 8 { "%{sae%}," } else { "" };
                let asm = if class {
                    format!("{opcode} ${imm},%2,%0%{{%1%}}")
                } else if binary {
                    format!("{opcode} ${imm},{decorator}%3,%2,%0%{{%1%}}")
                } else {
                    format!("{opcode} ${imm},{decorator}%2,%0%{{%1%}}")
                };
                let asm = asm.replace("{imm}", &imm.to_string());
                let refparams = if binary && !class {
                    format!("{vector} b;memcpy(&b,bp,{bytes});")
                } else {
                    String::new()
                };
                let operands = if binary && !class { ",\"v\"(b)" } else { "" };
                let refbody = if class {
                    format!(
                        "{cmask} result;__asm__ volatile(\"{asm}\":\"=Yk\"(result):\"Yk\"(k),\"v\"(a));memcpy(o,&result,{mbytes});"
                    )
                } else {
                    format!(
                        "{vector} result;memcpy(&result,sp,{bytes});__asm__ volatile(\"{asm}\":\"+v\"(result):\"Yk\"(k),\"v\"(a){operands});memcpy(o,&result,{bytes});"
                    )
                };
                c += &format!(
                    "extern void {name}(const void*,const void*,const void*,void*,{cmask});\n__attribute__((noinline)) static void ref_{name}(const void*sp,const void*ap,const void*bp,void*o,{cmask} k){{{vector} a;memcpy(&a,ap,{bytes});{refparams}{refbody}}}\n"
                );
                let familymodel = if class {
                    format!(
                        "unsigned category_mask=0;for(unsigned i=0;i<{active};i++)if((k&(1u<<i))&&(category(a[i],{bits},daz)&{imm}))category_mask|=1u<<i;{cmask} got;memcpy(&got,out,{mbytes});if(got!=category_mask)return 14;"
                    )
                } else {
                    let source = if family == "Range" {
                        format!("finite_range(a[i],b[i],{imm},{bits})")
                    } else {
                        format!("finite_reduce({imm},rc,{bits})")
                    };
                    format!(
                        "if(sample==4{} )for(unsigned i=0;i<{active};i++){{{uint} got;memcpy(&got,out+i*sizeof got,sizeof got);uint64_t expected=(k&(1u<<i))?{source}:s[i];if(got!=expected)return 15;}}",
                        if family == "Reduce" && imm >> 4 > 1 {
                            "&&0"
                        } else {
                            ""
                        }
                    )
                };
                let resultbytes = if class { mbytes } else { bytes };
                let zero_check = if class {
                    String::new()
                } else {
                    format!(
                        "unsigned char zero[64],zref[64];memcpy(zref,expected,{bytes});for(unsigned i=0;i<{active};i++)if(!(k&(1u<<i)))memset(zref+i*({bits}/8),0,{bits}/8);zero_{name}(s,a,b,zero,k);if(memcmp(zero,zref,{bytes}))return 18;"
                    )
                };
                let fillb = if binary && !class {
                    format!(
                        "memcpy(b+ i,&{}[(i+sample*3+5)%16],sizeof b[i]);",
                        if bits == 32 { "f" } else { "d" }
                    )
                } else {
                    String::new()
                };
                let finite_a = if family == "Range" {
                    if bits == 32 {
                        "0xc0000000u"
                    } else {
                        "0xc000000000000000ULL"
                    }
                } else {
                    if bits == 32 {
                        "0x40500000u"
                    } else {
                        "0x400a000000000000ULL"
                    }
                };
                let finite_b = if bits == 32 {
                    "0x40500000u"
                } else {
                    "0x400a000000000000ULL"
                };
                let source_note = if scalar && !class {
                    format!(
                        "for(unsigned i=1;i<{lanes};i++){{{uint} got;memcpy(&got,out+i*sizeof got,sizeof got);if(got!=a[i])return 16;}} "
                    )
                } else {
                    String::new()
                };
                checks += &format!(
                    "{{{uint} a[{lanes}],b[{lanes}],s[{lanes}];unsigned char out[288],expected[64],standard[64];{cmask} masks[]={{0,({cmask})~0u,({cmask})0xaaaa,({cmask})(1u<<({active}-1)),({cmask})~((1u<<{active})-1)}};for(unsigned sample=0;sample<7;sample++){{for(unsigned i=0;i<{lanes};i++){{memcpy(a+i,&{}[(i+(sample==5?5:sample==6?4:sample*3))%16],sizeof a[i]);{fillb}s[i]=({uint})(UINT64_C(0xa54321ff81234567)+i*37);if(sample==4){{a[i]={finite_a};b[i]={finite_b};}}}}for(unsigned daz=0;daz<2;daz++)for(unsigned rc=0;rc<4;rc++)for(unsigned mi=0;mi<5;mi++){{{cmask} k=masks[mi];unsigned csr=0x1f80|(rc<<13)|(daz?0x40:0)|(sample&1?0x8000:0)|(sample==3?0x21:0);if({}||{sae}==8||!(k&((1u<<{active})-1)))csr&=~0x80u;memset(out,0xa5,sizeof out);_mm_setcsr(csr);{name}(s,a,b,out,k);unsigned flags=_mm_getcsr();_mm_setcsr(csr);ref_{name}(s,a,b,expected,k);unsigned reference_flags=_mm_getcsr();_mm_setcsr(csr);std_{name}(s,a,b,standard,k);if(memcmp(standard,expected,{resultbytes}))return 17;if(memcmp(out,expected,{resultbytes})||flags!=reference_flags){{fprintf(stderr,\"{name} sample%u daz%u rc%u mask%u flags%x ref%x\\n\",sample,daz,rc,mi,flags,reference_flags);return 10;}}{familymodel}{source_note}{zero_check}if(memcmp(out+64,s,{bytes})||memcmp(out+128,a,{bytes})||memcmp(out+256,&k,{mbytes}){})return 11;for(unsigned i={resultbytes};i<64;i++)if(out[i]!=0xa5)return 12;for(unsigned i={mbytes};i<32;i++)if(out[256+i]!=0xa5)return 13;memcpy(out,a,{bytes});_mm_setcsr(csr);{name}(s,out,b,out,k);if(memcmp(out,expected,{resultbytes})||memcmp(out+128,a,{bytes})||_mm_getcsr()!=reference_flags)return 20;memcpy(out,s,{bytes});_mm_setcsr(csr);{name}(out,a,b,out,k);if(memcmp(out,expected,{resultbytes})||memcmp(out+64,s,{bytes})||_mm_getcsr()!=reference_flags)return 21;}}}}}}\n",
                    if bits == 32 { "f" } else { "d" },
                    u8::from(class),
                    if binary && !class {
                        format!("||memcmp(out+192,b,{bytes})")
                    } else {
                        String::new()
                    }
                );
            }
        }
    }
    source += "__attribute__((target(\"avx512f,avx512dq\"))) void classify_daz_order(const void*ap,void*out){__m512 a=*(const __m512*)ap;unsigned old=_mm_getcsr();_mm_setcsr(0x1f80);__mmask16 first=_mm512_fpclass_ps_mask(a,32);_mm_setcsr(0x1fc0);__mmask16 second=_mm512_fpclass_ps_mask(a,32);_mm_setcsr(old);*(unsigned short*)out=first;*((unsigned short*)out+1)=second;}\n";
    c += "extern void classify_daz_order(const void*,void*);\n";
    checks += "{uint32_t input[16];uint16_t out[2];for(unsigned i=0;i<16;i++)input[i]=1;classify_daz_order(input,out);if(out[0]!=65535||out[1]!=0)return 22;}";
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, c)
}
#[test]
fn dq_special_c_bits_flags_sources_and_masks() {
    let (source, harness) = fixtures();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&m, base).unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-dq-special-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native && std::is_x86_feature_detected!("avx512dq");
    if native {
        // Compile the immutable oracle once, then relink each allocation variant.
        let output = Command::new("cc")
            .args([
                "-O1",
                "-mavx512f",
                "-mavx512vl",
                "-mavx512dq",
                "-frounding-math",
                "-c",
            ])
            .arg(dir.join("harness.c"))
            .arg("-o")
            .arg(dir.join("harness.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(12000)
                .collect::<String>()
        );
    }
    for (tier, alloc, pools, opt) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Fast,
            &keel_codegen::x86_64::REGISTERS,
            false,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            true,
        ),
        (SelectionTier::Full, AllocationTier::Full, &EMPTY, true),
        (SelectionTier::Full, AllocationTier::Full, &ONE, true),
    ] {
        let mut module = m.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features: base,
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &p).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(dir.join("convert.o"), object).unwrap();
        if native {
            let output = Command::new("cc")
                .arg(dir.join("harness.o"))
                .arg(dir.join("convert.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(12000)
                    .collect::<String>()
            );
            let status = Command::new(dir.join("run")).status().unwrap();
            assert!(status.success(), "DQ special oracle: {status}");
        }
    }
}
#[test]
fn dq_special_c_features_and_controls() {
    let (source, _) = fixtures();
    for attr in [
        "__attribute__((target(\"avx512f,avx512dq,avx512vl\"))) ",
        "__attribute__((target(\"avx512f,avx512dq\"))) ",
    ] {
        let module = parse(&source.replace(attr, ""), Target::X86_64LinuxGnu).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features("avx512f").unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
                .is_err()
        );
    }
    for call in [
        "_mm512_range_ps(a,b,16)",
        "_mm512_reduce_ps(a,256)",
        "_mm512_reduce_round_ps(a,0,9)",
        "_mm512_range_round_ps(a,b,0,11)",
    ] {
        let text = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512dq\"))) __m512 bad(__m512 a,__m512 b){{return {call};}}"
        );
        assert!(parse(&text, Target::X86_64LinuxGnu).is_err());
    }
}
