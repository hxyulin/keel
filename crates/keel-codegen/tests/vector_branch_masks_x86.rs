//! SSE2 unsigned ordering and mask proofs; the exported oracle runs on Linux x86.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator},
};
use std::{fs, path::PathBuf, process::Command};

#[path = "common/x86_packed.rs"]
mod x86_packed;

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
const LAYOUTS: [&str; 8] = [
    "lhs",
    "rhs",
    "distinct",
    "same",
    "stacka",
    "stackb",
    "stackresult",
    "stackall",
];

// Force each destructive two-address overlap independently of allocator choices.
struct Controlled;
impl RegisterAllocator for Controlled {
    fn name(&self) -> &'static str {
        "controlled-packed-compare"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let locations = match input.func.name.rsplit('_').next().unwrap() {
            "lhs" | "same" => [Loc::FReg(0), Loc::FReg(1), Loc::FReg(0)],
            "rhs" => [Loc::FReg(0), Loc::FReg(1), Loc::FReg(1)],
            "distinct" => [Loc::FReg(0), Loc::FReg(1), Loc::FReg(2)],
            "stacka" => [Loc::Stack(0), Loc::FReg(1), Loc::FReg(1)],
            "stackb" => [Loc::FReg(0), Loc::Stack(0), Loc::FReg(0)],
            "stackresult" => [Loc::FReg(0), Loc::FReg(1), Loc::Stack(0)],
            "stackall" => [Loc::Stack(0), Loc::Stack(1), Loc::Stack(2)],
            _ => unreachable!(),
        };
        a.spill_slots = 3;
        for (i, loc) in locations.into_iter().enumerate() {
            a.locs[0][i] = Some(loc);
        }
        a
    }
}

fn compile(
    source: &str,
    selection: SelectionTier,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
    optimize: bool,
) -> (String, Vec<u8>) {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if optimize {
        keel_opt::run_passes_for_target_with_options(
            &mut m,
            keel_opt::DEFAULT_PIPELINE,
            keel_opt::VectorTarget::X86Sse2,
            keel_opt::OptimizeOptions {
                verification: keel_opt::VerificationPolicy::Enabled,
            },
        )
        .unwrap();
    }
    let pipeline = CodegenPipeline {
        selector: &selection,
        allocator,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    assert_eq!(p.stats().text_bytes, 0);
    let features = keel_asm_ir::x86::Features::default();
    let typed = keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap();
    let parsed = keel_asm::x86::assemble_object_with_features(&text, features).unwrap();
    assert_eq!(typed, parsed, "typed/text ELF mismatch");
    (text, typed)
}

fn command(c: &mut Command) {
    let output = c.output().unwrap();
    assert!(output.status.success(), "{c:?}: {output:?}");
}
fn evidence() -> Option<PathBuf> {
    std::env::var_os("KEEL_BRANCH_MASK_EVIDENCE").map(PathBuf::from)
}
fn export(stem: &str, source: &str, c: &str, asm: &str, object: &[u8]) {
    if let Some(dir) = evidence() {
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{stem}.keel")), source).unwrap();
        fs::write(dir.join(format!("{stem}.c")), c).unwrap();
        fs::write(dir.join(format!("{stem}.s")), asm).unwrap();
        fs::write(dir.join(format!("{stem}.o")), object).unwrap();
    }
}
fn oracle(stem: &str, source: &str, c: &str, asm: &str, object: &[u8]) {
    export(stem, source, c, asm, object);
    let dir = std::env::temp_dir().join(format!("keel-branch-mask-{}-{stem}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("oracle.c");
    fs::write(&input, c).unwrap();
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
            .arg(&input),
    );
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let obj = dir.join("oracle.o");
        fs::write(&obj, object).unwrap();
        let exe = dir.join("oracle");
        command(
            Command::new("clang")
                .args(["-O2", "-msse2", "-mno-avx"])
                .arg(input)
                .arg(obj)
                .arg("-o")
                .arg(&exe),
        );
        command(&mut Command::new(exe));
    }
    fs::remove_dir_all(dir).unwrap();
}

fn controlled_fixture() -> (String, String) {
    let mut source = String::new();
    let mut c = String::from(
        "typedef __UINT64_TYPE__ u64; extern void *memcpy(void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void) {\n");
    for (shape, ty, lanes) in [
        ("i8x16", "__UINT8_TYPE__", 16),
        ("i16x8", "__UINT16_TYPE__", 8),
        ("i32x4", "__UINT32_TYPE__", 4),
        ("i64x2", "__UINT64_TYPE__", 2),
    ] {
        c += &format!(
            "typedef {ty} U_{shape}; typedef {ty} V_{shape} __attribute__((vector_size(16)));\n"
        );
        for (cc, symbol) in [("ult", "<"), ("ule", "<="), ("ugt", ">"), ("uge", ">=")] {
            for layout in LAYOUTS {
                let name = format!("controlled_{shape}_{cc}_{layout}");
                let rhs = if layout == "same" { "v0" } else { "v1" };
                source += &format!(
                    "func @{name}({shape}, {shape}) -> {shape} {{\nblock0(v0: {shape}, v1: {shape}):\n v2 = vicmp.{cc} v0, {rhs}\n return v2\n}}\n"
                );
                c += &format!("extern V_{shape} {name}(V_{shape},V_{shape});\n");
                let predicate = if layout == "same" {
                    if matches!(cc, "ule" | "uge") {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                } else {
                    format!("a[l] {symbol} b[l]")
                };
                main += &format!(
                    "{{ u64 edges[]={{0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff,0x7fffffff,0x80000000,0xffffffff,0x100000000,0x17fffffff,0x180000000,0x1ffffffff,0x7fffffff00000000,0x7fffffffffffffff,0x8000000000000000ULL,0x8000000000000001ULL,0xffffffff00000000ULL,0xffffffff7fffffffULL,0xffffffff80000000ULL,0xfffffffffffffffeULL,0xffffffffffffffffULL}};\nfor(unsigned i=0;i<24;i++)for(unsigned j=0;j<24;j++){{ U_{shape} a[{lanes}],b[{lanes}],got[{lanes}];for(unsigned l=0;l<{lanes};l++){{a[l]=(U_{shape})edges[(i+7*l)%24];b[l]=(U_{shape})edges[(j+11*l)%24];}}V_{shape} va,vb,vr;memcpy(&va,a,16);memcpy(&vb,b,16);vr={name}(va,vb);memcpy(got,&vr,16);for(unsigned l=0;l<{lanes};l++)if(got[l]!=(U_{shape})(({predicate}) ? (U_{shape})-1:0))return 1;}}}}\n"
                );
            }
        }
    }
    main += "return 0;}\n";
    (source, c + &main)
}

#[test]
fn unsigned_ordering_is_packed_at_every_width_and_overlap() {
    let (source, c) = controlled_fixture();
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
            false,
        );
        for opcode in ["pcmpgtb", "pcmpgtw", "pcmpgtd", "pslld", "pshufd"] {
            assert!(asm.contains(opcode), "missing {opcode}");
        }
        assert!(!asm.contains("pextr"));
        assert!(!asm.contains("\tcmp\t"), "unsigned compare scalarized");
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
fn integer_compare_oracle_preserves_live_inputs_with_both_allocators_and_all_spills() {
    let (source, c) = x86_packed::fixture();
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
                let (asm, object) = compile(
                    &source,
                    selection,
                    &allocation,
                    pools,
                    matches!(selection, SelectionTier::Full),
                );
                if pool == "spilled" {
                    assert!(asm.contains("xmmword ptr [rbp"));
                }
                oracle(
                    &format!("compare-{selection:?}-{allocation:?}-{pool}"),
                    &source,
                    &c,
                    &asm,
                    &object,
                );
            }
        }
    }
}

fn select_fixture() -> (String, String) {
    let mut source = String::new();
    let mut c = String::from(
        "typedef __UINT64_TYPE__ u64; extern void *memcpy(void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void) {\n");
    for (shape, mask, ty, signed, lanes) in [
        ("i8x16", "i8x16", "__UINT8_TYPE__", "__INT8_TYPE__", 16),
        ("i16x8", "i16x8", "__UINT16_TYPE__", "__INT16_TYPE__", 8),
        ("i32x4", "i32x4", "__UINT32_TYPE__", "__INT32_TYPE__", 4),
        ("i64x2", "i64x2", "__UINT64_TYPE__", "__INT64_TYPE__", 2),
        ("f32x4", "i32x4", "__UINT32_TYPE__", "__INT32_TYPE__", 4),
        ("f64x2", "i64x2", "__UINT64_TYPE__", "__INT64_TYPE__", 2),
    ] {
        for (cc, symbol, sign) in [
            ("eq", "==", false),
            ("ne", "!=", false),
            ("slt", "<", true),
            ("sle", "<=", true),
            ("sgt", ">", true),
            ("sge", ">=", true),
            ("ult", "<", false),
            ("ule", "<=", false),
            ("ugt", ">", false),
            ("uge", ">=", false),
            ("arbitrary", "!=", false),
        ] {
            let name = format!("select_{shape}_{cc}");
            let mask_inst = if cc == "arbitrary" {
                format!("v6 = load.{mask} align 1 v2")
            } else {
                format!("v6 = vicmp.{cc} v3, v4")
            };
            // Keep both data sources and the original mask live through two
            // selects, and make the destination overlap each memory source.
            source += &format!(
                "func @{name}(ptr, ptr, ptr, ptr) {{\nblock0(v0: ptr, v1: ptr, v2: ptr, v5: ptr):\n v3 = load.{mask} align 1 v0\n v4 = load.{mask} align 1 v1\n {mask_inst}\n v7 = bitcast.{shape} v3\n v8 = bitcast.{shape} v4\n v9 = vselect v6, v7, v8\n v10 = vselect v6, v8, v7\n store align 1 v9, v5\n v11 = iconst.i64 16\n v12 = ptradd v5, v11\n store align 1 v10, v12\n v13 = iconst.i64 32\n v14 = ptradd v5, v13\n store align 1 v6, v14\n v15 = iconst.i64 48\n v16 = ptradd v5, v15\n store align 1 v3, v16\n v17 = iconst.i64 64\n v18 = ptradd v5, v17\n store align 1 v4, v18\n return\n}}\n"
            );
            c += &format!("extern void {name}(const void*,const void*,const void*,void*);\n");
            let cast = if sign { signed } else { ty };
            let predicate = if cc == "arbitrary" {
                "m[l] != 0".to_string()
            } else {
                format!("({cast})a[l] {symbol} ({cast})b[l]")
            };
            let mask_expected = if cc == "arbitrary" {
                "m[l]"
            } else {
                "(truth ? (U)-1 : 0)"
            };
            main += &format!(
                "{{ typedef {ty} U; u64 edge[]={{0,1,2,0x80,0x8000,0x7fffffff,0x80000000,0xffffffff,0x100000000,0x17fffffff,0x180000000,0x1ffffffff,0x7fffffff00000000,0x7fffffffffffffff,0x8000000000000000ULL,0x8000000000000001ULL,0xffffffff00000000ULL,0xffffffff7fffffffULL,0xffffffff80000000ULL,0xffffffffffffffffULL,0x7fc12345,0x7f812345,0x7ff8123456789abcULL,0x7ff0123456789abcULL}};for(unsigned i=0;i<24;i++)for(unsigned j=0;j<24;j++){{U a[{lanes}],b[{lanes}],m[{lanes}],got[5*{lanes}]; for(unsigned l=0;l<{lanes};l++){{a[l]=(U)edge[(i+7*l)%24];b[l]=(U)edge[(j+11*l)%24];m[l]=(U)edge[(i+j+3*l)%24];}} for(unsigned alias=0;alias<4;alias++){{unsigned char aa[81],bb[81],mm[81],out[81];memcpy(aa+1,a,16);memcpy(bb+1,b,16);memcpy(mm+1,m,16);void *dest=alias==0?out+1:alias==1?aa+1:alias==2?bb+1:mm+1;{name}(aa+1,bb+1,mm+1,dest);memcpy(got,dest,80);for(unsigned l=0;l<{lanes};l++){{int truth=({predicate});if(got[l]!=(truth?a[l]:b[l])||got[{lanes}+l]!=(truth?b[l]:a[l])||got[2*{lanes}+l]!=(U){mask_expected}||got[3*{lanes}+l]!=a[l]||got[4*{lanes}+l]!=b[l])return 1;}}}}}}}}\n"
            );
        }
    }
    main += "return 0;}\n";
    (source, c + &main)
}

#[test]
fn compare_masks_are_reused_but_arbitrary_and_reinterpreted_masks_are_normalized() {
    for shape in ["i8x16", "i16x8", "i32x4", "i64x2"] {
        for mode in ["direct", "arbitrary", "block", "bitcast"] {
            let (params, body) = match mode {
                "direct" => (
                    format!("{shape}, {shape}"),
                    format!(
                        "block0(v0: {shape}, v1: {shape}):\n v2 = vicmp.ugt v0, v1\n v3 = vselect v2, v0, v1\n return v3"
                    ),
                ),
                "arbitrary" => (
                    format!("{shape}, {shape}, {shape}"),
                    format!(
                        "block0(v0: {shape}, v1: {shape}, v2: {shape}):\n v3 = vselect v2, v0, v1\n return v3"
                    ),
                ),
                "block" => (
                    format!("{shape}, {shape}"),
                    format!(
                        "block0(v0: {shape}, v1: {shape}):\n v2 = vicmp.ugt v0, v1\n jump block1(v2)\nblock1(v3: {shape}):\n v4 = vselect v3, v0, v1\n return v4"
                    ),
                ),
                "bitcast" => (
                    format!("i8x16, i8x16"),
                    format!(
                        "block0(v0: i8x16, v1: i8x16):\n v2 = vicmp.ugt v0, v1\n v3 = bitcast.{shape} v2\n v4 = bitcast.{shape} v0\n v5 = bitcast.{shape} v1\n v6 = vselect v3, v4, v5\n return v6"
                    ),
                ),
                _ => unreachable!(),
            };
            let source = format!("func @proof({params}) -> {shape} {{\n{body}\n}}\n");
            for allocation in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&keel_codegen::x86_64::REGISTERS, &SPILLED] {
                    let (asm, _) =
                        compile(&source, SelectionTier::Baseline, &allocation, pools, false);
                    let select = asm.split(" = vselect ").nth(1).unwrap();
                    let normalized = mode != "direct";
                    assert_eq!(
                        select.contains("andnps"),
                        normalized,
                        "{mode}/{shape}: {asm}"
                    );
                    assert_eq!(
                        select.contains("pcmpeq"),
                        normalized,
                        "{mode}/{shape}: {asm}"
                    );
                }
            }
        }
    }
}

#[test]
fn select_oracle_reuses_live_compare_masks_and_preserves_arbitrary_nonzero_lanes() {
    let (source, c) = select_fixture();
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
                let (asm, object) = compile(
                    &source,
                    selection,
                    &allocation,
                    pools,
                    matches!(selection, SelectionTier::Full),
                );
                oracle(
                    &format!("select-{selection:?}-{allocation:?}-{pool}"),
                    &source,
                    &c,
                    &asm,
                    &object,
                );
            }
        }
    }
}

struct SelectControlled;
impl RegisterAllocator for SelectControlled {
    fn name(&self) -> &'static str {
        "controlled-packed-select"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let mut a = AllocationTier::Full.allocate(input);
        let direct = input.func.name.contains("_direct_");
        let (mask, lhs, rhs) = if direct { (2, 0, 1) } else { (0, 1, 2) };
        let mut locs = [Loc::FReg(0), Loc::FReg(1), Loc::FReg(2), Loc::FReg(3)];
        match input.func.name.rsplit('_').next().unwrap() {
            "mask" => locs[3] = locs[mask],
            "lhs" => locs[3] = locs[lhs],
            "rhs" => locs[3] = locs[rhs],
            "distinct" => {}
            "spillmask" => locs[mask] = Loc::Stack(0),
            "spilla" => locs[lhs] = Loc::Stack(0),
            "spillb" => locs[rhs] = Loc::Stack(0),
            "spillresult" => locs[3] = Loc::Stack(0),
            "allspills" => locs = [Loc::Stack(0), Loc::Stack(1), Loc::Stack(2), Loc::Stack(3)],
            _ => unreachable!(),
        }
        a.spill_slots = 4;
        for (i, loc) in locs.into_iter().enumerate() {
            a.locs[0][i] = Some(loc);
        }
        a
    }
}

fn select_controlled_fixture() -> (String, String) {
    let mut source = String::new();
    let mut c = String::from(
        "typedef __UINT64_TYPE__ u64; extern void *memcpy(void*,const void*,__SIZE_TYPE__);\n",
    );
    let mut main = String::from("int main(void){\n");
    for (shape, ty, lanes) in [
        ("i8x16", "__UINT8_TYPE__", 16),
        ("i16x8", "__UINT16_TYPE__", 8),
        ("i32x4", "__UINT32_TYPE__", 4),
        ("i64x2", "__UINT64_TYPE__", 2),
    ] {
        c += &format!(
            "typedef {ty} U_{shape}; typedef {ty} V_{shape} __attribute__((vector_size(16)));\n"
        );
        for mode in ["direct", "arbitrary"] {
            for layout in [
                "mask",
                "lhs",
                "rhs",
                "distinct",
                "spillmask",
                "spilla",
                "spillb",
                "spillresult",
                "allspills",
            ] {
                let name = format!("select_controlled_{shape}_{mode}_{layout}");
                let (signature, body, call, predicate) = if mode == "direct" {
                    (
                        format!("{shape}, {shape}"),
                        format!(
                            "block0(v0: {shape}, v1: {shape}):\n v2 = vicmp.ugt v0, v1\n v3 = vselect v2, v0, v1"
                        ),
                        "va,vb",
                        "a[l]>b[l]",
                    )
                } else {
                    (
                        format!("{shape}, {shape}, {shape}"),
                        format!(
                            "block0(v0: {shape}, v1: {shape}, v2: {shape}):\n v3 = vselect v0, v1, v2"
                        ),
                        "vm,va,vb",
                        "m[l]!=0",
                    )
                };
                source +=
                    &format!("func @{name}({signature}) -> {shape} {{\n{body}\n return v3\n}}\n");
                c += &format!(
                    "extern V_{shape} {name}(V_{shape},V_{shape}{});\n",
                    if mode == "direct" {
                        "".to_string()
                    } else {
                        format!(",V_{shape}")
                    }
                );
                main += &format!(
                    "{{ typedef U_{shape} U; u64 edge[]={{0,1,0x7f,0x80,0x7fff,0x8000,0x7fffffff,0x80000000,0xffffffff,0x100000000,0x17fffffff,0x180000000,0x1ffffffff,0x7fffffff00000000,0x7fffffffffffffff,0x8000000000000000ULL,0x8000000000000001ULL,0xffffffff00000000ULL,0xffffffff7fffffffULL,0xffffffff80000000ULL,0xfffffffffffffffeULL,0xffffffffffffffffULL}};for(unsigned i=0;i<22;i++)for(unsigned j=0;j<22;j++){{ U a[{lanes}],b[{lanes}],m[{lanes}],got[{lanes}];for(unsigned l=0;l<{lanes};l++){{a[l]=(U)edge[(i+7*l)%22];b[l]=(U)edge[(j+11*l)%22];m[l]=(U)edge[(i+j+3*l)%22];}}V_{shape} va,vb,vm,vr;memcpy(&va,a,16);memcpy(&vb,b,16);memcpy(&vm,m,16);vr={name}({call});memcpy(got,&vr,16);for(unsigned l=0;l<{lanes};l++)if(got[l]!=(({predicate})?a[l]:b[l]))return 1;}}}}\n"
                );
            }
        }
    }
    main += "return 0;}\n";
    (source, c + &main)
}

#[test]
fn select_handles_every_destination_alias_and_spill_position() {
    let (source, c) = select_controlled_fixture();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        let (asm, object) = compile(
            &source,
            selection,
            &SelectControlled,
            &keel_codegen::x86_64::REGISTERS,
            false,
        );
        oracle(
            &format!("select-controlled-{selection:?}"),
            &source,
            &c,
            &asm,
            &object,
        );
    }
}

#[test]
fn compare_and_select_keep_debug_locations_and_volatile_access_order() {
    let source = "file f0 = \"packed.c\"
func @located(ptr, ptr, ptr, i64, i64) -> i64 {
block0(v0: ptr, v1: ptr, v2: ptr, v3: i64, v4: i64):
 v5 = load.i64x2 volatile align 1 v0 !f0:2:1
 v6 = load.i64x2 volatile align 1 v1 !f0:3:1
 v7 = vicmp.ugt v5, v6 !f0:4:1
 v8 = sdiv v3, v4 !f0:5:1
 v9 = vselect v7, v5, v6 !f0:6:1
 store volatile align 1 v9, v2 !f0:7:1
 return v8
}\n";
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        let (asm, _) = compile(
            source,
            SelectionTier::Full,
            &allocation,
            &keel_codegen::x86_64::REGISTERS,
            false,
        );
        let mut location = 0;
        let mut operations = Vec::new();
        for line in asm.lines().map(str::trim) {
            if line.starts_with(".loc") {
                location = line
                    .split_whitespace()
                    .nth(2)
                    .unwrap()
                    .parse::<u32>()
                    .unwrap();
            } else if line.starts_with("pcmpgtd")
                || line.starts_with("idiv")
                || line.starts_with("andps")
                || line.starts_with("movups")
            {
                operations.push((location, line.split_whitespace().next().unwrap()));
            }
        }
        assert_eq!(
            operations,
            [
                (2, "movups"),
                (3, "movups"),
                (4, "pcmpgtd"),
                (4, "andps"),
                (5, "idiv"),
                (6, "andps"),
                (7, "movups")
            ],
            "{asm}"
        );
    }
}
