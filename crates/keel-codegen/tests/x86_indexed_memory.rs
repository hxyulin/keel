//! Exact-width indexed memory covers, including allocation-visible leaves and
//! scratch reloads composed with stores and adjacent CMP-memory selection.
use keel_codegen::{
    CodegenPipeline, OptionsSelector, SelectionOptions,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
    select::{Selection, Target},
};
use keel_ir::EntityRef;

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE_REG: Pools = Pools {
    caller_saved: &[7],
    ..SPILLED
};

fn source(ty: &str, shift: i64) -> String {
    format!(
        "func @f(ptr, i64, {ty}) -> {ty}{} cc(c) {{
block0(v0: ptr, v1: i64, v2: {ty}):
 v3 = iconst.i64 {shift}
 v4 = shl v1, v3
 v5 = ptradd v0, v4
 store v2, v5
 v6 = shl v1, v3
 v7 = ptradd v0, v6
 v8 = load.{ty} v7
 return v8
}}",
        if matches!(ty, "i8" | "i16") {
            " uext"
        } else {
            ""
        }
    )
}

fn module(src: &str) -> keel_ir::Module {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

fn compile(
    m: &keel_ir::Module,
    options: SelectionOptions,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    compile_allocator(m, options, &alloc, pools)
}

fn compile_allocator(
    m: &keel_ir::Module,
    options: SelectionOptions,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let pipeline = CodegenPipeline {
        selector: &OptionsSelector(options),
        allocator,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline).unwrap();
    let program = keel_codegen::x86_64::compile_program_with_pipeline(m, pools, &pipeline).unwrap();
    let object = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object(&text).unwrap(),
        "{text}"
    );
    (text, object)
}

fn indexed_accesses(text: &str) -> usize {
    text.lines()
        .filter(|l| l.contains(" ptr [") && l.contains(" * "))
        .count()
}

#[test]
fn widths_scales_masked_shifts_options_allocators_and_spills() {
    for ty in ["i8", "i16", "i32", "i64", "f32", "f64", "ptr"] {
        for shift in 0..4 {
            for count in [shift, shift + 64, shift - 64] {
                let m = module(&source(ty, count));
                for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                    for pools in [&SPILLED, &ONE_REG, &keel_codegen::x86_64::REGISTERS] {
                        for bits in 0..8 {
                            let options = SelectionOptions {
                                trees: bits & 1 != 0,
                                folded_operands: bits & 2 != 0,
                                register_hints: bits & 4 != 0,
                            };
                            let (text, _) = compile(&m, options, alloc, pools);
                            assert_eq!(
                                indexed_accesses(&text),
                                if options.trees { 2 } else { 0 },
                                "{text}"
                            );
                            if options.trees {
                                assert!(!text.contains("\tshl\t"), "{text}");
                                assert!(
                                    !text
                                        .lines()
                                        .any(|l| l.contains("\tlea\t") && l.contains(" * ")),
                                    "{text}"
                                );
                                assert!(text.contains(&format!(" * {}]", 1 << shift)), "{text}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn selected_leaves_keep_duplicate_store_operands_and_aliases() {
    for repeated in ["v1", "v0"] {
        let src = source(if repeated == "v0" { "ptr" } else { "i64" }, 3)
            .replace("store v2, v5", &format!("store {repeated}, v5"));
        let mut m = module(&src);
        let f = &mut m.functions[0];
        let b = f.blocks().next().unwrap();
        let insts = f.block_insts(b).to_vec();
        let params = f.block_params(b);
        let expected = vec![
            params[if repeated == "v0" { 0 } else { 1 }],
            params[0],
            params[1],
        ];
        let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
        assert!(plan.covered(insts[1]) && plan.covered(insts[2]));
        assert_eq!(plan.operands(f, insts[3]), expected);
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            compile(&m, SelectionOptions::ALL, alloc, &SPILLED);
        }
    }
}

#[test]
fn shared_crossblock_debug_atomic_vector_and_narrow_boundaries() {
    let original = source("i64", 3);
    for src in [
        original
            .replace("v6 = shl v1, v3\n v7 = ptradd v0, v6\n ", "")
            .replace("load.i64 v7", "load.i64 v5"),
        original.replace("store v2, v5", "jump block1\nblock1:\n store v2, v5"),
        original
            .replace("store v2, v5", "atomic_store release v2, v5")
            .replace("load.i64 v7", "atomic_load.i64 acquire v7"),
        source("i128", 3),
        source("i32x4", 3),
    ] {
        let m = module(&src);
        let f = &m.functions[0];
        let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
        let address = f.block_insts(f.blocks().next().unwrap())[2];
        assert!(!plan.covered(address), "{src}");
        compile(&m, SelectionOptions::ALL, AllocationTier::Full, &SPILLED);
    }
    // Narrow wrapping shift followed by sext cannot become a 64-bit scale.
    let narrow = "func @n(ptr,i32) -> i64 {
block0(v0: ptr,v1: i32):
 v2 = iconst.i32 3
 v3 = shl v1,v2
 v4 = sext.i64 v3
 v5 = ptradd v0,v4
 v6 = load.i64 v5
 return v6
}";
    let m = module(narrow);
    let (text, _) = compile(&m, SelectionOptions::ALL, AllocationTier::Full, &SPILLED);
    assert!(
        text.contains("\tshl\t") && text.contains("\tmovsxd\t"),
        "{text}"
    );
    assert!(text.contains(" * 1]"), "{text}");
    // Illegal scales stay materialized, with only the ptradd folded.
    for shift in [4, 63, -1] {
        let (text, _) = compile(
            &module(&source("i64", shift)),
            SelectionOptions::ALL,
            AllocationTier::Full,
            &SPILLED,
        );
        assert_eq!(text.matches("\tshl\t").count(), 2, "{text}");
        assert_eq!(text.matches(" * 1]").count(), 2, "{text}");
    }
    // Identical line/file is the existing source-boundary policy. A located
    // address cannot disappear into an unlocated or different-line access.
    for (address_line, access_line, expected) in [
        (Some(2), None, false),
        (Some(2), Some(3), false),
        (Some(2), Some(2), true),
        (None, Some(3), true),
    ] {
        let mut m = module(&original);
        let f = &mut m.functions[0];
        let b = f.blocks().next().unwrap();
        let insts = f.block_insts(b).to_vec();
        for (inst, line) in [(insts[2], address_line), (insts[3], access_line)] {
            f.set_inst_loc(
                inst,
                line.map(|line| keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line,
                    col: 1,
                    scope: None,
                }),
            );
        }
        let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
        assert_eq!(plan.covered(insts[2]), expected);
        compile(&m, SelectionOptions::ALL, AllocationTier::Full, &SPILLED);
    }
}

fn compare_source(cc: &str, branch: bool) -> String {
    format!(
        "func @cmp(ptr,i64,i64) -> i64 {{
block0(v0: ptr,v1: i64,v2: i64):
 v3 = iconst.i64 3
 v4 = shl v1,v3
 v5 = ptradd v0,v4
 v6 = load.i64 v5
 v7 = icmp.{cc} v2,v6
 {}
}}",
        if branch {
            "brif v7,block1,block2\nblock1:\n v8 = iconst.i64 17\n return v8\nblock2:\n v9 = iconst.i64 91\n return v9"
        } else {
            "v8 = uext.i64 v7\n return v8"
        }
    )
}

#[test]
fn compare_memory_composes_with_indexed_leaves_and_branch_fusion() {
    for cc in [
        "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
    ] {
        for branch in [false, true] {
            let m = module(&compare_source(cc, branch));
            let f = &m.functions[0];
            let b = f.blocks().next().unwrap();
            let insts = f.block_insts(b);
            let params = f.block_params(b);
            let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
            assert_eq!(
                plan.operands(f, insts[4]),
                vec![params[2], params[0], params[1]]
            );
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&SPILLED, &ONE_REG, &keel_codegen::x86_64::REGISTERS] {
                    let (text, _) = compile(&m, SelectionOptions::ALL, alloc, pools);
                    assert_eq!(
                        text.lines()
                            .filter(|l| l.contains("\tcmp\t") && l.contains(" * 8]"))
                            .count(),
                        1,
                        "{text}"
                    );
                    if pools.caller_saved.is_empty() {
                        assert!(
                            text.contains("cmp\trax, qword ptr [rcx + rdx * 8]"),
                            "{text}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn shared_shifts_and_load_destination_overlapping_either_address_leaf() {
    let shared = source("i64", 3)
        .replace(" v6 = shl v1, v3\n", "")
        .replace("ptradd v0, v6", "ptradd v0, v4");
    let (text, _) = compile(
        &module(&shared),
        SelectionOptions::ALL,
        AllocationTier::Full,
        &SPILLED,
    );
    assert_eq!(text.matches("\tshl\t").count(), 1, "{text}");
    assert_eq!(text.matches(" * 1]").count(), 2, "{text}");
    for (params, base, index, expected) in [
        ("v0: ptr,v1: i64", "v0", "v1", "[rdi + rdx * 8]"),
        ("v0: i64,v1: ptr", "v1", "v0", "[rcx + rdi * 8]"),
    ] {
        let signature = if base == "v0" { "ptr,i64" } else { "i64,ptr" };
        let src = format!(
            "func @overlap({signature}) -> i64 {{\nblock0({params}):\n v2 = iconst.i64 3\n v3 = shl {index},v2\n v4 = ptradd {base},v3\n v5 = load.i64 v4\n return v5\n}}"
        );
        let (text, _) = compile(
            &module(&src),
            SelectionOptions::ALL,
            AllocationTier::Full,
            &ONE_REG,
        );
        assert!(
            text.contains(&format!("mov\trdi, qword ptr {expected}")),
            "{text}"
        );
    }
}

#[test]
fn compare_lhs_can_repeat_the_index_leaf() {
    let m = module(&compare_source("ult", true).replace("icmp.ult v2,v6", "icmp.ult v1,v6"));
    let f = &m.functions[0];
    let b = f.blocks().next().unwrap();
    let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
    let params = f.block_params(b);
    assert_eq!(
        plan.operands(f, f.block_insts(b)[4]),
        vec![params[1], params[0], params[1]]
    );
    compile(&m, SelectionOptions::ALL, AllocationTier::Full, &SPILLED);
}

#[test]
fn volatile_effect_order_and_optimizer_activation() {
    let src = source("i64", 3)
        .replace("store v2, v5", "store volatile v2, v5")
        .replace("load.i64 v7", "load.i64 volatile v7");
    let mut m = module(&src);
    for optimized in [false, true] {
        if optimized {
            keel_opt::optimize(&mut m).unwrap();
        }
        keel_ir::verify_module(&m).unwrap();
        let (text, _) = compile(&m, SelectionOptions::ALL, AllocationTier::Full, &SPILLED);
        // GVN shares the address across the two effects. Shared addresses
        // must then remain materialized, even for volatile accesses.
        assert_eq!(
            indexed_accesses(&text),
            if optimized { 0 } else { 2 },
            "{text}"
        );
        let store = text
            .lines()
            .position(|l| l.contains("# store volatile"))
            .unwrap();
        let load = text
            .lines()
            .position(|l| l.contains("# v8 = load.i64 volatile"))
            .unwrap();
        assert!(store < load, "{text}");
    }
    let mut m = module(&src.replace(" store volatile v2, v5\n", ""));
    keel_opt::optimize(&mut m).unwrap();
    assert_eq!(
        indexed_accesses(&compile(&m, SelectionOptions::ALL, AllocationTier::Full, &SPILLED).0),
        1
    );
}

#[test]
fn resolved_aliases_and_stack_or_constant_rematerialization() {
    let mut m = module(&source("i64", 3));
    let f = &mut m.functions[0];
    let b = f.blocks().next().unwrap();
    let insts = f.block_insts(b).to_vec();
    let params = f.block_params(b).to_vec();
    // Replace the store's shift result with an existing parameter. The old
    // definition remains in layout; consumers must resolve its alias.
    f.replace_all_uses(f.first_result(insts[1]), params[1]);
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
    assert_eq!(
        plan.operands(f, insts[3]),
        vec![params[2], params[0], params[1]]
    );
    compile(&m, SelectionOptions::ALL, AllocationTier::Full, &SPILLED);
    for base in ["v0", "v10"] {
        let src = source("i64", 3)
            .replace("block0(", " ss0 = stack_slot 64, align 8\nblock0(")
            .replace(" v3 =", " v10 = stack_addr ss0\n v3 =")
            .replace("ptradd v0,", &format!("ptradd {base},"));
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            compile(&module(&src), SelectionOptions::ALL, alloc, &SPILLED);
            #[cfg(feature = "experimental-regalloc")]
            {
                let allocator = keel_codegen::regalloc::RematerializingAllocator {
                    base: &alloc,
                    prefer_spilling: false,
                    constants: true,
                    stack_addresses: true,
                    null_pointers: false,
                };
                let (text, _) =
                    compile_allocator(&module(&src), SelectionOptions::ALL, &allocator, &SPILLED);
                assert_eq!(indexed_accesses(&text), 2, "{text}");
            }
        }
    }
    // Constant base is independently rematerialized while the index spills.
    let src = source("i64", 3)
        .replace(
            " v3 =",
            " v11 = iconst.i64 0\n v10 = inttoptr.ptr v11\n v3 =",
        )
        .replace("ptradd v0,", "ptradd v10,");
    compile(
        &module(&src),
        SelectionOptions::ALL,
        AllocationTier::Full,
        &SPILLED,
    );
    #[cfg(feature = "experimental-regalloc")]
    {
        let allocator = keel_codegen::regalloc::RematerializingAllocator {
            base: &AllocationTier::Full,
            prefer_spilling: false,
            constants: false,
            stack_addresses: false,
            null_pointers: true,
        };
        let (text, _) =
            compile_allocator(&module(&src), SelectionOptions::ALL, &allocator, &SPILLED);
        assert_eq!(text.matches("mov\tecx, 0").count(), 2, "{text}");
    }
}

/// This same independent C oracle can run locally on ARM and later on Linux
/// x86. ARM execution validates the IR/oracle subset only, not x86 machine code.
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
))]
#[test]
fn native_independent_c_oracles_exact_width_wrap_spills_and_fault_order() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("indexed-memory-oracle");
    fs::create_dir_all(&dir).unwrap();
    let mut ir = String::from("decl @clobber()\n");
    let mut oracle = String::from(
        "#include <stdint.h>\n#include <string.h>\nvolatile unsigned calls; void clobber(void){calls++;}\n",
    );
    let mut driver = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <assert.h>\n#include <unistd.h>\n#include <sys/mman.h>\n#include <sys/wait.h>\n#include <signal.h>\n#include <sys/resource.h>\n#ifdef __linux__\n#include <sys/prctl.h>\n#endif\n",
    );
    let mut checks = String::new();
    let mut faults = String::new();
    for (cc, op, signed) in [
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
    ] {
        for branch in [false, true] {
            let tag = format!("{cc}_{branch}");
            ir.push_str(&compare_source(cc, branch).replace("@cmp(", &format!("@cmp_{tag}(")));
            ir.push('\n');
            let cast = if signed { "int64_t" } else { "uint64_t" };
            let yes = if branch { 17 } else { 1 };
            let no = if branch { 91 } else { 0 };
            oracle.push_str(&format!("uint64_t oracle_cmp_{tag}(void*b,uint64_t i,uint64_t x){{uint64_t y;memcpy(&y,(void*)((uintptr_t)b+(i<<3)),8);return ({cast})x {op} ({cast})y ? {yes} : {no};}}\n"));
            driver.push_str(&format!("extern uint64_t cmp_{tag}(void*,uint64_t,uint64_t);extern uint64_t oracle_cmp_{tag}(void*,uint64_t,uint64_t);\n"));
            checks.push_str(&format!("{{uint64_t a[]={{0,1,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX}};for(int i=-2;i<=2;i++)for(unsigned j=0;j<5;j++)assert(cmp_{tag}(a+2,(uint64_t)(int64_t)i,a[j])==oracle_cmp_{tag}(a+2,(uint64_t)(int64_t)i,a[j]));}}\n"));
        }
    }
    for (ty, cty, size) in [
        ("i8", "uint8_t", 1),
        ("i16", "uint16_t", 2),
        ("i32", "uint32_t", 4),
        ("i64", "uint64_t", 8),
        ("f32", "float", 4),
        ("f64", "double", 8),
        ("ptr", "uintptr_t", 8),
    ] {
        for shift in 0..4 {
            for masked in [0, 64, -64] {
                let tag = format!("{ty}_{shift}_{}", masked + 64);
                // The call separates the pure address from its effect. Both
                // address leaves must remain live across the clobbering call.
                let get = source(ty, shift + masked)
                    .replace("@f(", &format!("@get_{tag}("))
                    .replace(" store v2, v5\n", "")
                    .replace(
                        &format!(" v8 = load.{ty}"),
                        &format!(" call @clobber()\n v8 = load.{ty}"),
                    );
                let set = source(ty, shift + masked)
                    .replace("@f(", &format!("@set_{tag}("))
                    .replace(" store v2, v5", " call @clobber()\n store v2, v5")
                    .replace(&format!(" v8 = load.{ty} v7\n return v8"), " return v2");
                ir.push_str(&get);
                ir.push('\n');
                ir.push_str(&set);
                ir.push('\n');
                driver.push_str(&format!("extern {cty} get_{tag}(void*,uint64_t,{cty}); extern {cty} set_{tag}(void*,uint64_t,{cty}); extern {cty} oracle_{tag}(void*,uint64_t,{cty},int);\n"));
                oracle.push_str(&format!("{cty} oracle_{tag}(void*b,uint64_t i,{cty} v,int store){{ uintptr_t p=(uintptr_t)b+(i<<{shift}); if(store) memcpy((void*)p,&v,sizeof(v)); else memcpy(&v,(void*)p,sizeof(v)); return v; }}\n"));
                let bits = match ty {
                    "f32" => "UINT64_C(0x80000000)",
                    "f64" => "UINT64_C(0x7ff8000000001234)",
                    _ => "UINT64_C(0xfedcba9876543210)",
                };
                checks.push_str(&format!("{{ unsigned char a[1024],b[1024]; for(int j=-32;j<=32;j++){{memset(a,0x55,sizeof(a));memset(b,0x55,sizeof(b));uint64_t raw={bits}^(uint64_t)(j&15); {cty} v;memcpy(&v,&raw,sizeof(v));uint64_t i=(uint64_t)(int64_t)j; {cty} got=set_{tag}(a+512,i,v), want=oracle_{tag}(b+512,i,v,1);assert(!memcmp(&got,&want,sizeof(got)));assert(!memcmp(a,b,sizeof(a)));got=get_{tag}(a+512,i,v);want=oracle_{tag}(b+512,i,v,0);assert(!memcmp(&got,&want,sizeof(got))); }} }}\n"));
                // Boundary accesses prove the exact byte width. A following
                // protected-page crossing must fault for both load and store.
                if masked == 0 {
                    faults.push_str(&format!("{{ {cty} v;memset(&v,0x35,sizeof(v));uint64_t i=1;void*p=m+n-{size}-(1<<{shift});set_{tag}(p,i,v);{cty} got=get_{tag}(p,i,v);assert(!memcmp(&got,&v,sizeof(v)));for(int store=0;store<2;store++){{pid_t c=fork();assert(c>=0);if(!c){{if(store)set_{tag}(m+n-{size}+1-(1<<{shift}),i,v);else get_{tag}(m+n-{size}+1-(1<<{shift}),i,v);_exit(0);}}int s;assert(waitpid(c,&s,0)==c);assert(WIFSIGNALED(s));assert(WTERMSIG(s)==SIGSEGV||WTERMSIG(s)==SIGBUS);}} }}\n"));
                }
            }
        }
    }
    // Executable modular overflow: offset (2^61 + 1)*8 wraps to 8.
    ir.push_str("func @wrap(ptr,i64) -> i64 {\nblock0(v0: ptr,v1: i64):\n v2 = iconst.i64 3\n v3 = shl v1,v2\n v4 = ptradd v0,v3\n v5 = load.i64 v4\n return v5\n}\n");
    // Store index as data; store base as data; destination reuses base/index
    // under hints/one-register pressure. Also test a stack-address remat base.
    ir.push_str(
        &source("i64", 3)
            .replace("@f(", "@same_index(")
            .replace("store v2, v5", "store v1, v5"),
    );
    ir.push('\n');
    ir.push_str(
        &source("ptr", 3)
            .replace("@f(", "@same_base(")
            .replace("store v2, v5", "store v0, v5"),
    );
    ir.push('\n');
    ir.push_str("func @stack(i64,i64) -> i64 {\n ss0 = stack_slot 64, align 8\nblock0(v0: i64,v1: i64):\n v2 = stack_addr ss0\n v3 = iconst.i64 3\n v4 = shl v0,v3\n v5 = ptradd v2,v4\n store v1,v5\n v6 = shl v0,v3\n v7 = ptradd v2,v6\n v8 = load.i64 v7\n return v8\n}\n");
    // A fault must occur after the earlier volatile marker store and before
    // the later marker store; shared mapping makes ordering parent-visible.
    ir.push_str("func @ordered(ptr,i64,ptr) -> i64 {\nblock0(v0: ptr,v1: i64,v2: ptr):\n v3 = iconst.i64 3\n v4 = shl v1,v3\n v5 = ptradd v0,v4\n v6 = iconst.i64 17\n store volatile v6,v2\n v7 = load.i64 volatile v5\n v8 = iconst.i64 91\n store volatile v8,v2\n return v7\n}\n");
    // Linux core_pattern pipes can invoke crash reporters despite RLIMIT_CORE=0.
    // Disable dumps for this oracle and its deliberate-fault children only.
    driver.push_str("extern uint64_t wrap(void*,uint64_t);extern uint64_t same_index(void*,uint64_t,uint64_t);extern void* same_base(void*,uint64_t,void*);extern uint64_t stack(uint64_t,uint64_t);extern uint64_t ordered(void*,uint64_t,void*);\nint main(void){struct rlimit lim={0,0};assert(!setrlimit(RLIMIT_CORE,&lim));\n#ifdef __linux__\nassert(!prctl(PR_SET_DUMPABLE,0));\n#endif\n");
    driver.push_str(&checks);
    driver.push_str("{uint64_t a[65]={0}; a[1]=UINT64_C(0xfedcba9876543210);assert(wrap(a,(UINT64_C(1)<<61)+1)==a[1]);assert(wrap((void*)(UINTPTR_MAX-7),((uintptr_t)&a[1]+8)>>3)==a[1]);for(int i=-32;i<=32;i++){assert(same_index(a+32,(uint64_t)(int64_t)i,0)==(uint64_t)(int64_t)i);assert(a[i+32]==(uint64_t)(int64_t)i);}void* b[65];for(int i=-32;i<=32;i++){assert(same_base(b+32,(uint64_t)(int64_t)i,0)==b+32);assert(b[i+32]==b+32);}for(unsigned i=0;i<8;i++)assert(stack(i,UINT64_C(0xfedcba9876543210)^i)==(UINT64_C(0xfedcba9876543210)^i));}\nlong n=sysconf(_SC_PAGESIZE);assert(n>0);unsigned char*m=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_SHARED|MAP_ANONYMOUS,-1,0);assert(m!=MAP_FAILED);assert(!mprotect(m+n,n,PROT_NONE));\n");
    driver.push_str(&faults);
    driver.push_str("{uint64_t*mark=(uint64_t*)m;*mark=0;pid_t c=fork();assert(c>=0);if(!c){ordered(m+n,0,mark);_exit(0);}int s;assert(waitpid(c,&s,0)==c);assert(WIFSIGNALED(s));assert(WTERMSIG(s)==SIGSEGV||WTERMSIG(s)==SIGBUS);assert(*mark==17);}assert(!munmap(m,2*n));return 0;}\n");
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    fs::write(dir.join("driver.c"), driver).unwrap();
    let run = |command: &mut Command| {
        let out = command.output().unwrap();
        assert!(
            out.status.success(),
            "{command:?}\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(Command::new("clang")
        .args(["-O0", "-fno-lto", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o")));
    let mut m = module(&ir);
    for optimized in [false, true] {
        if optimized {
            keel_opt::optimize(&mut m).unwrap();
        }
        for (case, (options, alloc, spilled)) in [
            (SelectionOptions::default(), AllocationTier::Full, false),
            (SelectionOptions::ALL, AllocationTier::Fast, false),
            (SelectionOptions::ALL, AllocationTier::Full, false),
            (SelectionOptions::ALL, AllocationTier::Fast, true),
            (SelectionOptions::ALL, AllocationTier::Full, true),
            (SelectionOptions::ALL, AllocationTier::Full, false),
        ]
        .into_iter()
        .enumerate()
        {
            #[cfg(feature = "experimental-regalloc")]
            let remat = keel_codegen::regalloc::RematerializingAllocator {
                base: &alloc,
                prefer_spilling: false,
                constants: true,
                stack_addresses: true,
                null_pointers: false,
            };
            #[cfg(feature = "experimental-regalloc")]
            let allocator: &dyn RegisterAllocator = if spilled { &remat } else { &alloc };
            #[cfg(not(feature = "experimental-regalloc"))]
            let allocator: &dyn RegisterAllocator = &alloc;
            #[cfg(target_arch = "x86_64")]
            let (text, object) = compile_allocator(
                &m,
                options,
                allocator,
                if spilled {
                    &SPILLED
                } else if case == 5 {
                    &ONE_REG
                } else {
                    &keel_codegen::x86_64::REGISTERS
                },
            );
            #[cfg(target_arch = "aarch64")]
            let (text, object) = {
                let pipeline = CodegenPipeline {
                    selector: &OptionsSelector(options),
                    allocator,
                };
                let arm_one = Pools {
                    caller_saved: &[9],
                    ..SPILLED
                };
                let pools = if spilled {
                    &SPILLED
                } else if case == 5 {
                    &arm_one
                } else {
                    &keel_codegen::aarch64::REGISTERS
                };
                let text =
                    keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap();
                assert_eq!(object, keel_asm::assemble_object(&text).unwrap());
                (text, object)
            };
            let stem = format!("test-{optimized}-{case}");
            fs::write(dir.join(format!("{stem}.s")), &text).unwrap();
            fs::write(dir.join(format!("{stem}.o")), &object).unwrap();
            // Independent assembler encoding check as well as text/direct parity.
            run(Command::new("clang")
                .arg("-c")
                .arg(dir.join(format!("{stem}.s")))
                .arg("-o")
                .arg(dir.join("clang.o")));
            use object::{Object, ObjectSection};
            let section = if cfg!(target_arch = "x86_64") {
                ".text"
            } else {
                "__text"
            };
            let bytes = |b: &[u8]| {
                object::File::parse(b)
                    .unwrap()
                    .section_by_name(section)
                    .unwrap()
                    .data()
                    .unwrap()
                    .to_vec()
            };
            assert_eq!(
                bytes(&object),
                bytes(&fs::read(dir.join("clang.o")).unwrap())
            );
            let binary = dir.join(&stem);
            run(Command::new("clang")
                .args(["-O0", "-fno-lto"])
                .arg(dir.join("driver.c"))
                .arg(dir.join("oracle.o"))
                .arg(dir.join(format!("{stem}.o")))
                .arg("-o")
                .arg(&binary));
            run(&mut Command::new(binary));
        }
    }
}

#[test]
fn independent_llvm_x86_assembler_matches_indexed_objects() {
    use object::{Object, ObjectSection};
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("x86-indexed-objects");
    fs::create_dir_all(&dir).unwrap();
    let mut src = String::new();
    for ty in ["i8", "i16", "i32", "i64", "f32", "f64", "ptr"] {
        for shift in 0..4 {
            src.push_str(&source(ty, shift).replace("@f(", &format!("@f_{ty}_{shift}(")));
            src.push('\n');
        }
    }
    for cc in [
        "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
    ] {
        for branch in [false, true] {
            src.push_str(
                &compare_source(cc, branch).replace("@cmp(", &format!("@cmp_{cc}_{branch}(")),
            );
            src.push('\n');
        }
    }
    let m = module(&src);
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        for (i, pools) in [&SPILLED, &ONE_REG, &keel_codegen::x86_64::REGISTERS]
            .into_iter()
            .enumerate()
        {
            let (text, object) = compile(&m, SelectionOptions::ALL, alloc, pools);
            let stem = format!("{alloc:?}-{i}");
            let assembly = dir.join(format!("{stem}.s"));
            fs::write(&assembly, text).unwrap();
            fs::write(dir.join(format!("{stem}.o")), &object).unwrap();
            let clang_object = dir.join(format!("{stem}-clang.o"));
            let out = Command::new("clang")
                .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
                .arg(&assembly)
                .arg("-o")
                .arg(&clang_object)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let bytes = |b: &[u8]| {
                object::File::parse(b)
                    .unwrap()
                    .section_by_name(".text")
                    .unwrap()
                    .data()
                    .unwrap()
                    .to_vec()
            };
            assert_eq!(bytes(&object), bytes(&fs::read(clang_object).unwrap()));
        }
    }
}
