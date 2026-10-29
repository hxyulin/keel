//! SSE2 widening/bitcasts and the bounded SSE4.1 signed dword register pilot.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[path = "vector_widen_x86/x86_avx_widen_fallback.rs"]
mod x86_avx_widen_fallback;
#[path = "vector_widen_x86/x86_packed_low_load.rs"]
mod x86_packed_low_load;
#[path = "vector_widen_x86/x86_signed_wide_load.rs"]
mod x86_signed_wide_load;
#[path = "vector_widen_x86/x86_signed_widen_load.rs"]
mod x86_signed_widen_load;
#[path = "vector_widen_x86/x86_wide_conversions.rs"]
mod x86_wide_conversions;
#[path = "vector_widen_x86/x86_widen_sum.rs"]
mod x86_widen_sum;

use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
};

use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::x86_features::FeatureSelector;

const PAIRS: [(u32, u32); 6] = [(8, 16), (8, 32), (8, 64), (16, 32), (16, 64), (32, 64)];

fn shape(bits: u32) -> String {
    format!("i{bits}x{}", 128 / bits)
}

fn compile(
    source: &str,
    selection: SelectionTier,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
) -> (String, Vec<u8>) {
    compile_features(
        source,
        selection,
        allocator,
        pools,
        Features::for_level(CpuLevel::V1),
    )
}

fn compile_features(
    source: &str,
    selection: SelectionTier,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
    features: Features,
) -> (String, Vec<u8>) {
    let mut module = keel_ir::parse(source).unwrap();
    // Make the conversion consume an actual resolved SSA alias as well.
    for f in &mut module.functions {
        if f.name == "resolved_alias" {
            use keel_ir::EntityRef;
            f.replace_all_uses(keel_ir::Value::new(1), keel_ir::Value::new(0));
        }
    }
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &selection,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
        &module,
        pools,
        &pipeline,
        &mut |p| chunks.items.extend(p.items),
    )
    .unwrap();
    for p in [&program, &chunks] {
        for item in &p.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(features.contains(i.required_features()), "{i}");
            }
        }
    }
    let direct = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        direct,
        keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap()
    );
    assert_eq!(
        direct,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (text, direct)
}

#[test]
fn all_integer_widening_pairs_stay_packed_with_baseline_sse2() {
    let mut source = String::new();
    for (from, to) in PAIRS {
        for op in ["swiden_low", "uwiden_low"] {
            source += &format!(
                "func @{op}_{from}_{to}({a}) -> {b} {{\nblock0(v0: {a}):\n v1 = {op}.{b} v0\n return v1\n}}\n",
                a = shape(from),
                b = shape(to)
            );
        }
    }
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let (text, _) = compile(
                &source,
                selection,
                &allocation,
                &keel_codegen::x86_64::REGISTERS,
            );
            for opcode in [
                "punpcklbw",
                "punpcklwd",
                "punpckldq",
                "pcmpgtb",
                "pcmpgtw",
                "pcmpgtd",
            ] {
                assert!(text.contains(opcode), "missing {opcode}");
            }
            assert!(
                !text.contains("ptr ["),
                "register widening unexpectedly accessed memory: {text}"
            );
            assert!(
                !text.contains("pextr") && !text.contains("pmovsx") && !text.contains("pmovzx")
            );
        }
    }
}

use keel_codegen::regalloc::{Allocation, AllocationInput, Loc};
use std::{fs, path::PathBuf, process::Command};

const TINY: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3],
    float_caller_saved: &[0, 1],
    float_callee_saved: &[],
};
const SPILLED: Pools = Pools {
    float_caller_saved: &[],
    ..TINY
};
const LAYOUTS: [&str; 5] = [
    "alias",
    "distinct",
    "spillsource",
    "spillresult",
    "allspills",
];
const SHAPES: [&str; 6] = ["i8x16", "i16x8", "i32x4", "i64x2", "f32x4", "f64x2"];

// Independent of allocator heuristics, including disjoint 16-byte spill homes.
struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-widen-bitcast"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let locs = match input.func.name.rsplit('_').next().unwrap() {
            "alias" => [Loc::FReg(0), Loc::FReg(0)],
            "distinct" => [Loc::FReg(0), Loc::FReg(1)],
            "spillsource" => [Loc::Stack(0), Loc::FReg(0)],
            "spillresult" => [Loc::FReg(0), Loc::Stack(0)],
            "allspills" => [Loc::Stack(0), Loc::Stack(2)],
            _ => unreachable!(),
        };
        a.spill_slots = 4;
        for (i, loc) in locs.into_iter().enumerate() {
            a.locs[0][i] = Some(loc);
        }
        a
    }
}

// No FP operations enter the C oracle: its vector ABI arguments carry raw bits.
const C_HEADER: &str = r#"
typedef __UINT8_TYPE__ u8;
typedef __UINT16_TYPE__ u16;
typedef __UINT32_TYPE__ u32;
typedef __UINT64_TYPE__ u64;
typedef __SIZE_TYPE__ usize;
extern void *memcpy(void *, const void *, usize);
extern void *memset(void *, int, usize);
extern int memcmp(const void *, const void *, usize);
extern int printf(const char *, ...);
extern void *mmap(void *, usize, int, int, int, long);
extern int mprotect(void *, usize, int);
extern int munmap(void *, usize);
extern long sysconf(int);
typedef u8 V_i8x16 __attribute__((vector_size(16)));
typedef u16 V_i16x8 __attribute__((vector_size(16)));
typedef u32 V_i32x4 __attribute__((vector_size(16)));
typedef u64 V_i64x2 __attribute__((vector_size(16)));
typedef float V_f32x4 __attribute__((vector_size(16)));
typedef double V_f64x2 __attribute__((vector_size(16)));
struct Case { const char *name; void (*run)(const void *, void *); unsigned from, to, sign, bitcast, live; };
"#;

const C_ORACLE: &str = r#"
static int check(const struct Case *c, const u8 *input, u8 *dest) {
    u8 saved[16], expected[32]; memcpy(saved,input,16);
    if(c->bitcast) memcpy(expected,saved,16);
    else for(unsigned l=0;l<128/c->to;l++) {
        u64 v=0; memcpy(&v,saved+l*(c->from/8),c->from/8);
        if(c->sign && (v & (1ULL<<(c->from-1)))) v |= (~0ULL)<<c->from;
        memcpy(expected+l*(c->to/8),&v,c->to/8);
    }
    if(c->live) memcpy(expected+16,saved,16);
    c->run(input,dest);
    if(memcmp(dest,expected,c->live?32:16)) { printf("failed %s\n",c->name); return 1; }
    return 0;
}
int main(void) {
    const u64 edges[]={0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff,
        0x7fffffff,0x80000000,0xffffffff,0x7fffffffffffffffULL,
        0x8000000000000000ULL,~0ULL,0x7fc12345,0x7f812345,
        0x8000000000000000ULL,0x7ff8123456789abcULL,0x7ff0123456789abcULL};
    for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++) {
        // Every byte value appears in every source-byte position. Wide extrema,
        // NaN payloads and both FP signed zeros also receive separate patterns.
        for(unsigned pattern=0;pattern<256+sizeof(edges)/sizeof(edges[0]);pattern++) {
            u8 saved[16];
            if(pattern<256) for(unsigned b=0;b<16;b++) saved[b]=(u8)(pattern+31*b);
            else for(unsigned l=0;l<128/cases[c].from;l++) {
                u64 v=edges[(pattern-256+7*l)%(sizeof(edges)/sizeof(edges[0]))];
                memcpy(saved+l*(cases[c].from/8),&v,cases[c].from/8);
            }
            for(unsigned offset=0;offset<16;offset++) for(unsigned alias=0;alias<2;alias++) {
                u8 in[80],out[80]; memset(in,0x5a,sizeof(in)); memset(out,0x5a,sizeof(out));
                memcpy(in+16+offset,saved,16);
                u8 *dest=(alias?in:out)+16+offset;
                if(check(&cases[c],in+16+offset,dest)) return 1;
                unsigned size=cases[c].live?32:16;
                if(dest[-1]!=0x5a || dest[size]!=0x5a) return 2;
                if(!alias && memcmp(in+16+offset,saved,16)) return 3;
            }
        }
    }
    usize page=(usize)sysconf(30); // Linux _SC_PAGESIZE.
    u8 *a=mmap(0,3*page,3,0x22,-1,0), *b=mmap(0,3*page,3,0x22,-1,0);
    if(a==(void*)-1 || b==(void*)-1) return 4;
    if(mprotect(a,page,0)||mprotect(a+2*page,page,0)||mprotect(b,page,0)||mprotect(b+2*page,page,0)) return 5;
    for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++) for(unsigned offset=0;offset<16;offset++) {
        // Full vector IR loads read exactly 16 bytes; no widening-load fusion.
        // Test both ends of the accessible page, unaligned up to 15 bytes.
        for(unsigned side=0;side<2;side++) {
            u8 *in=side?a+2*page-16-offset:a+page+offset;
            unsigned size=cases[c].live?32:16;
            u8 *out=side?b+2*page-size-offset:b+page+offset;
            for(unsigned i=0;i<16;i++) in[i]=(u8)(i*71+128);
            if(check(&cases[c],in,out)) return 6;
        }
    }
    munmap(a,3*page); munmap(b,3*page);
    printf("passed %u packed widening/bitcast cases, byte/extreme/alias/live/spill/guard oracles\n",(unsigned)(sizeof(cases)/sizeof(cases[0])));
    return 0;
}
"#;

fn fixture(controlled: bool) -> (String, String) {
    fixture_filtered(controlled, false)
}

fn fixture_filtered(controlled: bool, pilot: bool) -> (String, String) {
    let mut source = String::new();
    let mut c = C_HEADER.to_string();
    let mut cases = String::from("static const struct Case cases[]={\n");
    let mut conversions = Vec::new();
    for (from, to) in PAIRS {
        for (op, signed) in [("swiden_low", true), ("uwiden_low", false)] {
            conversions.push((shape(from), shape(to), op, from, to, signed, false));
        }
    }
    for from in SHAPES {
        for to in SHAPES {
            let width = if from.starts_with("i8") {
                8
            } else if from.starts_with("i16") {
                16
            } else if from.contains("32") {
                32
            } else {
                64
            };
            conversions.push((
                from.to_string(),
                to.to_string(),
                "bitcast",
                width,
                width,
                false,
                true,
            ));
        }
    }
    for (from, to, op, from_bits, to_bits, signed, bitcast) in conversions {
        if pilot && !(from == "i32x4" && to == "i64x2" && op == "swiden_low") {
            continue;
        }
        let layouts: &[&str] = if controlled { &LAYOUTS } else { &["live"] };
        for layout in layouts {
            let name = format!("{op}_{from}_{to}_{layout}");
            if controlled {
                source += &format!(
                    "func @{name}({from}) -> {to} {{\nblock0(v0: {from}):\n v1 = {op}.{to} v0\n return v1\n}}\n"
                );
                c += &format!(
                    "extern V_{to} {name}(V_{from});\nstatic void call_{name}(const void *p,void *q) {{ V_{from} x; V_{to} y; memcpy(&x,p,16); y={name}(x); memcpy(q,&y,16); }}\n"
                );
            } else {
                source += &format!(
                    "func @{name}(ptr,ptr) {{\nblock0(v0: ptr,v1: ptr):\n v2 = load.{from} align 1 v0\n v3 = {op}.{to} v2\n store align 1 v3,v1\n v4 = iconst.i64 16\n v5 = ptradd v1,v4\n store align 1 v2,v5\n return\n}}\n"
                );
                c += &format!("extern void {name}(const void*,void*);\n");
            }
            let call = if controlled {
                format!("call_{name}")
            } else {
                name.clone()
            };
            cases += &format!(
                "{{\"{name}\",{call},{from_bits},{to_bits},{},{},{}}},\n",
                u8::from(signed),
                u8::from(bitcast),
                u8::from(!controlled)
            );
        }
    }
    cases += "};\n";
    let mut c = c + &cases + C_ORACLE;
    if pilot {
        // Independent signed extension above is integer-only; add single-bit
        // and seeded random inputs without changing the older SSE2 corpus.
        c = c.replace("int main(void) {", "int main(void) { u32 seed=0x12345678U;");
        c = c.replace("pattern<256+sizeof(edges)/sizeof(edges[0])", "pattern<819");
        c = c.replace(
            "else for(unsigned l=0;l<128/cases[c].from;l++) {",
            r#"else if(pattern>=275) {
                for(unsigned l=0;l<4;l++) {
                    seed^=seed<<13; seed^=seed>>17; seed^=seed<<5;
                    u32 v=pattern<307?(1U<<((pattern-275+l)%32)):seed;
                    memcpy(saved+4*l,&v,4);
                }
            } else for(unsigned l=0;l<128/cases[c].from;l++) {"#,
        );
        c = c.replace(
            "munmap(a,3*page);",
            r#"
    // Both crossing input edges must fault before any result is published.
    u8 *shared=mmap(0,page,3,0x21,-1,0); if(shared==(void*)-1) return 7;
    for(unsigned k=0;k<sizeof(cases)/sizeof(cases[0]);k++) for(unsigned side=0;side<2;side++) {
        memset(shared,0xa5,32);
        const u8 *bad=side?a+2*page-15:a+page-1;
        int pid=fork(); if(pid<0)return 8;
        if(!pid){signal(11,fault);signal(7,fault);cases[k].run(bad,shared);_exit(1);}
        int status=0;if(waitpid(pid,&status,0)!=pid||status!=(77<<8))return 9;
        for(unsigned i=0;i<32;i++)if(shared[i]!=0xa5)return 10;
    }
    munmap(shared,page);
    munmap(a,3*page);"#,
        );
        c = c.replace(
            "c->run(input,dest);",
            r#"unsigned before,after;
    __asm__ volatile("stmxcsr %0":"=m"(before));
    c->run(input,dest);
    __asm__ volatile("stmxcsr %0":"=m"(after));
    if(before!=after)return 11;"#,
        );
        c = format!("{PILOT_C}\n{c}");
        if !controlled {
            // Keep both the input and widened result live across a real
            // external call which clobbers every XMM register, at strict v1.
            let called = source
                .replace("_live(", "_calls(")
                .replace(
                    " store align 1 v3,v1",
                    " call @widen_clobber()\n store align 1 v3,v1",
                )
                .replace(
                    " v3 = swiden_low",
                    " call @widen_clobber()\n v3 = swiden_low",
                );
            source = format!("decl @widen_clobber()\n{source}{called}");
            c = c.replace("static const struct Case cases[]={", "extern void swiden_low_i32x4_i64x2_calls(const void*,void*);\nstatic const struct Case cases[]={\n{\"calls\",swiden_low_i32x4_i64x2_calls,32,64,1,0,1},");
        }
    }
    (source, c)
}

fn command(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(output.status.success(), "{command:?}: {output:?}");
}

/// Type-checks a test's C oracle once. `oracle` then records each
/// configuration and, on x86-64 Linux, links and runs it.
fn check_oracle_c(stem: &str, c: &str) {
    let path = std::env::temp_dir().join(format!(
        "keel-widen-x86-{}-{stem}-oracle.c",
        std::process::id()
    ));
    fs::write(&path, c).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-ffreestanding",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(&path),
    );
    fs::remove_file(path).unwrap();
}

fn oracle(stem: &str, source: &str, c: &str, asm: &str, object: &[u8]) {
    let dir = std::env::var_os("KEEL_X86_WIDEN_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-widen-x86-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    for (extension, bytes) in [
        ("keel", source.as_bytes()),
        ("c", c.as_bytes()),
        ("s", asm.as_bytes()),
        ("o", object),
    ] {
        fs::write(dir.join(format!("{stem}.{extension}")), bytes).unwrap();
    }
    fs::write(
        dir.join("run_native.py"),
        include_str!("x86_widen/run_native.py"),
    )
    .unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        let exe = dir.join(stem);
        command(
            Command::new("clang")
                .args(["-O2", "-msse2", "-mno-avx"])
                .arg(dir.join(format!("{stem}.c")))
                .arg(dir.join(format!("{stem}.o")))
                .arg("-o")
                .arg(&exe),
        );
        command(&mut Command::new(exe));
    }
    if std::env::var_os("KEEL_X86_WIDEN_EVIDENCE").is_none() {
        // Tests may share this process, so remove only this oracle's files.
        for extension in ["keel", "c", "s", "o", ""] {
            let path = if extension.is_empty() {
                dir.join(stem)
            } else {
                dir.join(format!("{stem}.{extension}"))
            };
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
    }
}

#[test]
fn widening_and_all_vector_bitcasts_cover_controlled_aliases_and_spill_positions() {
    let (source, c) = fixture(true);
    check_oracle_c("controlled", &c);
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        let (asm, object) = compile(
            &source,
            selection,
            &Controlled,
            &keel_codegen::x86_64::REGISTERS,
        );
        assert!(!asm.contains("pextr") && !asm.contains("pmovsx") && !asm.contains("pmovzx"));
        oracle(
            &format!("controlled-{selection:?}"),
            &source,
            &c,
            &asm,
            &object,
        );
    }
}

#[test]
fn widening_and_bitcasts_preserve_live_sources_with_both_allocators_and_all_spills() {
    let (source, c) = fixture(false);
    check_oracle_c("live", &c);
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("tiny", &TINY),
                ("spilled", &SPILLED),
            ] {
                let (asm, object) = compile(&source, selection, &allocation, pools);
                if pool == "spilled" {
                    assert!(asm.contains("xmmword ptr [rbp"));
                }
                oracle(
                    &format!("live-{selection:?}-{allocation:?}-{pool}"),
                    &source,
                    &c,
                    &asm,
                    &object,
                );
            }
        }
    }
}

// Explicit vector loop: the existing full-width load is guarded by a full
// source vector's remaining lanes. It advances only the destination lane count,
// then uses scalar tails. This deliberately makes no load-fusion assumption.
fn stream_fixture() -> (String, String) {
    let mut source = String::new();
    let mut c = C_HEADER.to_string();
    c += "struct Stream {const char *name; void (*run)(const void*,void*,u64); unsigned from,to,sign;};\nstatic const u64 edges[]={0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff,0x7fffffff,0x80000000,0xffffffff,~0ULL};\n";
    let mut cases = String::from("static const struct Stream streams[]={\n");
    for (from, to) in PAIRS {
        for (op, scalar, sign) in [("swiden_low", "sext", 1), ("uwiden_low", "uext", 0)] {
            let name = format!("stream_{op}_{from}_{to}");
            source += &format!(
                "func @{name}(ptr,ptr,i64) {{
block0(v0: ptr,v1: ptr,v2: i64):
 v3 = iconst.i64 0
 jump block1(v3)
block1(v4: i64):
 v5 = iconst.i64 {source_lanes}
 v6 = isub v2,v4
 v7 = icmp.uge v6,v5
 brif v7,block2,block3
block2:
 v8 = iconst.i64 {from_bytes}
 v9 = imul v4,v8
 v10 = ptradd v0,v9
 v11 = load.{a} align 1 v10
 v12 = {op}.{b} v11
 v13 = iconst.i64 {to_bytes}
 v14 = imul v4,v13
 v15 = ptradd v1,v14
 store align 1 v12,v15
 v16 = iconst.i64 {dest_lanes}
 v17 = iadd v4,v16
 jump block1(v17)
block3:
 jump block4(v4)
block4(v18: i64):
 v19 = icmp.ult v18,v2
 brif v19,block5,block6
block5:
 v20 = iconst.i64 {from_bytes}
 v21 = imul v18,v20
 v22 = ptradd v0,v21
 v23 = load.i{from} align 1 v22
 v24 = {scalar}.i{to} v23
 v25 = iconst.i64 {to_bytes}
 v26 = imul v18,v25
 v27 = ptradd v1,v26
 store align 1 v24,v27
 v28 = iconst.i64 1
 v29 = iadd v18,v28
 jump block4(v29)
block6:
 return
}}\n",
                source_lanes = 128 / from,
                from_bytes = from / 8,
                to_bytes = to / 8,
                a = shape(from),
                b = shape(to),
                dest_lanes = 128 / to
            );
            c += &format!("extern void {name}(const void*,void*,u64);\n");
            cases += &format!("{{\"{name}\",{name},{from},{to},{sign}}},\n");
        }
    }
    c += &(cases + "};\n");
    c += r#"
int main(void) {
 usize page=(usize)sysconf(30);
 u8 *a=mmap(0,3*page,3,0x22,-1,0),*b=mmap(0,3*page,3,0x22,-1,0);
 if(a==(void*)-1||b==(void*)-1) return 1;
 if(mprotect(a,page,0)||mprotect(a+2*page,page,0)||mprotect(b,page,0)||mprotect(b+2*page,page,0)) return 2;
 for(unsigned c=0;c<sizeof(streams)/sizeof(streams[0]);c++) for(unsigned n=0;n<98;n++)
 for(unsigned offset=0;offset<16;offset++) for(unsigned pattern=0;pattern<16;pattern++) {
  const struct Stream *s=&streams[c]; unsigned sb=s->from/8,db=s->to/8;
  u8 *in=a+2*page-n*sb-offset,*out=b+2*page-n*db-offset;
  u8 expected[98*8],saved[98*4];
  memset(b+page,0x5a,page);
  for(unsigned l=0;l<n;l++) {
   u64 v=pattern<12?edges[(pattern+l*7)%12]:(u64)(l*0x9e3779b9U+pattern);
   memcpy(in+l*sb,&v,sb);
   v=0;memcpy(&v,in+l*sb,sb);
   if(s->sign&&(v&(1ULL<<(s->from-1)))) v|=(~0ULL)<<s->from;
   memcpy(expected+l*db,&v,db);
  }
  memcpy(saved,in,n*sb);
  // n=0,offset=0 passes pointers directly into inaccessible guard pages.
  s->run(in,out,n);
  if(memcmp(expected,out,n*db)||memcmp(saved,in,n*sb)) {printf("failed %s n=%u offset=%u pattern=%u\n",s->name,n,offset,pattern);return 3;}
  if(out[-1]!=0x5a || (offset&&out[n*db]!=0x5a)) return 4;
 }
 munmap(a,3*page);munmap(b,3*page);
 printf("passed 12 stream widening shapes/signs, zero/tails/guards/unaligned/wrapping-width patterns\n");
 return 0;
}
"#;
    (source, c)
}

#[test]
fn widening_streams_cover_zero_tails_and_guard_boundaries_without_load_fusion() {
    let (source, c) = stream_fixture();
    check_oracle_c("stream", &c);
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (pool, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("spilled", &SPILLED),
            ] {
                let (asm, object) = compile(&source, selection, &allocation, pools);
                oracle(
                    &format!("stream-{selection:?}-{allocation:?}-{pool}"),
                    &source,
                    &c,
                    &asm,
                    &object,
                );
            }
        }
    }
}

#[test]
fn widening_recipe_encodings_match_independent_clang_sse2_assembler() {
    use object::{Object, ObjectSection};
    let (source, _) = fixture(true);
    let (asm, _) = compile(
        &source,
        SelectionTier::Baseline,
        &Controlled,
        &keel_codegen::x86_64::REGISTERS,
    );
    let mut recipes = String::from(".intel_syntax noprefix\n.text\n");
    for line in asm.lines() {
        if matches!(
            line.split_whitespace().next(),
            Some(
                "movaps"
                    | "xorps"
                    | "pcmpgtb"
                    | "pcmpgtw"
                    | "pcmpgtd"
                    | "punpcklbw"
                    | "punpcklwd"
                    | "punpckldq"
            )
        ) {
            recipes += line;
            recipes += "\n";
        }
    }
    let dir = std::env::var_os("KEEL_X86_WIDEN_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-widen-recipes-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("recipes.s"), &recipes).unwrap();
    command(
        Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-msse2",
                "-mno-avx",
                "-c",
            ])
            .arg(dir.join("recipes.s"))
            .arg("-o")
            .arg(dir.join("recipes-clang.o")),
    );
    let ours = keel_asm::x86::assemble_object(&recipes).unwrap();
    let clang = fs::read(dir.join("recipes-clang.o")).unwrap();
    let bytes = |object: &[u8]| {
        object::File::parse(object)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    assert_eq!(bytes(&ours), bytes(&clang));
    if std::env::var_os("KEEL_X86_WIDEN_EVIDENCE").is_some() {
        fs::write(dir.join("recipes-keel.o"), ours).unwrap();
    } else {
        fs::remove_dir_all(dir).unwrap();
    }
}

fn profiles() -> Vec<(&'static str, Features)> {
    let v1 = Features::for_level(CpuLevel::V1);
    let v2 = Features::for_level(CpuLevel::V2);
    let v3 = Features::for_level(CpuLevel::V3);
    vec![
        ("v1", v1),
        ("v2", v2),
        ("v3", v3),
        ("v1+sse41", Features { sse41: true, ..v1 }),
        ("v2-sse41", Features { sse41: false, ..v2 }),
        ("v3-sse41", Features { sse41: false, ..v3 }),
    ]
}

#[test]
fn signed_dword_register_feature_alias_and_spill_windows() {
    let (source, c) = fixture_filtered(true, true);
    check_oracle_c("register-controlled", &c);
    for (profile, features) in profiles() {
        for selection in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            let (asm, object) = compile_features(
                &source,
                selection,
                &Controlled,
                &keel_codegen::x86_64::REGISTERS,
                features,
            );
            assert_eq!(
                asm.matches("pmovsxdq").count(),
                if features.sse41 { 5 } else { 0 }
            );
            if features.sse41 {
                assert!(
                    !asm.contains("pcmpgtd")
                        && !asm.contains("punpckldq")
                        && !asm.contains("xorps")
                );
                for (layout, instruction) in [
                    ("alias", "pmovsxdq\txmm0, xmm0"),
                    ("distinct", "pmovsxdq\txmm1, xmm0"),
                    ("spillsource", "pmovsxdq\txmm0, xmm13"),
                    ("spillresult", "pmovsxdq\txmm13, xmm0"),
                    ("allspills", "pmovsxdq\txmm13, xmm13"),
                ] {
                    let name = format!("\"swiden_low_i32x4_i64x2_{layout}\":");
                    let body = asm
                        .split(&name)
                        .nth(1)
                        .unwrap()
                        .split("\tret")
                        .next()
                        .unwrap();
                    let lines: Vec<_> = body
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .collect();
                    let at = lines
                        .iter()
                        .position(|l| *l == instruction)
                        .unwrap_or_else(|| panic!("{layout}: {body}"));
                    if matches!(layout, "spillsource" | "allspills") {
                        assert!(
                            lines[at - 1].starts_with("movups\txmm13, xmmword ptr [rbp"),
                            "{body}"
                        );
                    }
                    if matches!(layout, "spillresult" | "allspills") {
                        assert!(
                            lines[at + 1].starts_with("movups\txmmword ptr [rbp")
                                && lines[at + 1].ends_with(", xmm13"),
                            "{body}"
                        );
                    }
                }
            } else {
                assert_eq!(asm.matches("pcmpgtd").count(), 5);
                assert_eq!(asm.matches("punpckldq").count(), 5);
            }
            oracle(
                &format!("register-{profile}-{selection:?}-controlled"),
                &source,
                &c,
                &asm,
                &object,
            );
        }
    }
}

#[test]
fn signed_dword_register_live_full_width_and_shape_controls() {
    let (source, c) = fixture_filtered(false, true);
    check_oracle_c("register-live", &c);
    for (profile, features) in profiles() {
        for selection in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for (pool, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("tiny", &TINY),
                    ("spilled", &SPILLED),
                ] {
                    let (asm, object) =
                        compile_features(&source, selection, &allocation, pools, features);
                    assert_eq!(
                        asm.matches("pmovsxdq").count(),
                        2 * usize::from(features.sse41)
                    );
                    // Live-source full-width load cannot enter the existing m64 cover.
                    assert!(
                        asm.lines()
                            .filter(|l| l.contains("pmovsxdq"))
                            .all(|l| !l.contains("ptr ["))
                    );
                    assert!(asm.contains("movups") && asm.contains("xmmword ptr ["));
                    oracle(
                        &format!("register-{profile}-{selection:?}-{allocation:?}-{pool}"),
                        &source,
                        &c,
                        &asm,
                        &object,
                    );
                }
            }
            for negative in [
                "func @unsigned(i32x4) -> i64x2 {\nentry(v0:i32x4):\n v1 = uwiden_low.i64x2 v0\n return v1\n}",
                "func @high(i32x4) -> i64x2 {\nentry(v0:i32x4):\n v1 = swiden_high.i64x2 v0\n return v1\n}",
                "func @other(i16x8) -> i64x2 {\nentry(v0:i16x8):\n v1 = swiden_low.i64x2 v0\n return v1\n}",
                "func @narrow(i32x2) -> i64x2 {\nentry(v0:i32x2):\n v1 = swiden_low.i64x2 v0\n return v1\n}",
                "func @fp(f32x4) -> i32x4 {\nentry(v0:f32x4):\n v1 = bitcast.i32x4 v0\n return v1\n}",
            ] {
                let (asm, _) = compile_features(
                    negative,
                    selection,
                    &AllocationTier::Full,
                    &keel_codegen::x86_64::REGISTERS,
                    features,
                );
                assert!(!asm.contains("pmovsxdq"), "{asm}");
            }
            let alias = "func @resolved_alias(i32x4,i32x4) -> i64x2 {\nentry(v0:i32x4,v1:i32x4):\n v2 = swiden_low.i64x2 v1\n return v2\n}";
            let (asm, _) = compile_features(
                alias,
                selection,
                &AllocationTier::Full,
                &keel_codegen::x86_64::REGISTERS,
                features,
            );
            assert_eq!(asm.matches("pmovsxdq").count(), usize::from(features.sse41));
        }
    }
}

const PILOT_C: &str = r#"
extern int fork(void);
extern int waitpid(int,int*,int);
extern void _exit(int);
extern void (*signal(int,void (*)(int)))(int);
static void fault(int sig){(void)sig;_exit(77);}
__attribute__((noinline)) void widen_clobber(void) {
 __asm__ volatile(
 "pxor %%xmm0,%%xmm0; pxor %%xmm1,%%xmm1; pxor %%xmm2,%%xmm2; pxor %%xmm3,%%xmm3;"
 "pxor %%xmm4,%%xmm4; pxor %%xmm5,%%xmm5; pxor %%xmm6,%%xmm6; pxor %%xmm7,%%xmm7;"
 "pxor %%xmm8,%%xmm8; pxor %%xmm9,%%xmm9; pxor %%xmm10,%%xmm10; pxor %%xmm11,%%xmm11;"
 "pxor %%xmm12,%%xmm12; pxor %%xmm13,%%xmm13; pxor %%xmm14,%%xmm14; pxor %%xmm15,%%xmm15"
 ::: "xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7",
 "xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15");
}
"#;

#[test]
fn signed_dword_register_encodings_match_clang_and_effective_feature_guards() {
    use object::{Object, ObjectSection};
    let mut asm = String::from(".intel_syntax noprefix\n.text\n");
    for d in 0..16 {
        for a in 0..16 {
            asm += &format!("pmovsxdq xmm{d},xmm{a}\n");
        }
    }
    let mut encoded = None;
    for (_, features) in profiles() {
        let result = keel_asm::x86::assemble_object_with_features(&asm, features);
        assert_eq!(result.is_ok(), features.sse41);
        if let Ok(object) = result {
            encoded = Some(object);
        }
    }
    let dir = std::env::var_os("KEEL_X86_WIDEN_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-pmovsxdq-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("pmovsxdq.s"), asm).unwrap();
    command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(dir.join("pmovsxdq.s"))
            .arg("-o")
            .arg(dir.join("pmovsxdq-clang.o")),
    );
    let bytes = |o: &[u8]| {
        object::File::parse(o)
            .unwrap()
            .section_by_name(".text")
            .unwrap()
            .data()
            .unwrap()
            .to_vec()
    };
    let ours = encoded.unwrap();
    assert_eq!(
        bytes(&ours),
        bytes(&fs::read(dir.join("pmovsxdq-clang.o")).unwrap())
    );
    if std::env::var_os("KEEL_X86_WIDEN_EVIDENCE").is_some() {
        fs::write(dir.join("pmovsxdq-keel.o"), ours).unwrap();
    } else {
        fs::remove_dir_all(dir).unwrap();
    }
}
