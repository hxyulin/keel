//! Packed-string controls and signed length widths with exact ISA oracles.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::Intrinsic;
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

#[path = "common/string_allocation.rs"]
mod string_allocation;
const ONE_SIMD: Pools = Pools {
    caller_saved: &[6, 7, 8, 9],
    callee_saved: &[3, 12, 13, 14, 15],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
/// The imm8 control reaches the encoding unchanged, so boundary controls cover
/// its lowering. One native configuration still executes all 256 controls.
const BOUNDARY_CONTROLS: [u8; 11] = [0, 1, 2, 4, 8, 16, 32, 64, 127, 128, 255];
#[derive(Clone, Copy, PartialEq)]
enum Fixture {
    Narrow,
    #[cfg_attr(
        not(all(target_os = "linux", target_arch = "x86_64")),
        allow(dead_code)
    )]
    AllControls,
    Wide,
}
struct Case {
    id: Intrinsic,
    mode: u8,
    alias: bool,
    wide: bool,
}
impl Case {
    fn name(&self) -> String {
        format!("str_{}_{}_{}_{}", self.id, self.mode, self.alias, self.wide)
    }
}
fn cases(fixture: Fixture) -> Vec<Case> {
    let wide = fixture == Fixture::Wide;
    let mut v = Vec::new();
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|i| i.is_x86_string() && !i.x86_string_info().unwrap().0.starts_with('v'))
    {
        let (_, _, kind) = id.x86_string_info().unwrap();
        if wide && kind != 1 {
            continue;
        }
        for mode in 0..=255 {
            let selected = match fixture {
                Fixture::Narrow => BOUNDARY_CONTROLS.contains(&mode),
                Fixture::AllControls => true,
                Fixture::Wide => matches!(mode, 0 | 64 | 127 | 255),
            };
            if !selected {
                continue;
            }
            for alias in [false, true] {
                v.push(Case {
                    id,
                    mode,
                    alias,
                    wide,
                });
            }
        }
    }
    v
}
fn source(fixture: Fixture) -> String {
    let wide = fixture == Fixture::Wide;
    let mut s = String::new();
    for c in cases(fixture) {
        let info = c.id.info();
        let (_, index, _) = c.id.x86_string_info().unwrap();
        let name = c.name();
        let out = info.returns[0].bytes().unwrap();
        let a_bytes = if wide { 32 } else { 16 };
        s += &format!("func @{name}(ptr,ptr,ptr,ptr){{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr):\n");
        if wide {
            s += " v4 = load.i8x32 align 1 v0\n";
        }
        s += " v5 = load.i8x16 align 1 v0\n v6 = load.i8x16 align 1 v1\n";
        if index == 4 {
            let ty = info.params[2];
            s += &format!(
                " v7 = load.{ty} align 1 v2\n v8 = iconst.i64 8\n v9 = ptradd v2,v8\n v10 = load.{ty} align 1 v9\n"
            );
        }
        s += &format!(
            " v11 = iconst.i32 {}\n v12 = intrinsic {}(v5,{}{})\n store align 1 v12,v3\n v13 = iconst.i64 {out}\n v14 = ptradd v3,v13\n store align 1 {},v14\n v15 = iconst.i64 {a_bytes}\n v16 = ptradd v14,v15\n store align 1 v6,v16\n",
            c.mode,
            c.id,
            if c.alias { "v5" } else { "v6" },
            if index == 4 { ",v7,v10,v11" } else { ",v11" },
            if wide { "v4" } else { "v5" }
        );
        if index == 4 {
            let n = info.params[2].bytes().unwrap();
            s += &format!(
                " v17 = iconst.i64 16\n v18 = ptradd v16,v17\n store align 1 v7,v18\n v19 = iconst.i64 {n}\n v20 = ptradd v18,v19\n store align 1 v10,v20\n"
            );
        }
        s += " return\n}\n";
    }
    s
}
fn opcode_byte(op: &str) -> u8 {
    if op.contains("estrm") {
        0x60
    } else if op.contains("estri") {
        0x61
    } else if op.contains("istrm") {
        0x62
    } else {
        0x63
    }
}
fn harness(fixture: Fixture) -> String {
    let wide = fixture == Fixture::Wide;
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\nextern void*memcpy(void*,const void*,__SIZE_TYPE__);extern void*memset(void*,int,__SIZE_TYPE__);extern int memcmp(const void*,const void*,__SIZE_TYPE__);extern int printf(const char*,...);\n",
    );
    let mut table = String::from(
        "struct Case{void(*run)(const void*,const void*,const void*,void*);void(*ref)(const void*,const void*,const void*,void*);unsigned out,abytes,len;const char*name;};static const struct Case cases[]={\n",
    );
    for ca in cases(fixture) {
        let info = ca.id.info();
        let (op, index, kind) = ca.id.x86_string_info().unwrap();
        let name = ca.name();
        let out = info.returns[0].bytes().unwrap();
        let len = if index == 4 {
            info.params[2].bytes().unwrap()
        } else {
            0
        };
        let w = op.ends_with("64");
        c += &format!(
            "extern void {name}(const void*,const void*,const void*,void*);__attribute__((noinline))static void ref_{name}(const void*p,const void*q,const void*l,void*out){{register __m128i a __asm__(\"xmm1\")=_mm_loadu_si128((const __m128i*)p);register __m128i b __asm__(\"xmm2\")={};",
            if ca.alias {
                "a"
            } else {
                "_mm_loadu_si128((const __m128i*)q)"
            }
        );
        if len != 0 {
            c += &format!(
                "int{}_t la,lb;memcpy(&la,l,{len});memcpy(&lb,(const char*)l+8,{len});",
                len * 8
            );
        }
        let instruction = format!(
            ".byte 0x66,{}0x0f,0x3a,{},0xca,{}",
            if w { "0x48," } else { "" },
            opcode_byte(op),
            ca.mode
        );
        let inputs = if len != 0 {
            "\"x\"(a),\"x\"(b),\"a\"(la),\"d\"(lb)"
        } else {
            "\"x\"(a),\"x\"(b)"
        };
        match kind {
            0 => {
                c += &format!(
                    "uint64_t e;__asm__ volatile(\"{instruction}\":\"=c\"(e):{inputs}:\"cc\",\"memory\");memcpy(out,&e,{out});}}\n"
                )
            }
            1 => {
                c += &format!(
                    "register __m128i e __asm__(\"xmm0\");__asm__ volatile(\"{instruction}\":\"=x\"(e):{inputs}:\"cc\",\"memory\");__m128i stored=e;memcpy(out,&stored,16);}}\n"
                )
            }
            2 => {
                c += &format!(
                    "unsigned char cf,zf,sf,of;__asm__ volatile(\"{instruction};setb %0;sete %1;sets %2;seto %3\":\"=qm\"(cf),\"=qm\"(zf),\"=qm\"(sf),\"=qm\"(of):{inputs}:\"rcx\",\"cc\",\"memory\");uint32_t e=cf|zf<<6|sf<<7|of<<11;memcpy(out,&e,4);}}\n"
                )
            }
            _ => unreachable!(),
        }
        table += &format!(
            "{{{name},ref_{name},{out},{},{len},\"{name}\"}},\n",
            if wide { 32 } else { 16 }
        );
    }
    c += &table;
    c += "};\n";
    c += r#"
int main(void){unsigned saved=_mm_getcsr(),csr=saved|0x1f80|63;_mm_setcsr(csr);uint64_t rng=12345;const uint64_t lengths[32]={0,1,2,7,8,9,15,16,17,31,32,~0ULL,~1ULL,(uint64_t)-7,(uint64_t)-8,(uint64_t)-9,(uint64_t)-15,(uint64_t)-16,(uint64_t)-17,0x7fffffff,0x80000000,0x100000000ULL,0x100000001ULL,0x80000001ULL,0x7fffffffffffffffULL,0x8000000000000000ULL,(uint64_t)-0x100000001LL,128,129,255,256,0xffffffff00000001ULL};const unsigned offsets[]={0,1,7,15};
for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++){const struct Case*c=&cases[ci];for(unsigned pattern=0;pattern<1024;pattern++)for(unsigned oi=0;oi<4;oi++)for(unsigned inplace=0;inplace<2;inplace++){
 unsigned char in[160],inb[160],out[160],lens[64],e[96];memset(in,0x5a,160);memset(inb,0x5a,160);memset(out,0x5a,160);memset(lens,0x5a,64);unsigned char*p=in+16+offsets[oi],*q=inb+16+offsets[oi],*r=(inplace?in:out)+16+offsets[oi],*l=lens+1+offsets[oi];unsigned pk=pattern&127;
 for(unsigned i=0;i<c->abytes;i++){rng=rng*6364136223846793005ULL+1442695040888963407ULL;p[i]=pk<32?(unsigned char)(i+1):(unsigned char)(rng>>32);}for(unsigned i=0;i<16;i++){rng=rng*6364136223846793005ULL+1442695040888963407ULL;q[i]=pk<32?(unsigned char)(1+(i+pk)%16):(unsigned char)(rng>>32);}
 if(pk<17){if(pk<16)p[pk]=0;unsigned b=pk*3%17;if(b<16)q[b]=0;}else if(pk<26){unsigned a=(pk-17)%9,b=(pk-17)*3%9;if(a<8){p[2*a]=p[2*a+1]=0;}if(b<8){q[2*b]=q[2*b+1]=0;}}else if(pk<32){memset(p,pk&1?0xff:0,c->abytes);memset(q,pk&2?0x80:0x7f,16);}else if(pk<64){for(unsigned i=0;i<16;i++)q[i]=p[(i+pk)%16];}
 uint64_t la=lengths[pattern%32],lb=lengths[(pattern/32)%32];memcpy(l,&la,8);memcpy(l+8,&lb,8);memcpy(e+c->out,p,c->abytes);memcpy(e+c->out+c->abytes,q,16);if(c->len){memcpy(e+c->out+c->abytes+16,l,c->len);memcpy(e+c->out+c->abytes+16+c->len,l+8,c->len);}unsigned total=c->out+c->abytes+16+2*c->len;c->ref(p,q,l,e);c->run(p,q,l,r);if(memcmp(r,e,total)||r[-1]!=0x5a||r[total]!=0x5a){printf("strings failed %s pattern%u offset%u inplace%u\n",c->name,pattern,offsets[oi],inplace);return 10;}
}}
if(_mm_getcsr()!=csr)return 11;_mm_setcsr(saved);return 0;}
"#;
    c
}
#[path = "common/matrix.rs"]
mod matrix;

fn object(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
    fixture: Fixture,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    object_with_forced_wide(s, a, empty, f, fixture, false)
}
fn object_with_forced_wide(
    s: SelectionTier,
    a: AllocationTier,
    empty: bool,
    f: Features,
    fixture: Fixture,
    force: bool,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    use std::sync::OnceLock;
    static NARROW: OnceLock<matrix::PreparedModule> = OnceLock::new();
    static ALL_CONTROLS: OnceLock<matrix::PreparedModule> = OnceLock::new();
    static WIDE: OnceLock<matrix::PreparedModule> = OnceLock::new();
    static FORCED: OnceLock<matrix::PreparedModule> = OnceLock::new();
    let wide = fixture == Fixture::Wide;
    let prepared = if force {
        FORCED.get_or_init(|| matrix::PreparedModule::parse(&forced_source()))
    } else {
        match fixture {
            Fixture::Narrow => &NARROW,
            Fixture::AllControls => &ALL_CONTROLS,
            Fixture::Wide => &WIDE,
        }
        .get_or_init(|| matrix::PreparedModule::parse(&source(fixture)))
    };
    let m = prepared.for_selector(s);
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let forced = string_allocation::LiveWide0(a);
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: if force { &forced } else { &a },
    };
    let pools = if empty {
        &EMPTY
    } else if wide {
        &ONE_SIMD
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

fn forced_source() -> String {
    let original = source(Fixture::Wide);
    let mut s = String::new();
    for (case, function) in cases(Fixture::Wide)
        .into_iter()
        .zip(original.split(" return\n}\n").filter(|f| !f.is_empty()))
    {
        let (_, index, _) = case.id.x86_string_info().unwrap();
        let len = if index == 4 {
            case.id.info().params[2].bytes().unwrap()
        } else {
            0
        };
        s += &function.replace(
            " v12 = intrinsic",
            " v24 = intrinsic x86_avx2_vpmovmskb256(v4)\n v12 = intrinsic",
        );
        s += &format!(
            " v25 = iconst.i64 {}\n v26 = ptradd v3,v25\n store align 1 v24,v26\n return\n}}\n",
            64 + 2 * len
        );
    }
    s
}
fn forced_harness() -> String {
    harness(Fixture::Wide).replace("c->ref(p,q,l,e);c->run", "uint32_t signbits=0;for(unsigned i=0;i<32;i++)signbits|=(uint32_t)(p[i]>>7)<<i;memcpy(e+total,&signbits,4);total+=4;c->ref(p,q,l,e);c->run")
}
#[test]
fn forced_live_ymm0_string_mask_exports() {
    let export = std::env::var_os("KEEL_X86_256_EXPORT_DIR").map(PathBuf::from);
    let prefix = "legacy-strings-forced-wide";
    let temp = common::temp("legacy-strings-forced-syntax");
    fs::create_dir_all(&temp).unwrap();
    fs::write(temp.join("forced.c"), forced_harness()).unwrap();
    common::command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-O2",
                "-march=x86-64",
                "-mno-avx",
                "-c",
            ])
            .arg(temp.join("forced.c"))
            .arg("-o")
            .arg(temp.join("forced.o")),
    );
    fs::remove_dir_all(temp).unwrap();

    if let Some(d) = &export {
        fs::create_dir_all(d).unwrap();
        fs::write(d.join(format!("{prefix}.ir")), forced_source()).unwrap();
        fs::write(d.join(format!("{prefix}.c")), forced_harness()).unwrap();
    }
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            let (b, text) = object_with_forced_wide(
                s,
                a,
                false,
                Features::for_level(CpuLevel::V3),
                Fixture::Wide,
                true,
            )
            .unwrap();
            assert!(
                text.contains("vmovups\tymm0, ymmword ptr"),
                "wide definition must occupy YMM0"
            );
            assert!(
                text.lines()
                    .any(|l| l.contains("vmovups\tymmword ptr") && l.ends_with(", ymm0")),
                "wide use must still read YMM0"
            );
            if let Some(d) = &export {
                let name = format!("{prefix}-{s:?}-{a:?}");
                fs::write(d.join(format!("{name}.o")), b).unwrap();
                fs::write(d.join(format!("{name}.s")), text).unwrap();
            }
        }
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_forced_live_ymm0_across_string_masks() {
    if !std::is_x86_feature_detected!("avx2") || !std::is_x86_feature_detected!("sse4.2") {
        return;
    }
    let d = common::temp("legacy-strings-forced-wide-native");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("forced.c"), forced_harness()).unwrap();
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            let (b, _) = object_with_forced_wide(
                s,
                a,
                false,
                Features::for_level(CpuLevel::V3),
                Fixture::Wide,
                true,
            )
            .unwrap();
            fs::write(d.join("forced.o"), b).unwrap();
            common::command(
                Command::new("clang")
                    .args(["-O2", "-march=x86-64", "-mno-avx"])
                    .arg(d.join("forced.c"))
                    .arg(d.join("forced.o"))
                    .arg("-o")
                    .arg(d.join("forced")),
            );
            common::command(&mut Command::new(d.join("forced")));
        }
    }
    fs::remove_dir_all(d).unwrap();
}
#[test]
fn exhaustive_legacy_strings_controls_register_spill_and_wide_alias_exports() {
    check_string_exports(true);
}

#[test]
fn legacy_strings_boundary_smoke() {
    check_string_exports(false);
}

fn check_string_exports(exhaustive: bool) {
    let export = exhaustive
        .then(|| std::env::var_os("KEEL_X86_256_EXPORT_DIR"))
        .flatten()
        .map(PathBuf::from);
    let d = common::temp(if exhaustive {
        "legacy-string-oracle"
    } else {
        "legacy-string-smoke"
    });
    fs::create_dir_all(&d).unwrap();
    let mut configurations = 0;
    for fixture in [Fixture::Narrow, Fixture::Wide] {
        let wide = fixture == Fixture::Wide;
        let name = if wide {
            "legacy-strings-wide"
        } else {
            "legacy-strings"
        };
        let c = harness(fixture);
        fs::write(d.join("oracle.c"), &c).unwrap();
        common::command(
            Command::new("clang")
                .args([
                    "-target",
                    "x86_64-unknown-linux-gnu",
                    "-ffreestanding",
                    "-O2",
                    "-march=x86-64",
                    "-mno-avx",
                    "-c",
                ])
                .arg(d.join("oracle.c"))
                .arg("-o")
                .arg(d.join("oracle.o")),
        );
        if let Some(e) = &export {
            fs::create_dir_all(e).unwrap();
            fs::write(e.join(format!("{name}.ir")), source(fixture)).unwrap();
            fs::write(e.join(format!("{name}.c")), c).unwrap();
        }
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for empty in [false, true] {
                    if !exhaustive
                        && !matches!(
                            (s, a, empty),
                            (SelectionTier::Baseline, AllocationTier::Fast, false)
                                | (SelectionTier::Full, AllocationTier::Full, true)
                        )
                    {
                        continue;
                    }
                    let (b, text) = object(
                        s,
                        a,
                        empty,
                        Features::for_level(if wide { CpuLevel::V3 } else { CpuLevel::V2 }),
                        fixture,
                    )
                    .unwrap();
                    configurations += 1;
                    if let Some(e) = &export {
                        let n = format!("{name}-{s:?}-{a:?}-{empty}");
                        fs::write(e.join(format!("{n}.o")), b).unwrap();
                        fs::write(e.join(format!("{n}.s")), text).unwrap();
                    }
                }
            }
        }
    }
    assert_eq!(configurations, if exhaustive { 24 } else { 4 });
    fs::remove_dir_all(d).unwrap();
}
#[test]
fn string_features_controls_lengths_purity_and_foreign_target() {
    assert!(
        object(
            SelectionTier::Baseline,
            AllocationTier::Full,
            false,
            Features::NONE,
            Fixture::Narrow
        )
        .is_err()
    );
    let small = source(Fixture::Wide);
    let bad = small.replace("iconst.i32 255", "iconst.i32 256");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let bad = small.replace("iconst.i32 255", "load.i32 align 1 v0");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    let bad = small.replace("load.i64 align 1", "load.i32 align 1");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    for c in cases(Fixture::Narrow) {
        assert!(c.id.info().pure);
    }
    let narrow = source(Fixture::Narrow)
        .split("func @str_x86_sse42_pcmpistri_index_1_")
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
    let mut f = Features::NONE;
    f.sse42 = true;
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, &EMPTY, &pipeline).unwrap();
    keel_asm::x86::assemble_object_with_features(&text, f).unwrap();
    let unused = narrow
        .lines()
        .filter(|l| !l.trim_start().starts_with("store "))
        .collect::<Vec<_>>()
        .join("\n");
    let mut m = keel_ir::parse(&unused).unwrap();
    keel_opt::optimize(&mut m).unwrap();
    for f in &m.functions {
        assert!(!f.layout_insts().any(|(_,i)|matches!(f.inst_data(i),keel_ir::InstData::Intrinsic{id,..}if id.is_x86_string())));
    }
}
#[test]
fn exact_oracle_instruction_bytes_gnu_and_clang() {
    use object::{Object, ObjectSection};
    let d = common::temp("legacy-string-bytes");
    fs::create_dir_all(&d).unwrap();
    for op in ["pcmpestri", "pcmpestrm", "pcmpistri", "pcmpistrm"] {
        for w in [false, true] {
            if w && op == "pcmpistrm" {
                continue;
            }
            let code = format!(
                ".intel_syntax noprefix\n.text\n {}{op} xmm1,xmm2,3\n",
                if w { "rex64 " } else { "" }
            );
            fs::write(d.join("oracle.s"), code).unwrap();
            let mut expected = vec![0x66];
            if w {
                expected.push(0x48);
            }
            expected.extend([0x0f, 0x3a, opcode_byte(op), 0xca, 3]);
            for gnu in [true, false] {
                if !gnu && w {
                    continue;
                }
                let mut command = if gnu {
                    let mut c = Command::new(
                        if std::path::Path::new("/opt/homebrew/bin/x86_64-elf-as").exists() {
                            "/opt/homebrew/bin/x86_64-elf-as"
                        } else {
                            "as"
                        },
                    );
                    c.arg("--64");
                    c
                } else {
                    let mut c = Command::new("clang");
                    c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
                    c
                };
                common::command(
                    command
                        .arg(d.join("oracle.s"))
                        .arg("-o")
                        .arg(d.join("oracle.o")),
                );
                let bytes = fs::read(d.join("oracle.o")).unwrap();
                let obj = object::File::parse(&*bytes).unwrap();
                assert_eq!(
                    obj.section_by_name(".text").unwrap().data().unwrap(),
                    expected
                );
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn exhaustive_native_legacy_strings_all_controls_lengths_flags_aliases() {
    if !std::is_x86_feature_detected!("sse4.2") {
        return;
    }
    let d = common::temp("legacy-strings-native");
    fs::create_dir_all(&d).unwrap();
    for wide in [false, true] {
        if wide && !std::is_x86_feature_detected!("avx") {
            continue;
        }
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for empty in [false, true] {
                    let fixture = if wide {
                        Fixture::Wide
                    } else if (s, a, empty)
                        == (SelectionTier::Baseline, AllocationTier::Fast, false)
                    {
                        Fixture::AllControls
                    } else {
                        Fixture::Narrow
                    };
                    fs::write(d.join("strings.c"), harness(fixture)).unwrap();
                    let (b, _) = object(
                        s,
                        a,
                        empty,
                        Features::for_level(if wide { CpuLevel::V3 } else { CpuLevel::V2 }),
                        fixture,
                    )
                    .unwrap();
                    fs::write(d.join("strings.o"), b).unwrap();
                    common::command(
                        Command::new("clang")
                            .args(["-O2", "-march=x86-64", "-mno-avx"])
                            .arg(d.join("strings.c"))
                            .arg(d.join("strings.o"))
                            .arg("-o")
                            .arg(d.join("strings")),
                    );
                    common::command(&mut Command::new(d.join("strings")));
                }
            }
        }
    }
    fs::remove_dir_all(d).unwrap();
}
