//! Bounded adjacent integer memory RHS covers. Native C is an independent
//! semantic oracle; ARM runs validate the fallback/oracle, not x86 execution.
use keel_codegen::{
    CodegenPipeline, OptionsSelector, SelectionOptions,
    regalloc::{AllocationTier, Pools, RegisterAllocator},
    select::{Selection, Target},
};
use object::{Object, ObjectSection};

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[7],
    ..SPILLED
};

fn module(src: &str) -> keel_ir::Module {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn source(ty: &str, op: &str, address: &str) -> String {
    format!(
        "func @f(ptr, i64, {ty}) -> i64 cc(c) {{
block0(v0: ptr, v1: i64, v2: {ty}):
 {address}
 v5 = load.{ty} v4
 v6 = {op} v2, v5
 v7 = uext.i64 v6
 return v7
}}"
    )
    .replace(
        "v7 = uext.i64 v6",
        if ty == "i64" {
            "v7 = iadd v6, v1"
        } else {
            "v7 = uext.i64 v6"
        },
    )
}
const INDEX: &str = "v3 = iconst.i64 3\n v8 = shl v1, v3\n v4 = ptradd v0, v8";
const OFFSET: &str = "v3 = iconst.i64 8\n v4 = ptradd v0, v3";
fn compile(
    m: &keel_ir::Module,
    options: SelectionOptions,
    alloc: &dyn RegisterAllocator,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let pipeline = CodegenPipeline {
        selector: &OptionsSelector(options),
        allocator: alloc,
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
fn folded(m: &keel_ir::Module) -> bool {
    let f = &m.functions[0];
    let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
    f.layout_insts()
        .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Load { .. }) && plan.covered(i))
}
fn text_bytes(b: &[u8], section: &str) -> Vec<u8> {
    object::File::parse(b)
        .unwrap()
        .section_by_name(section)
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
#[test]
fn exact_width_alu_and_compare_selection_allocators_options_and_llvm() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-rhs-static");
    fs::create_dir_all(&dir).unwrap();
    let mut all = String::new();
    let mut n = 0;
    for ty in ["i8", "i16", "i32", "i64"] {
        for op in [
            "iadd", "isub", "and", "or", "xor", "imul", "icmp.slt", "icmp.ult",
        ] {
            let mut src = source(ty, op, INDEX);
            if op.starts_with("icmp") {
                src = src.replace("v7 = iadd v6, v1", "v7 = uext.i64 v6");
            }
            let m = module(&src);
            assert_eq!(folded(&m), !(ty == "i8" && op == "imul"), "{src}");
            for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                for pools in [&SPILLED, &ONE, &keel_codegen::x86_64::REGISTERS] {
                    let (text, _) = compile(&m, SelectionOptions::ALL, &alloc, pools);
                    if !(ty == "i8" && op == "imul") {
                        let mnemonic = match op {
                            "iadd" => "add",
                            "isub" => "sub",
                            "icmp.slt" | "icmp.ult" => "cmp",
                            _ => op,
                        };
                        let size = match ty {
                            "i8" => "byte",
                            "i16" => "word",
                            "i32" => "dword",
                            _ => "qword",
                        };
                        assert!(
                            text.lines().any(|l| l
                                .trim_start()
                                .starts_with(&format!("{mnemonic}\t"))
                                && l.contains(&format!("{size} ptr"))
                                && l.contains(" * ")),
                            "{text}"
                        );
                    }
                }
            }
            all.push_str(&src.replace("@f(", &format!("@f{n}(")));
            n += 1;
        }
    }
    let m = module(&all);
    // Encoding parity with an independent assembler, on either host platform.
    let (text, obj) = compile(&m, SelectionOptions::ALL, &AllocationTier::Full, &SPILLED);
    fs::write(dir.join("test.keel"), &all).unwrap();
    fs::write(dir.join("keel.o"), &obj).unwrap();
    fs::write(dir.join("test.s"), text).unwrap();
    let out = Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(dir.join("test.s"))
        .arg("-o")
        .arg(dir.join("clang.o"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        text_bytes(&obj, ".text"),
        text_bytes(&fs::read(dir.join("clang.o")).unwrap(), ".text")
    );
    for options in [
        SelectionOptions::default(),
        SelectionOptions {
            trees: false,
            ..SelectionOptions::ALL
        },
        SelectionOptions {
            folded_operands: false,
            ..SelectionOptions::ALL
        },
    ] {
        compile(&m, options, &AllocationTier::Full, &SPILLED);
    }
}

#[test]
fn adjacency_shared_aliases_debug_and_effect_boundaries() {
    let src = source("i32", "isub", OFFSET);
    for negative in [
        src.replace("load.i32 v4", "load.i32 volatile v4"),
        src.replace("load.i32 v4", "atomic_load.i32 relaxed v4"),
        src.replace("v6 = isub", "store v2, v0\n v6 = isub"),
        src.replace("v6 = isub", "v9 = iconst.i32 3\n v6 = isub"),
        src.replace("v6 = isub", "fence seq_cst\n v6 = isub"),
        format!(
            "decl @clobber()\n{}",
            src.replace("v6 = isub", "call @clobber()\n v6 = isub")
        ),
        src.replace("v7 = uext.i64 v6", "store v5, v0\n v7 = uext.i64 v6"),
        src.replace("isub v2, v5", "isub v5, v2"),
        src.replace("isub v2, v5", "isub v5, v5"),
        format!(
            "file f0 = \"rhs.c\"\n{}",
            src.replace("load.i32 v4", "load.i32 v4 !f0:1:1")
        ),
    ] {
        let m = module(&negative);
        assert!(!folded(&m), "{negative}");
        compile(&m, SelectionOptions::ALL, &AllocationTier::Full, &SPILLED);
    }
    // The isort-shaped shared comparison input must keep its original read.
    let m = module(
        &src.replace("isub v2, v5", "icmp.slt v2, v5")
            .replace("v7 = uext", "store v5, v0\n v7 = uext"),
    );
    assert!(!folded(&m));
    let mut m = module(&src);
    let f = &mut m.functions[0];
    let b = f.blocks().next().unwrap();
    let insts = f.block_insts(b).to_vec();
    let loaded = f.inst_results(insts[2])[0];
    f.replace_all_uses(loaded, f.block_params(b)[2]);
    keel_ir::verify_module(&m).unwrap();
    assert!(!folded(&m), "aliased dead load cannot own a read");

    // Resolve raw RHS aliases before counting uses, but never attach a new
    // read to an aliased consumer result that emission will discard.
    let mut m = module(
        &src.replace("v5 = load", "v10 = iconst.i32 9\n v5 = load")
            .replace("isub v2, v5", "isub v2, v10"),
    );
    let f = &mut m.functions[0];
    let b = f.blocks().next().unwrap();
    let insts = f.block_insts(b).to_vec();
    f.replace_all_uses(f.inst_results(insts[2])[0], f.inst_results(insts[3])[0]);
    keel_ir::verify_module(&m).unwrap();
    assert!(folded(&m));
    compile(&m, SelectionOptions::ALL, &AllocationTier::Full, &SPILLED);
    let f = &mut m.functions[0];
    f.replace_all_uses(f.inst_results(insts[4])[0], f.block_params(b)[2]);
    keel_ir::verify_module(&m).unwrap();
    assert!(!folded(&m));
}

#[test]
fn publishes_lhs_and_duplicate_index_leaves_without_destructive_overlap() {
    let m = module(&source("i64", "isub", INDEX).replace("isub v2, v5", "isub v1, v5"));
    let f = &m.functions[0];
    let b = f.blocks().next().unwrap();
    let params = f.block_params(b);
    let insts = f.block_insts(b);
    let plan = Selection::compute_with_options(f, Target::X86, SelectionOptions::ALL);
    assert_eq!(
        plan.operands(f, insts[4]),
        vec![params[1], params[0], params[1]]
    );
    assert!(plan.covered(insts[1]) && plan.covered(insts[2]) && plan.covered(insts[3]));
    let (text, _) = compile(&m, SelectionOptions::ALL, &AllocationTier::Full, &ONE);
    assert!(
        text.lines().any(|l| l.contains("sub\trax, qword ptr")),
        "{text}"
    );
    for options in [
        SelectionOptions::default(),
        SelectionOptions {
            trees: false,
            ..SelectionOptions::ALL
        },
    ] {
        let plan = Selection::compute_with_options(f, Target::X86, options);
        assert!(!plan.covered(insts[3]));
    }
}

// The lhs is the first argument and dies at the ALU, so normal and one-GPR
// Fast/Full allocation can give the result the same register. In the index
// case the lhs is also the actual selected address leaf; the base case derives
// the pointer base from the lhs's integer bits through inttoptr.
fn coalesced_source(kind: &str) -> String {
    match kind {
        "index" => "func @coalesce_index(i64,ptr) -> i64 cc(c) {
block0(v0: i64,v1: ptr):
 v2 = iconst.i64 3
 v3 = shl v0,v2
 v4 = ptradd v1,v3
 v5 = load.i64 v4
 v6 = isub v0,v5
 return v6
}"
        .into(),
        "base" => "func @coalesce_base(i64,i64) -> i64 cc(c) {
block0(v0: i64,v1: i64):
 v2 = inttoptr.ptr v0
 v3 = iconst.i64 3
 v4 = shl v1,v3
 v5 = ptradd v2,v4
 v6 = load.i64 v5
 v7 = isub v0,v6
 return v7
}"
        .into(),
        ty @ ("i8" | "i16") => format!(
            "func @coalesce_{ty}({ty},ptr,i64) -> i64 cc(c) {{
block0(v0: {ty},v1: ptr,v2: i64):
 v3 = iconst.i64 3
 v4 = shl v2,v3
 v5 = ptradd v1,v4
 v6 = load.{ty} v5
 v7 = isub v0,v6
 v8 = uext.i64 v7
 return v8
}}"
        ),
        _ => unreachable!(),
    }
}

#[test]
fn coalesced_lhs_avoids_rax_roundtrip_and_normalizes_its_own_register() {
    use keel_codegen::regalloc::{Allocation, AllocationInput, Loc, OperandKind, ValuePart};
    struct Coalesced<'a>(&'a dyn RegisterAllocator);
    impl RegisterAllocator for Coalesced<'_> {
        fn name(&self) -> &'static str {
            "assert-coalesced"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let allocation = self.0.allocate(input);
            for (_, inst) in input.func.layout_insts() {
                if let keel_ir::InstData::Binary {
                    op: keel_ir::BinaryOp::Isub,
                    args,
                } = *input.func.inst_data(inst)
                {
                    let index = allocation.edits.instruction_index(inst);
                    let lhs = allocation.operand_loc(
                        index,
                        ValuePart::low(input.func.resolve(args[0])),
                        OperandKind::Use,
                    );
                    let result = allocation.operand_loc(
                        index,
                        ValuePart::low(input.func.first_result(inst)),
                        OperandKind::Def,
                    );
                    assert!(matches!(lhs, Some(Loc::Reg(_))), "{lhs:?}");
                    assert_eq!(result, lhs, "fixture must exercise d == a");
                }
            }
            allocation
        }
    }
    let mut all = String::new();
    for kind in ["index", "base", "i8", "i16"] {
        all.push_str(&coalesced_source(kind));
        all.push('\n');
        let m = module(&coalesced_source(kind));
        assert!(folded(&m));
        for tier in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&ONE, &keel_codegen::x86_64::REGISTERS] {
                let (text, _) = compile(&m, SelectionOptions::ALL, &Coalesced(&tier), pools);
                let body = text
                    .split(" = isub ")
                    .nth(1)
                    .unwrap()
                    .split("\n\t#")
                    .next()
                    .unwrap();
                assert!(!body.contains("rax") && !body.contains("eax"), "{text}");
                let alu = body
                    .lines()
                    .find(|l| l.trim_start().starts_with("sub\t"))
                    .unwrap()
                    .trim();
                assert!(alu.contains(" ptr ["), "{text}");
                let accumulator = alu.split_once('\t').unwrap().1.split(',').next().unwrap();
                if matches!(kind, "i8" | "i16") {
                    assert!(
                        body.lines().any(|l| l.trim_start().starts_with("movzx\t")
                            && l.ends_with(&format!(", {accumulator}"))),
                        "{text}"
                    );
                }
            }
        }
    }
    // Independent encoding parity for the allocated accumulators, including
    // partial-register normalization and one-register address reloads.
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-rhs-coalesced");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("test.keel"), &all).unwrap();
    let m = module(&all);
    for (case, pools) in [&ONE, &keel_codegen::x86_64::REGISTERS]
        .into_iter()
        .enumerate()
    {
        let (text, obj) = compile(
            &m,
            SelectionOptions::ALL,
            &Coalesced(&AllocationTier::Full),
            pools,
        );
        let asm = dir.join(format!("test-{case}.s"));
        let clang = dir.join(format!("clang-{case}.o"));
        fs::write(&asm, text).unwrap();
        fs::write(dir.join(format!("keel-{case}.o")), &obj).unwrap();
        let out = Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(&asm)
            .arg("-o")
            .arg(&clang)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            text_bytes(&obj, ".text"),
            text_bytes(&fs::read(clang).unwrap(), ".text")
        );
    }
}

#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
))]
#[test]
fn native_independent_c_oracle_arithmetic_normalization_compares_and_fault_order() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("memory-rhs-oracle");
    fs::create_dir_all(&dir).unwrap();
    let mut ir = String::from("decl @clobber()\n");
    let mut oracle = String::from(
        "#include <stdint.h>\n#include <string.h>\nvolatile unsigned calls;void clobber(void){calls++;}\n",
    );
    let mut driver = String::from(
        "#include <stdint.h>\n#include <string.h>\n#include <assert.h>\n#include <unistd.h>\n#include <sys/mman.h>\n#include <sys/wait.h>\n#include <signal.h>\n#include <sys/resource.h>\n#ifdef __linux__\n#include <sys/prctl.h>\n#endif\n",
    );
    let mut checks = String::new();
    let mut faults = String::new();
    for (ty, cty, sty, size) in [
        ("i8", "uint8_t", "int8_t", 1),
        ("i16", "uint16_t", "int16_t", 2),
        ("i32", "uint32_t", "int32_t", 4),
        ("i64", "uint64_t", "int64_t", 8),
    ] {
        for (op, expr) in [
            ("iadd", "(uint64_t)x+(uint64_t)y"),
            ("isub", "(uint64_t)x-(uint64_t)y"),
            ("and", "x&y"),
            ("or", "x|y"),
            ("xor", "x^y"),
            ("imul", "(uint64_t)x*(uint64_t)y"),
            ("icmp.slt", "(S)x<(S)y"),
            ("icmp.ult", "x<y"),
            ("icmp.eq", "x==y"),
        ] {
            let tag = format!("{ty}_{}", op.replace('.', "_"));
            let compare = op.starts_with("icmp");
            let mut src = source(ty, op, INDEX).replace("@f(", &format!("@f_{tag}("));
            if compare {
                src = src.replace("v7 = iadd v6, v1", "v7 = uext.i64 v6");
            }
            // Keep all inputs alive across a call before the adjacent pair.
            src = src.replace(
                &format!("v5 = load.{ty}"),
                &format!("call @clobber()\n v5 = load.{ty}"),
            );
            ir.push_str(&src);
            ir.push('\n');
            let expression = expr.replace("S", sty);
            let expression = if compare {
                format!("(uint64_t)({expression})")
            } else {
                format!("(uint64_t)({cty})({expression})")
            };
            let expression = if ty == "i64" && !compare {
                format!("({expression})+i")
            } else {
                expression
            };
            oracle.push_str(&format!("uint64_t o_{tag}(void*p,uint64_t i,{cty} x){{{cty} y;memcpy(&y,(void*)((uintptr_t)p+(i<<3)),sizeof(y));return {expression};}}\n"));
            driver.push_str(&format!("extern uint64_t f_{tag}(void*,uint64_t,{cty});extern uint64_t o_{tag}(void*,uint64_t,{cty});\n"));
            checks.push_str(&format!("for(unsigned a=0;a<12;a++)for(unsigned b=0;b<12;b++)for(int j=-1;j<=1;j++){{uint64_t raw=v[a];memcpy(buf+32+j*8,&raw,{size});uint64_t i=(uint64_t)(int64_t)j;assert(f_{tag}(buf+32,i,({cty})v[b])==o_{tag}(buf+32,i,({cty})v[b]));}}\n"));
            faults.push_str(&format!("{{void*p=m+n-{size};memset(p,0xa5,{size});assert(f_{tag}(p,0,({cty})-1)==o_{tag}(p,0,({cty})-1));pid_t c=fork();assert(c>=0);if(!c){{f_{tag}(m+n-{size}+1,0,3);_exit(0);}}int s;assert(waitpid(c,&s,0)==c);assert(WIFSIGNALED(s));assert(WTERMSIG(s)==SIGSEGV||WTERMSIG(s)==SIGBUS);}}\n"));
        }
    }
    for kind in ["index", "base", "i8", "i16"] {
        ir.push_str(&coalesced_source(kind));
        ir.push('\n');
    }
    // Lhs duplicates an address index. Its allocated register may also be d.
    ir.push_str(
        &source("i64", "isub", INDEX)
            .replace("@f(", "@same_index(")
            .replace("isub v2, v5", "isub v1, v5"),
    );
    ir.push_str(
        &source("i64", "isub", INDEX)
            .replace("@f(", "@same_base(")
            .replace("v5 = load.i64", "v9 = ptrtoint.i64 v0\n v5 = load.i64")
            .replace("isub v2, v5", "isub v9, v5"),
    );
    // Shared address stays materialized, while its single-use loaded data can
    // still fold. Returning the updated memory also checks the store data.
    ir.push_str(
        &source("i64", "isub", INDEX)
            .replace("@f(", "@shared_address(")
            .replace("v7 = iadd", "store v6, v4\n v7 = iadd"),
    );
    // Exact-width signed extension of a modular narrow result.
    ir.push_str(
        &source("i16", "isub", OFFSET)
            .replace("@f(", "@signed16(")
            .replace("uext.i64", "sext.i64"),
    );
    // Both outcomes and branch/compare fusion at a narrow width.
    ir.push_str("func @branch(ptr,i32) -> i64 cc(c) {\nblock0(v0: ptr,v1: i32):\n v2 = load.i32 v0\n v3 = icmp.slt v1,v2\n brif v3,block1,block2\nblock1:\n v4 = iconst.i64 17\n return v4\nblock2:\n v5 = iconst.i64 91\n return v5\n}\n");
    // A fault must occur between the two externally observable marker stores.
    ir.push_str("func @ordered(ptr,ptr,i32) -> i32 cc(c) {\nblock0(v0: ptr,v1: ptr,v2: i32):\n v3 = iconst.i32 17\n store volatile v3,v1\n v4 = load.i32 v0\n v5 = isub v2,v4\n v6 = iconst.i32 91\n store volatile v6,v1\n return v5\n}\n");
    // Rematerialized stack address and literal lhs, with an allocated result
    // potentially overlapping the address. The address is used at the ALU.
    ir.push_str("func @stack(i64) -> i64 cc(c) {\n ss0 = stack_slot 8, align 8\nblock0(v0: i64):\n v1 = stack_addr ss0\n store v0,v1\n v2 = iconst.i64 123\n v3 = load.i64 v1\n v4 = isub v2,v3\n return v4\n}\n");
    driver.push_str("extern uint64_t coalesce_index(uint64_t,void*);extern uint64_t coalesce_base(uint64_t,uint64_t);extern uint64_t coalesce_i8(uint8_t,void*,uint64_t);extern uint64_t coalesce_i16(uint16_t,void*,uint64_t);extern uint64_t same_base(void*,uint64_t,uint64_t);extern uint64_t shared_address(void*,uint64_t,uint64_t);extern uint64_t same_index(void*,uint64_t,uint64_t);extern int64_t signed16(void*,uint64_t,uint16_t);extern uint64_t branch(void*,uint32_t);extern uint32_t ordered(void*,void*,uint32_t);extern uint64_t stack(uint64_t);\nint main(void){struct rlimit lim={0,0};assert(!setrlimit(RLIMIT_CORE,&lim));\n#ifdef __linux__\nassert(!prctl(PR_SET_DUMPABLE,0));\n#endif\nuint64_t v[]={0,1,0x7f,0x80,0x7fff,0x8000,UINT64_C(0x7fffffff),UINT64_C(0x80000000),UINT64_C(0xffffffff),UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX};unsigned char buf[80];\n");
    driver.push_str(&checks);
    driver.push_str("for(unsigned a=0;a<12;a++){assert(stack(v[a])==123-v[a]);for(unsigned b=0;b<12;b++){uint16_t y=(uint16_t)v[a];memcpy(buf+8,&y,2);assert(signed16(buf,0,(uint16_t)v[b])==(int16_t)((uint16_t)v[b]-y));uint32_t z=(uint32_t)v[a];memcpy(buf,&z,4);assert(branch(buf,(uint32_t)v[b])==((int32_t)v[b]<(int32_t)z?17:91));}for(int j=-1;j<=1;j++){uint64_t i=(uint64_t)(int64_t)j;memcpy(buf+32+j*8,&v[a],8);assert(coalesce_index(i,buf+32)==i-v[a]);assert(coalesce_base((uintptr_t)(buf+32),i)==(uintptr_t)(buf+32)-v[a]);assert(coalesce_i8((uint8_t)v[(a+1)%12],buf+32,i)==(uint8_t)((uint8_t)v[(a+1)%12]-(uint8_t)v[a]));assert(coalesce_i16((uint16_t)v[(a+1)%12],buf+32,i)==(uint16_t)((uint16_t)v[(a+1)%12]-(uint16_t)v[a]));assert(same_index(buf+32,i,0)==i-v[a]+i);assert(same_base(buf+32,i,0)==(uintptr_t)(buf+32)-v[a]+i);uint64_t x=v[a]^UINT64_C(0xfedcba9876543210);assert(shared_address(buf+32,i,x)==x-v[a]+i);uint64_t stored;memcpy(&stored,buf+32+j*8,8);assert(stored==x-v[a]);}}long n=sysconf(_SC_PAGESIZE);assert(n>0);unsigned char*m=mmap(0,2*n,PROT_READ|PROT_WRITE,MAP_SHARED|MAP_ANONYMOUS,-1,0);assert(m!=MAP_FAILED);assert(!mprotect(m+n,n,PROT_NONE));\n");
    driver.push_str(&faults);
    driver.push_str("{uint32_t*mark=(uint32_t*)m;*mark=0;pid_t c=fork();assert(c>=0);if(!c){ordered(m+n,mark,0);_exit(0);}int s;assert(waitpid(c,&s,0)==c);assert(WIFSIGNALED(s));assert(WTERMSIG(s)==SIGSEGV||WTERMSIG(s)==SIGBUS);assert(*mark==17);}assert(!munmap(m,2*n));return 0;}\n");
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    fs::write(dir.join("driver.c"), driver).unwrap();
    fs::write(dir.join("test.keel"), &ir).unwrap();
    let run = |cmd: &mut Command| {
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{cmd:?}\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(Command::new("clang")
        .args(["-O0", "-fno-lto", "-c"])
        .arg(dir.join("oracle.c"))
        .arg("-o")
        .arg(dir.join("oracle.o")));
    let m = module(&ir);
    for (case, (options, tier, pressure)) in [
        (SelectionOptions::default(), AllocationTier::Full, 0),
        (SelectionOptions::ALL, AllocationTier::Fast, 0),
        (SelectionOptions::ALL, AllocationTier::Full, 0),
        (SelectionOptions::ALL, AllocationTier::Fast, 1),
        (SelectionOptions::ALL, AllocationTier::Full, 1),
        (SelectionOptions::ALL, AllocationTier::Full, 2),
        (
            SelectionOptions {
                folded_operands: false,
                ..SelectionOptions::ALL
            },
            AllocationTier::Full,
            1,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        #[cfg(feature = "experimental-regalloc")]
        let remat = keel_codegen::regalloc::RematerializingAllocator {
            base: &tier,
            prefer_spilling: false,
            constants: true,
            stack_addresses: true,
            null_pointers: false,
        };
        #[cfg(feature = "experimental-regalloc")]
        let allocator: &dyn RegisterAllocator = if pressure == 1 { &remat } else { &tier };
        #[cfg(not(feature = "experimental-regalloc"))]
        let allocator: &dyn RegisterAllocator = &tier;
        // Check the x86 selector/emitter even when the executable host is ARM.
        // In feature runs this also covers x86 rematerialization under spills.
        let (x86_text, x86_obj) = compile(
            &m,
            options,
            allocator,
            if pressure == 1 {
                &SPILLED
            } else if pressure == 2 {
                &ONE
            } else {
                &keel_codegen::x86_64::REGISTERS
            },
        );
        fs::write(dir.join(format!("x86-test-{case}.s")), &x86_text).unwrap();
        fs::write(dir.join(format!("x86-test-{case}.o")), &x86_obj).unwrap();
        #[cfg(target_arch = "x86_64")]
        let (text, obj) = (x86_text, x86_obj);
        #[cfg(target_arch = "aarch64")]
        let (text, obj) = {
            let one = Pools {
                caller_saved: &[9],
                ..SPILLED
            };
            let pools = match pressure {
                1 => &SPILLED,
                2 => &one,
                _ => &keel_codegen::aarch64::REGISTERS,
            };
            let pipeline = CodegenPipeline {
                selector: &OptionsSelector(options),
                allocator,
            };
            let text = keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
            let program =
                keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
            let obj = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
            assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
            (text, obj)
        };
        let stem = format!("test-{case}");
        fs::write(dir.join(format!("{stem}.s")), &text).unwrap();
        fs::write(dir.join(format!("{stem}.o")), &obj).unwrap();
        run(Command::new("clang")
            .arg("-c")
            .arg(dir.join(format!("{stem}.s")))
            .arg("-o")
            .arg(dir.join("clang.o")));
        let section = if cfg!(target_arch = "x86_64") {
            ".text"
        } else {
            "__text"
        };
        assert_eq!(
            text_bytes(&obj, section),
            text_bytes(&fs::read(dir.join("clang.o")).unwrap(), section)
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
