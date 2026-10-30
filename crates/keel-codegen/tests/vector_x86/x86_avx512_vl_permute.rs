//! Indexed and immediate shuffles match scalar lane formulas and system assembly.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, VectorType};
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
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
const TWELVE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[12],
    float_callee_saved: &[],
};
#[derive(Clone, Copy)]
enum Recipe {
    Full,
    Local,
    TwoIndex,
    TwoTable,
    Immediate,
    PairImmediate,
    Pair,
    Unary,
}
// These public shapes and scalar operation codes do not query intrinsic metadata.
const CASES: [(Intrinsic, VectorType, &str, Recipe, u8); 66] = [
    (
        Intrinsic::X86Avx512VlVpermi2d128,
        VectorType::I32x4,
        "vpermi2d",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermi2q128,
        VectorType::I64x2,
        "vpermi2q",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermi2ps128,
        VectorType::F32x4,
        "vpermi2ps",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermi2pd128,
        VectorType::F64x2,
        "vpermi2pd",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2d128,
        VectorType::I32x4,
        "vpermt2d",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2q128,
        VectorType::I64x2,
        "vpermt2q",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2ps128,
        VectorType::F32x4,
        "vpermt2ps",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2pd128,
        VectorType::F64x2,
        "vpermt2pd",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermilps128,
        VectorType::F32x4,
        "vpermilps",
        Recipe::Local,
        2,
    ),
    (
        Intrinsic::X86Avx512VlVpermilpd128,
        VectorType::F64x2,
        "vpermilpd",
        Recipe::Local,
        3,
    ),
    (
        Intrinsic::X86Avx512VlVpermilpsImm128,
        VectorType::F32x4,
        "vpermilps",
        Recipe::Immediate,
        4,
    ),
    (
        Intrinsic::X86Avx512VlVpermilpdImm128,
        VectorType::F64x2,
        "vpermilpd",
        Recipe::Immediate,
        5,
    ),
    (
        Intrinsic::X86Avx512VlVshufps128,
        VectorType::F32x4,
        "vshufps",
        Recipe::PairImmediate,
        7,
    ),
    (
        Intrinsic::X86Avx512VlVshufpd128,
        VectorType::F64x2,
        "vshufpd",
        Recipe::PairImmediate,
        8,
    ),
    (
        Intrinsic::X86Avx512VlVpshufd128,
        VectorType::I32x4,
        "vpshufd",
        Recipe::Immediate,
        9,
    ),
    (
        Intrinsic::X86Avx512VlValignd128,
        VectorType::I32x4,
        "valignd",
        Recipe::PairImmediate,
        11,
    ),
    (
        Intrinsic::X86Avx512VlValignq128,
        VectorType::I64x2,
        "valignq",
        Recipe::PairImmediate,
        11,
    ),
    (
        Intrinsic::X86Avx512VlVpunpckldq128,
        VectorType::I32x4,
        "vpunpckldq",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVpunpcklqdq128,
        VectorType::I64x2,
        "vpunpcklqdq",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVunpcklps128,
        VectorType::F32x4,
        "vunpcklps",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVunpcklpd128,
        VectorType::F64x2,
        "vunpcklpd",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVpunpckhdq128,
        VectorType::I32x4,
        "vpunpckhdq",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVpunpckhqdq128,
        VectorType::I64x2,
        "vpunpckhqdq",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVunpckhps128,
        VectorType::F32x4,
        "vunpckhps",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVunpckhpd128,
        VectorType::F64x2,
        "vunpckhpd",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVmovshdup128,
        VectorType::F32x4,
        "vmovshdup",
        Recipe::Unary,
        14,
    ),
    (
        Intrinsic::X86Avx512VlVmovsldup128,
        VectorType::F32x4,
        "vmovsldup",
        Recipe::Unary,
        15,
    ),
    (
        Intrinsic::X86Avx512VlVmovddup128,
        VectorType::F64x2,
        "vmovddup",
        Recipe::Unary,
        16,
    ),
    (
        Intrinsic::X86Avx512VlVpermd256,
        VectorType::I32x8,
        "vpermd",
        Recipe::Full,
        0,
    ),
    (
        Intrinsic::X86Avx512VlVpermq256,
        VectorType::I64x4,
        "vpermq",
        Recipe::Full,
        0,
    ),
    (
        Intrinsic::X86Avx512VlVpermps256,
        VectorType::F32x8,
        "vpermps",
        Recipe::Full,
        0,
    ),
    (
        Intrinsic::X86Avx512VlVpermpd256,
        VectorType::F64x4,
        "vpermpd",
        Recipe::Full,
        0,
    ),
    (
        Intrinsic::X86Avx512VlVpermi2d256,
        VectorType::I32x8,
        "vpermi2d",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermi2q256,
        VectorType::I64x4,
        "vpermi2q",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermi2ps256,
        VectorType::F32x8,
        "vpermi2ps",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermi2pd256,
        VectorType::F64x4,
        "vpermi2pd",
        Recipe::TwoIndex,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2d256,
        VectorType::I32x8,
        "vpermt2d",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2q256,
        VectorType::I64x4,
        "vpermt2q",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2ps256,
        VectorType::F32x8,
        "vpermt2ps",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermt2pd256,
        VectorType::F64x4,
        "vpermt2pd",
        Recipe::TwoTable,
        1,
    ),
    (
        Intrinsic::X86Avx512VlVpermilps256,
        VectorType::F32x8,
        "vpermilps",
        Recipe::Local,
        2,
    ),
    (
        Intrinsic::X86Avx512VlVpermilpd256,
        VectorType::F64x4,
        "vpermilpd",
        Recipe::Local,
        3,
    ),
    (
        Intrinsic::X86Avx512VlVpermilpsImm256,
        VectorType::F32x8,
        "vpermilps",
        Recipe::Immediate,
        4,
    ),
    (
        Intrinsic::X86Avx512VlVpermilpdImm256,
        VectorType::F64x4,
        "vpermilpd",
        Recipe::Immediate,
        5,
    ),
    (
        Intrinsic::X86Avx512VlVpermqImm256,
        VectorType::I64x4,
        "vpermq",
        Recipe::Immediate,
        6,
    ),
    (
        Intrinsic::X86Avx512VlVpermpdImm256,
        VectorType::F64x4,
        "vpermpd",
        Recipe::Immediate,
        6,
    ),
    (
        Intrinsic::X86Avx512VlVshufps256,
        VectorType::F32x8,
        "vshufps",
        Recipe::PairImmediate,
        7,
    ),
    (
        Intrinsic::X86Avx512VlVshufpd256,
        VectorType::F64x4,
        "vshufpd",
        Recipe::PairImmediate,
        8,
    ),
    (
        Intrinsic::X86Avx512VlVpshufd256,
        VectorType::I32x8,
        "vpshufd",
        Recipe::Immediate,
        9,
    ),
    (
        Intrinsic::X86Avx512VlVshufi32x4256,
        VectorType::I32x8,
        "vshufi32x4",
        Recipe::PairImmediate,
        10,
    ),
    (
        Intrinsic::X86Avx512VlVshufi64x2256,
        VectorType::I64x4,
        "vshufi64x2",
        Recipe::PairImmediate,
        10,
    ),
    (
        Intrinsic::X86Avx512VlVshuff32x4256,
        VectorType::F32x8,
        "vshuff32x4",
        Recipe::PairImmediate,
        10,
    ),
    (
        Intrinsic::X86Avx512VlVshuff64x2256,
        VectorType::F64x4,
        "vshuff64x2",
        Recipe::PairImmediate,
        10,
    ),
    (
        Intrinsic::X86Avx512VlValignd256,
        VectorType::I32x8,
        "valignd",
        Recipe::PairImmediate,
        11,
    ),
    (
        Intrinsic::X86Avx512VlValignq256,
        VectorType::I64x4,
        "valignq",
        Recipe::PairImmediate,
        11,
    ),
    (
        Intrinsic::X86Avx512VlVpunpckldq256,
        VectorType::I32x8,
        "vpunpckldq",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVpunpcklqdq256,
        VectorType::I64x4,
        "vpunpcklqdq",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVunpcklps256,
        VectorType::F32x8,
        "vunpcklps",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVunpcklpd256,
        VectorType::F64x4,
        "vunpcklpd",
        Recipe::Pair,
        12,
    ),
    (
        Intrinsic::X86Avx512VlVpunpckhdq256,
        VectorType::I32x8,
        "vpunpckhdq",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVpunpckhqdq256,
        VectorType::I64x4,
        "vpunpckhqdq",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVunpckhps256,
        VectorType::F32x8,
        "vunpckhps",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVunpckhpd256,
        VectorType::F64x4,
        "vunpckhpd",
        Recipe::Pair,
        13,
    ),
    (
        Intrinsic::X86Avx512VlVmovshdup256,
        VectorType::F32x8,
        "vmovshdup",
        Recipe::Unary,
        14,
    ),
    (
        Intrinsic::X86Avx512VlVmovsldup256,
        VectorType::F32x8,
        "vmovsldup",
        Recipe::Unary,
        15,
    ),
    (
        Intrinsic::X86Avx512VlVmovddup256,
        VectorType::F64x4,
        "vmovddup",
        Recipe::Unary,
        16,
    ),
];
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\ntypedef void(*function)(void*,void*,void*,void*,void*,uint8_t);\nstruct testcase{function run,reference;unsigned width,bits,operation,control,alias;const char*name;};\n",
    );
    let mut cases = String::new();
    for (id, shape, opcode, recipe, operation) in CASES {
        let width = shape.bytes() * 8;
        let bits = shape.lane_type().bytes().unwrap() * 8;
        let index = format!("i{bits}x{}", shape.lanes());
        let immediate = matches!(recipe, Recipe::Immediate | Recipe::PairImmediate);
        let controls = if !immediate {
            vec![0]
        } else if matches!(operation, 5 | 8) {
            let mut controls = (0..(1u32 << shape.lanes())).collect::<Vec<_>>();
            controls.extend([128, 255]);
            controls
        } else if operation == 10 {
            vec![0, 1, 2, 3, 128, 255]
        } else if operation == 11 {
            let mut controls = (0..u32::from(shape.lanes())).collect::<Vec<_>>();
            controls.extend([127, 128, 255]);
            controls
        } else {
            // Each two-bit selector sees all four source positions; asymmetric
            // controls expose output order and 128-bit lane boundaries.
            vec![0, 0x55, 0xaa, 0xff, 0x1b, 0x4e, 0xb1, 0xe4]
        };
        for control in controls {
            for alias in [false, true] {
                let name = format!("{}_{}_{}", id.info().name, control, u8::from(alias));
                let merge = if alias { "v7" } else { "v6" };
                let indexarg = if alias { "v20" } else { "v8" };
                let indexed = matches!(
                    recipe,
                    Recipe::Full | Recipe::Local | Recipe::TwoIndex | Recipe::TwoTable
                );
                let bitcast = if alias && indexed {
                    format!(" v20=bitcast.{index} v7\n")
                } else {
                    String::new()
                };
                let args = match recipe {
                    Recipe::Full | Recipe::Local => format!("{merge},v5,v7,{indexarg}"),
                    Recipe::TwoIndex | Recipe::TwoTable => format!("{merge},v5,v7,{indexarg},v9"),
                    Recipe::Immediate => format!("{merge},v5,v7,v10"),
                    Recipe::PairImmediate => format!("{merge},v5,v7,v9,v10"),
                    Recipe::Pair => format!("{merge},v5,v7,v9"),
                    Recipe::Unary => format!("{merge},v5,v7"),
                };
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:i8):\n v6=load.{shape} align 1 v0\n v7=load.{shape} align 1 v1\n v8=load.{index} align 1 v2\n v9=load.{shape} align 1 v3\n v10=iconst.i32 {control}\n{bitcast}"
                );
                if alias {
                    ir += " v21=load.i64x8 align 1 v0\n";
                }
                ir += &format!(
                    " v11=intrinsic {}({args})\n store align 1 v11,v4\n v12=iconst.i64 64\n v13=ptradd v4,v12\n store align 1 v7,v13\n v14=ptradd v13,v12\n store align 1 v9,v14\n v15=ptradd v14,v12\n store align 1 v8,v15\n v16=ptradd v15,v12\n store align 1 v6,v16\n v17=ptradd v16,v12\n store align 1 v5,v17\n",
                    id.info().name
                );
                if alias {
                    ir += " v18=ptradd v17,v12\n store align 1 v21,v18\n";
                }
                ir += " return\n}\n";
                let asm = match recipe {
                    Recipe::Full => format!("{opcode} %[a],%[i],%[o]"),
                    Recipe::Local => format!("{opcode} %[i],%[a],%[o]"),
                    Recipe::TwoIndex => format!("{opcode} %[b],%[a],%[o]"),
                    Recipe::TwoTable => format!("{opcode} %[b],%[i],%[o]"),
                    Recipe::Immediate => format!("{opcode} ${control},%[a],%[o]"),
                    Recipe::PairImmediate => format!("{opcode} ${control},%[b],%[a],%[o]"),
                    Recipe::Pair => format!("{opcode} %[b],%[a],%[o]"),
                    Recipe::Unary => format!("{opcode} %[a],%[o]"),
                };
                let initial = match recipe {
                    Recipe::TwoIndex => "o=i;",
                    Recipe::TwoTable => "o=a;",
                    _ => "",
                };
                let constraint = if matches!(recipe, Recipe::TwoIndex | Recipe::TwoTable) {
                    "+&v"
                } else {
                    "=&v"
                };
                let ip = if alias && indexed { "ap" } else { "ip" };
                let mp = if alias { "ap" } else { "mp" };
                let bytes = width / 8;
                c += &format!(
                    "extern void {name}(void*,void*,void*,void*,void*,uint8_t);\nstatic void reference_{name}(void*mp,void*ap,void*ip,void*bp,void*out,uint8_t k){{__m{width}i a,i,b,o;memcpy(&a,ap,{bytes});memcpy(&i,{ip},{bytes});memcpy(&b,bp,{bytes});{initial}__asm__ volatile(\"{asm}\":[o]\"{constraint}\"(o):[a]\"v\"(a),[i]\"v\"(i),[b]\"v\"(b):\"memory\");memcpy(out,&o,{bytes});for(unsigned lane=0;lane<{};lane++)if(!(k&(1u<<lane)))memcpy((char*)out+lane*({bits}/8),(char*){mp}+lane*({bits}/8),{bits}/8);}}\n",
                    shape.lanes()
                );
                cases += &format!(
                    "{{{name},reference_{name},{width},{bits},{operation},{control},{},\"{name}\"}},\n",
                    u8::from(alias)
                );
            }
        }
    }
    c += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    c += r#"
static uint64_t read_lane(void*p,unsigned bits,unsigned lane){uint64_t x=0;memcpy(&x,(char*)p+lane*(bits/8),bits/8);return x;}
static uint64_t scalar(struct testcase t,void*a,void*i,void*b,unsigned lane){unsigned n=t.width/t.bits,local=128/t.bits,group=(lane/local)*local,j=lane%local,ix=0;uint64_t index=read_lane(i,t.bits,lane);void*source=a;switch(t.operation){case 0:ix=index&(n-1);break;case 1:ix=index&(2*n-1);if(ix>=n){source=b;ix-=n;}break;case 2:ix=group+(index&3);break;case 3:ix=group+((index>>1)&1);break;case 4:case 9:ix=group+((t.control>>(2*j))&3);break;case 5:ix=group+((t.control>>lane)&1);break;case 6:ix=(t.control>>(2*lane))&3;break;case 7:source=j<2?a:b;ix=group+((t.control>>(2*j))&3);break;case 8:source=j?b:a;ix=group+((t.control>>lane)&1);break;case 10:{unsigned block=lane/local;source=block?b:a;ix=((t.control>>block)&1)*local+j;break;}case 11:ix=(t.control&(n-1))+lane;if(ix<n)source=b;else ix-=n;break;case 12:case 13:source=j&1?b:a;ix=group+(t.operation==13?local/2:0)+j/2;break;case 14:ix=lane|1;break;case 15:case 16:ix=lane&~1u;break;default:return 0;}return read_lane(source,t.bits,ix);}
int main(void){unsigned saved=_mm_getcsr();uint64_t values[]={0,UINT64_MAX,UINT64_C(0x800000007f812345),UINT64_C(0x7ff012347fc54321),UINT64_C(0xfff05432ffc12345),UINT64_C(0x3ff00000bf800000),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x8000000000000000)};for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase t=cases[ci];unsigned bytes=t.width/8,n=t.width/t.bits;uint64_t m[8],a[8],b[8],index[8];unsigned char out[464],want[64];uint8_t masks[]={0,1,2,0x55,0xaa,0xff,(uint8_t)~((1u<<n)-1)};for(unsigned sample=0;sample<2*n+2;sample++){for(unsigned j=0;j<8;j++){m[j]=values[(j+sample+3)%8];a[j]=values[(j+sample)%8];b[j]=values[(j+sample+5)%8];}for(unsigned lane=0;lane<n;lane++){uint64_t ix=(lane+sample)%(2*n);if(sample&1)ix|=t.bits==32?UINT64_C(0x7f810000):UINT64_C(0x7ff0123400000000);memcpy((char*)index+lane*(t.bits/8),&ix,t.bits/8);}for(unsigned mi=0;mi<7;mi++){unsigned csr=(saved&~0x80u)|0x3f;_mm_setcsr(csr);memset(out,0xa5,sizeof out);t.reference(m,a,index,b,want,masks[mi]);t.run(m,a,index,b,out,masks[mi]);if(_mm_getcsr()!=csr||memcmp(out,want,bytes)){fprintf(stderr,"%s sample=%u mask=%x system mismatch\n",t.name,sample,masks[mi]);return 1;}for(unsigned lane=0;lane<n;lane++){uint64_t expected=(masks[mi]&(1u<<lane))?scalar(t,a,t.alias?(void*)a:(void*)index,b,lane):read_lane(t.alias?(void*)a:(void*)m,t.bits,lane);if(read_lane(out,t.bits,lane)!=expected){fprintf(stderr,"%s sample=%u lane=%u expected=%llx got=%llx\n",t.name,sample,lane,(unsigned long long)expected,(unsigned long long)read_lane(out,t.bits,lane));return 2;}}for(unsigned j=bytes;j<64;j++)if(out[j]!=0xa5)return 3;if(memcmp(out+64,a,bytes)||memcmp(out+128,b,bytes)||memcmp(out+192,index,bytes)||memcmp(out+256,m,bytes)||out[320]!=masks[mi]||(t.alias&&memcmp(out+384,m,64)))return 4;}}}_mm_setcsr(saved);return 0;}
"#;
    (ir, c)
}
#[test]
fn vl_permutations_raw_controls_selectors_aliases_and_spills() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(!features.avx2 && !features.avx512bw && !features.avx512dq);
    let directory = std::env::temp_dir().join(format!("keel-vl-permute-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux")) && {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512vl")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    };
    if native {
        let output = Command::new("cc")
            .args([
                "-O1",
                "-fno-tree-vectorize",
                "-mavx512f",
                "-mavx512vl",
                "-c",
            ])
            .arg(directory.join("oracle.c"))
            .arg("-o")
            .arg(directory.join("oracle.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &ONE, &TWELVE] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocation,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            fs::write(directory.join("permute.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("permute.o"))
                    .arg("-o")
                    .arg(directory.join("run"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(directory.join("run"))
                        .status()
                        .unwrap()
                        .success(),
                    "{allocation:?} pools={pools:?}"
                );
            }
        }
    }
}
#[test]
fn vl_permutation_matrix_raw_controls_and_per_function_features() {
    assert_eq!(CASES.len(), 66);
    for (id, shape, _, _, _) in CASES {
        let descriptor = id.x86_avx512_vl_permute().unwrap();
        assert_eq!(descriptor.vector, shape);
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure && !inst.may_trap());
        assert_eq!(inst.memory_effects(), MemoryEffects::None);
    }
    for control in [-1, 256] {
        let invalid = format!(
            "func @invalid(i32x4,i8,i32x4)->i32x4 {{\nblock0(v0:i32x4,v1:i8,v2:i32x4):\n v3=iconst.i32 {control}\n v4=intrinsic x86_avx512vl_vpshufd128(v0,v1,v2,v3)\n return v4\n}}\n"
        );
        assert!(keel_ir::verify_module(&keel_ir::parse(&invalid).unwrap()).is_err());
    }
    let source = "func @operation(i32x4,i8,i32x4,i32x4,i32x4)->i32x4 {\nblock0(v0:i32x4,v1:i8,v2:i32x4,v3:i32x4,v4:i32x4):\n v5=intrinsic x86_avx512vl_vpermi2d128(v0,v1,v2,v3,v4)\n return v5\n}\n";
    let compile = |source: &str, features| {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
    };
    for features in [
        Features::NONE,
        Features::NONE.with_features("avx512f").unwrap(),
        Features::NONE.with_features("avx2").unwrap(),
    ] {
        assert!(compile(source, features).is_err());
    }
    compile(
        &source.replace("->i32x4 {", "->i32x4 target_features(\"avx512vl\") {"),
        Features::NONE,
    )
    .unwrap();
    assert!(
        compile(
            &source.replace("->i32x4 {", "->i32x4 target_features(\"-avx512vl\") {"),
            Features::NONE.with_features("avx512vl").unwrap()
        )
        .is_err()
    );
}
