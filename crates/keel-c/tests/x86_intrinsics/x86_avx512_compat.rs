//! Compatibility aliases retain low-half, FIXUP, FP and wrapping multiplication contracts.
use super::pclmul::parse;
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
struct Case {
    name: String,
    ty: &'static str,
    expression: String,
    evaluations: usize,
    indexed: bool,
    masked: bool,
}
fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    let mut add =
        |name: &str, ty: &'static str, expression: String, evaluations, indexed, masked| {
            result.push(Case {
                name: name.into(),
                ty,
                expression,
                evaluations,
                indexed,
                masked,
            })
        };
    for width in [32, 64] {
        add(
            &format!("loadu_epi{width}"),
            "__m512i",
            format!("_mm512_loadu_epi{width}(take_ptr(a))"),
            1,
            false,
            false,
        );
        add(
            &format!("storeu_epi{width}"),
            "void",
            format!("_mm512_storeu_epi{width}(take_ptr(a),take_i(b))"),
            2,
            false,
            false,
        );
    }
    for (name, input, merge, ty) in [
        ("cvtepi32lo_pd", "take_i(a)", "take_pd(m)", "__m512d"),
        ("cvtepu32lo_pd", "take_i(a)", "take_pd(m)", "__m512d"),
        ("cvtpslo_pd", "take_ps(a)", "take_pd(m)", "__m512d"),
        ("cvtpd_pslo", "take_pd(a)", "take_ps(m)", "__m512"),
    ] {
        add(name, ty, format!("_mm512_{name}({input})"), 1, false, false);
        add(
            &format!("mask_{name}"),
            ty,
            format!("_mm512_mask_{name}({merge},take_mask(k),{input})"),
            3,
            false,
            false,
        );
    }
    for (suffix, reader, ty) in [
        ("epi64", "take_i(b)", "__m512i"),
        ("pd", "take_pd(b)", "__m512d"),
    ] {
        for scale in [1, 2, 4, 8] {
            let merge = if suffix == "pd" {
                "take_pd(m)"
            } else {
                "take_i(m)"
            };
            add(
                &format!("i32logather_{suffix}_{scale}"),
                ty,
                format!("_mm512_i32logather_{suffix}(take_i(a),take_ptr(c),{scale})"),
                2,
                true,
                false,
            );
            add(
                &format!("mask_i32logather_{suffix}_{scale}"),
                ty,
                format!(
                    "_mm512_mask_i32logather_{suffix}({merge},take_mask(k),take_i(a),take_ptr(c),{scale})"
                ),
                4,
                true,
                true,
            );
            add(
                &format!("i32loscatter_{suffix}_{scale}"),
                "void",
                format!("_mm512_i32loscatter_{suffix}(take_ptr(c),take_i(a),{reader},{scale})"),
                3,
                true,
                false,
            );
            add(
                &format!("mask_i32loscatter_{suffix}_{scale}"),
                "void",
                format!(
                    "_mm512_mask_i32loscatter_{suffix}(take_ptr(c),take_mask(k),take_i(a),{reader},{scale})"
                ),
                4,
                true,
                true,
            );
        }
    }
    add(
        "int2mask",
        "__mmask16",
        "_mm512_int2mask(take_int((int)k))".into(),
        1,
        false,
        false,
    );
    add(
        "mask2int",
        "int",
        "_mm512_mask2int(take_mask(k))".into(),
        1,
        false,
        false,
    );
    add(
        "kmov",
        "__mmask16",
        "_mm512_kmov(take_mask(k))".into(),
        1,
        false,
        false,
    );
    add(
        "permutevar_epi32",
        "__m512i",
        "_mm512_permutevar_epi32(take_i(a),take_i(b))".into(),
        2,
        false,
        false,
    );
    add(
        "mask_permutevar_epi32",
        "__m512i",
        "_mm512_mask_permutevar_epi32(take_i(m),take_mask(k),take_i(a),take_i(b))".into(),
        4,
        false,
        false,
    );
    for (name, ty, expression) in [
        (
            "undefined",
            "__m512",
            "_mm512_mask_mov_ps(_mm512_undefined(),0xffff,take_ps(a))",
        ),
        (
            "undefined_ps",
            "__m512",
            "_mm512_mask_mov_ps(_mm512_undefined_ps(),0xffff,take_ps(a))",
        ),
        (
            "undefined_pd",
            "__m512d",
            "_mm512_mask_mov_pd(_mm512_undefined_pd(),0xff,take_pd(a))",
        ),
        (
            "undefined_epi32",
            "__m512i",
            "_mm512_mask_mov_epi32(_mm512_undefined_epi32(),0xffff,take_i(a))",
        ),
    ] {
        add(name, ty, expression.into(), 1, false, false);
    }
    for (suffix, reader, ty, table) in [
        ("ps", "take_ps", "__m512", "take_i(c)"),
        ("pd", "take_pd", "__m512d", "take_i(c)"),
        ("ss", "take_ss", "__m128", "take_x(c)"),
        ("sd", "take_sd", "__m128d", "take_x(c)"),
    ] {
        let prefix = if suffix == "ss" || suffix == "sd" {
            "_mm"
        } else {
            "_mm512"
        };
        for imm in [0, 255] {
            for round in [false, true] {
                let name = format!(
                    "maskz_fixupimm{}_{suffix}",
                    if round { "_round" } else { "" }
                );
                let extra = if round { ",8" } else { "" };
                add(
                    &format!("{name}_{imm}"),
                    ty,
                    format!(
                        "{prefix}_{name}(take_mask(k),{reader}(a),{reader}(b),{table},{imm}{extra})"
                    ),
                    4,
                    false,
                    false,
                );
            }
        }
    }
    for (suffix, reader, ty) in [("ps", "take_ps", "__m512"), ("pd", "take_pd", "__m512d")] {
        for operation in ["abs", "ceil", "floor"] {
            let name = format!("{operation}_{suffix}");
            add(
                &name,
                ty,
                format!("_mm512_{name}({reader}(a))"),
                1,
                false,
                false,
            );
            add(
                &format!("mask_{name}"),
                ty,
                format!("_mm512_mask_{name}({reader}(m),take_mask(k),{reader}(a))"),
                3,
                false,
                false,
            );
        }
    }
    add(
        "mullox_epi64",
        "__m512i",
        "_mm512_mullox_epi64(take_i(a),take_i(b))".into(),
        2,
        false,
        false,
    );
    add(
        "mask_mullox_epi64",
        "__m512i",
        "_mm512_mask_mullox_epi64(take_i(m),take_mask(k),take_i(a),take_i(b))".into(),
        4,
        false,
        false,
    );
    result
}
fn sources() -> (String, String) {
    let mut source = String::from(
        "#include <immintrin.h>\n#ifdef REFERENCE\n#define FN(n) reference_##n\n#else\n#define FN(n) n\n#endif\nint FN(evaluations);\n#define READER(name,type,width) static __inline__ __attribute__((always_inline)) type name(const void*p){type v;FN(evaluations)++;__builtin_memcpy(&v,p,width);return v;}\nREADER(take_i,__m512i,64)\nREADER(take_ps,__m512,64)\nREADER(take_pd,__m512d,64)\nREADER(take_ss,__m128,16)\nREADER(take_sd,__m128d,16)\nREADER(take_x,__m128i,16)\nstatic __inline__ __attribute__((always_inline)) void*take_ptr(const void*p){FN(evaluations)++;return (void*)p;}\nstatic __inline__ __attribute__((always_inline)) unsigned short take_mask(unsigned k){FN(evaluations)++;return k;}\nstatic __inline__ __attribute__((always_inline)) int take_int(int k){FN(evaluations)++;return k;}\n",
    );
    let mut harness = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\nextern int evaluations,reference_evaluations;\ntypedef void(*Fn)(const void*,const void*,const void*,const void*,void*,unsigned);\nstruct Case{Fn actual,reference;unsigned bytes,evaluations,indexed,masked;const char*name;};\n",
    );
    let mut rows = String::new();
    for (i, case) in cases().into_iter().enumerate() {
        let bytes = match case.ty {
            "void" => 0,
            "__mmask16" => 2,
            "int" => 4,
            "__m128" | "__m128d" => 16,
            _ => 64,
        };
        // Clang models this legacy wrapper as an unmasked conversion plus select.
        // Keep its value/evaluation oracle, but derive flags from masked hardware.
        let body = if case.name == "mask_cvtpslo_pd" {
            format!(
                "\n#ifdef REFERENCE\nunsigned before=_mm_getcsr();__m512d result={};_mm_setcsr(before);__m512d hardware;__m256 low;__builtin_memcpy(&hardware,m,64);__builtin_memcpy(&low,a,32);__mmask8 gate=(__mmask8)k;__asm__ volatile(\"vcvtps2pd %[a],%[d] %{{%[k]%}}\":[d]\"+&v\"(hardware):[a]\"v\"(low),[k]\"Yk\"(gate):\"memory\");if(__builtin_memcmp(&hardware,&result,64))__builtin_trap();__builtin_memcpy(out,&result,64);\n#else\n__m512d result={};__builtin_memcpy(out,&result,64);\n#endif\n",
                case.expression, case.expression
            )
        } else if case.ty == "void" {
            format!("{};", case.expression)
        } else {
            format!(
                "{} result={};__builtin_memcpy(out,&result,{bytes});",
                case.ty, case.expression
            )
        };
        source += &format!(
            "void FN(case_{i})(const void*m,const void*a,const void*b,const void*c,void*out,unsigned k){{{body}}}\n"
        );
        harness += &format!(
            "extern void case_{i}(const void*,const void*,const void*,const void*,void*,unsigned);\nextern void reference_case_{i}(const void*,const void*,const void*,const void*,void*,unsigned);\n"
        );
        rows += &format!(
            "{{case_{i},reference_case_{i},{bytes},{},{},{},\"{}\"}},\n",
            case.evaluations,
            u8::from(case.indexed),
            u8::from(case.masked),
            case.name
        );
    }
    harness += &format!(
        "static const struct Case cases[]={{{rows}}};\nint main(void){{unsigned saved=_mm_getcsr();unsigned char m[64],b[64],aa[1024],ra[1024],cc[1024],rc[1024],out[128],expected[128];static const unsigned masks[]={{0,1,0x80,0xffff,0x80010001u}};static const uint32_t values[]={{0,0x80000000,0x3fc00000,0xbfc00000,0x7f800001,0xffc12345,1,0x7f7fffff,0xffffffff,0x80000001,0x4f000000,0x4f800000,0x3f000001,0xbf000001,0x007fffff,0x00800000}};for(unsigned sample=0;sample<5;sample++)for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){{const struct Case*c=&cases[ci];memset(aa,0xa5,sizeof(aa));memset(cc,0x5a,sizeof(cc));for(unsigned j=0;j<16;j++){{uint32_t x=values[(j+sample*3)%16],y=values[(j+sample*3+5)%16],merge=0xa5a51234+j,table=sample==0?0:sample==1?0x11111111u:sample==2?0x22222222u:sample==3?0x88888888u:~0u;memcpy(aa+1+j*4,&x,4);memcpy(b+j*4,&y,4);memcpy(m+j*4,&merge,4);memcpy(cc+1+j*4,&table,4);}}if(c->indexed){{for(unsigned j=0;j<16;j++){{int32_t idx=j<8?(j%2?-(int32_t)(j+1):(int32_t)(j+1)):0x7fffffff;memcpy(aa+1+j*4,&idx,4);}}}}for(unsigned pi=0;pi<5;pi++){{unsigned k=masks[pi];memcpy(ra,aa,sizeof(aa));memcpy(rc,cc,sizeof(cc));const void*base=c->indexed?cc+256:cc+1;const void*rbase=c->indexed?rc+256:rc+1;memset(out,0xa5,sizeof(out));memset(expected,0xa5,sizeof(expected));evaluations=reference_evaluations=0;unsigned csr=0x1f80|((sample%4)<<13);_mm_setcsr(csr);c->actual(m,aa+1,b,base,out,k);unsigned status=_mm_getcsr()&63;_mm_setcsr(csr);c->reference(m,ra+1,b,rbase,expected,k);unsigned wanted=_mm_getcsr()&63;if(memcmp(out,expected,c->bytes)||memcmp(aa,ra,sizeof(aa))||memcmp(cc,rc,sizeof(cc))||status!=wanted||evaluations!=c->evaluations||reference_evaluations!=c->evaluations){{fprintf(stderr,\"%s sample%u mask%x flags%x/%x eval%d/%d expected%u\\n\",c->name,sample,k,status,wanted,evaluations,reference_evaluations,c->evaluations);return 1;}}for(unsigned j=c->bytes;j<sizeof(out);j++)if(out[j]!=0xa5)return 2;if(c->indexed&&c->masked&&!(k&255)){{evaluations=reference_evaluations=0;c->actual(m,aa+1,b,(void*)1,out,k);c->reference(m,ra+1,b,(void*)1,expected,k);if(memcmp(out,expected,c->bytes)||evaluations!=c->evaluations||reference_evaluations!=c->evaluations)return 3;}}}}}}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, harness)
}
fn command(cmd: &mut Command) {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn avx512_compat_alias_controls_and_signatures() {
    let (source, _) = sources();
    let m = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&m).unwrap();
    for text in [
        "__m512i f(__m512i a,void*p,int n){return _mm512_i32logather_epi64(a,p,n);}",
        "__m512 f(__m512 a,__m512 b,__m512i c){return _mm512_maskz_fixupimm_ps(1,a,b,c,256);}",
        "__m128 f(__m128 a,__m128 b,__m128i c,int r){return _mm_maskz_fixupimm_round_ss(1,a,b,c,0,r);}",
    ] {
        assert!(
            parse(
                &format!("#include <immintrin.h>\n{text}\n"),
                Target::X86_64LinuxGnu
            )
            .is_err()
        );
    }
}
#[test]
fn avx512_compat_alias_native_oracle_and_single_evaluation() {
    let (source, harness) = sources();
    let original = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&original).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx512vl && !features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-c-avx512-compat-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), source).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let native = {
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            std::is_x86_feature_detected!("avx512f")
        }
        #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
        {
            false
        }
    };
    if native {
        command(
            Command::new("clang")
                .args(["-O1", "-mavx512f", "-frounding-math", "-DREFERENCE", "-c"])
                .arg(dir.join("source.c"))
                .arg("-o")
                .arg(dir.join("reference.o")),
        );
        command(
            Command::new("cc")
                .args(["-O1", "-mavx512f", "-c"])
                .arg(dir.join("harness.c"))
                .arg("-o")
                .arg(dir.join("harness.o")),
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
    ] {
        let mut module = original.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(dir.join("actual.o"), object).unwrap();
        if native {
            command(
                Command::new("cc")
                    .arg(dir.join("actual.o"))
                    .arg(dir.join("reference.o"))
                    .arg(dir.join("harness.o"))
                    .arg("-o")
                    .arg(dir.join("run")),
            );
            command(&mut Command::new(dir.join("run")));
        }
    }
}
