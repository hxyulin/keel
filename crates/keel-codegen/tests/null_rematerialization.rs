//! Cross-target null recipe object, flag, and parallel-copy correctness tests.
#![cfg(feature = "experimental-regalloc")]
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{
        Allocation, AllocationInput, AllocationTier, Loc, Pools, RegisterAllocator,
        RematerializingAllocator,
    },
};
use keel_ir::{EntityRef, Value};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn compile(
    src: &str,
    arm: bool,
    selector: SelectionTier,
    allocator: &dyn RegisterAllocator,
    pools: &Pools,
) -> String {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator,
    };
    let (text, program) = if arm {
        (
            keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline).unwrap(),
            keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline)
                .unwrap(),
        )
    } else {
        (
            keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap(),
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap(),
        )
    };
    let (typed, parsed) = if arm {
        (
            keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
                .unwrap(),
            keel_asm::assemble_object(&text).unwrap(),
        )
    } else {
        (
            keel_asm::x86::assemble_program_object(&program).unwrap(),
            keel_asm::x86::assemble_object(&text).unwrap(),
        )
    };
    assert_eq!(typed, parsed, "{text}");
    text
}

#[test]
fn null_reload_between_predicate_and_select_preserves_flags_on_both_targets() {
    let src = "func @f(i64, ptr) -> ptr {
block0(v0: i64, v1: ptr):
 v2 = iconst.i64 0
 v3 = inttoptr.ptr v2
 v4 = select v0, v3, v1
 return v4
}";
    for base in [AllocationTier::Fast, AllocationTier::Full] {
        let allocator = RematerializingAllocator {
            base: &base,
            prefer_spilling: false,
            constants: false,
            stack_addresses: false,
            null_pointers: true,
        };
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for arm in [false, true] {
                let text = compile(src, arm, selector, &allocator, &EMPTY);
                let lines: Vec<_> = text.lines().collect();
                let conditional = lines
                    .iter()
                    .position(|l| l.contains(if arm { "\tcsel\t" } else { "\tcmov" }))
                    .expect(&text);
                let compare = lines[..conditional]
                    .iter()
                    .rposition(|l| l.contains("\tcmp\t") || l.contains("\ttest\t"))
                    .expect(&text);
                let reloads = &lines[compare + 1..conditional];
                // The producer precedes the predicate. This zero must be the
                // reload at the use, with flags still consumed by CSEL/CMOV.
                assert!(
                    reloads.iter().any(|l| if arm {
                        (l.contains("\tmovz\t") || l.contains("\tmov\t")) && l.contains("#0")
                    } else {
                        l.contains("\tmov\t") && l.ends_with(", 0")
                    }),
                    "{text}"
                );
                assert!(
                    reloads.iter().all(|l| !l.contains("\txor\t")
                        && !l.contains("\tadds\t")
                        && !l.contains("\tsubs\t")
                        && !l.contains("\tands\t")),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn null_calls_returns_edges_and_mode_off_objects_match_text() {
    for src in [
        include_str!("programs/null_rematerialization.keel"),
        include_str!("programs/null_invoke.keel"),
    ] {
        for base in [AllocationTier::Fast, AllocationTier::Full] {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                for arm in [false, true] {
                    // Invoke is currently supported only by the AArch64 backend.
                    if !arm && src.contains("func @null_invoke") {
                        continue;
                    }
                    let pools = if arm {
                        &keel_codegen::aarch64::REGISTERS
                    } else {
                        &keel_codegen::x86_64::REGISTERS
                    };
                    for pools in [&EMPTY, pools] {
                        let mut allocator = RematerializingAllocator {
                            base: &base,
                            prefer_spilling: false,
                            constants: false,
                            stack_addresses: false,
                            null_pointers: false,
                        };
                        let control = compile(src, arm, selector, &base, pools);
                        assert_eq!(compile(src, arm, selector, &allocator, pools), control);
                        allocator.null_pointers = true;
                        compile(src, arm, selector, &allocator, pools);
                    }
                }
            }
        }
    }
}

// Force an edge register swap together with a null recipe to a stack parameter.
// This isolates the reserved cycle temporary from allocator register choices.
struct CycleAllocator([u8; 2]);
impl RegisterAllocator for CycleAllocator {
    fn name(&self) -> &'static str {
        "null-copy-cycle-test"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let empty = AllocationInput {
            pools: &EMPTY,
            ..*input
        };
        let mut allocation = RematerializingAllocator {
            base: &AllocationTier::Full,
            prefer_spilling: false,
            constants: false,
            stack_addresses: false,
            null_pointers: true,
        }
        .allocate(&empty);
        for (v, reg) in [(5, self.0[0]), (6, self.0[1])] {
            allocation.locs[0][Value::new(v).index()] = Some(Loc::Reg(reg));
        }
        allocation
    }
}
// Bounded interpreter for this fixture's straight-line edge copies only.
// Memory operands are distinct symbolic cells; no address arithmetic or control
// flow is interpreted. Unknown operations fail rather than being ignored.
fn check_forced_edge(text: &str, arm: bool) {
    use std::collections::HashMap;
    let (a, b, temp) = if arm {
        ("x9", "x10", "x16")
    } else {
        ("rsi", "rdi", "r11")
    };
    let lines: Vec<_> = text.lines().map(str::trim).collect();
    let save = lines
        .iter()
        .rposition(|l| *l == format!("mov\t{temp}, {a}"))
        .expect(text);
    let branch = |l: &str| {
        let op = l.split_whitespace().next().unwrap_or("");
        if arm {
            op == "b" || op.starts_with("b.") || op == "cbz" || op == "cbnz"
        } else {
            op.starts_with('j')
        }
    };
    let start = lines[..save].iter().rposition(|l| branch(l)).expect(text) + 1;
    let end = save + lines[save..].iter().position(|l| branch(l)).expect(text);
    assert!(end - start <= 24, "unexpected edge size: {text}");
    let canonical = |operand: &str| {
        operand
            .trim()
            .strip_prefix("qword ptr ")
            .unwrap_or(operand.trim())
            .replace("eax", "rax")
    };
    let mut ops = Vec::new();
    for line in &lines[start..end] {
        let (op, rest) = line.split_once('\t').expect(text);
        let (left, right) = rest.split_once(", ").expect(text);
        let (dst, src) = match op {
            "mov" | "movz" | "ldr" | "ldur" => (left, right),
            "str" | "stur" => (right, left),
            _ => panic!("unexpected copy operation {line}: {text}"),
        };
        ops.push((canonical(dst), canonical(src)));
    }
    let mut state = HashMap::from([
        (a.to_owned(), 0x1234u64),
        (b.to_owned(), 0x5678),
        (temp.to_owned(), 0xabcdef),
    ]);
    let execute = |state: &mut HashMap<String, u64>, dst: &str, src: &str| {
        let value = if src == "0" || src == "#0x0" || src == "#0" {
            0
        } else if src.starts_with('[') {
            let token = 0x100000 + state.len() as u64;
            *state.entry(src.to_owned()).or_insert(token)
        } else {
            *state
                .get(src)
                .unwrap_or_else(|| panic!("uninitialized {src}: {text}"))
        };
        state.insert(dst.to_owned(), value);
    };
    let zero = ops
        .iter()
        .position(|(_, src)| src == "0" || src == "#0x0" || src == "#0")
        .expect(text);
    assert!(
        ops[zero + 1].0.starts_with('['),
        "null must transfer to a stack cell: {text}"
    );
    let null_home = ops[zero + 1].0.clone();
    for (dst, src) in &ops {
        execute(&mut state, dst, src);
    }
    assert_eq!(state[a], 0x5678, "{text}");
    assert_eq!(state[b], 0x1234, "{text}");
    assert_eq!(state[&null_home], 0, "{text}");
    // The scheduler may place this immutable source before the cycle. Replay
    // the *emitted* zero/store pair with a live saved cycle token as well:
    // materializing a stack recipe must not overwrite that reserved temporary.
    for token in [0x9876543210u64, u64::MAX] {
        state.insert(temp.to_owned(), token);
        for (dst, src) in &ops[zero..=zero + 1] {
            execute(&mut state, dst, src);
        }
        assert_eq!(state[temp], token, "{text}");
        assert_eq!(state[&null_home], 0, "{text}");
    }
}

#[test]
fn stack_null_transfer_does_not_destroy_parallel_copy_cycle_temporary() {
    let whole = include_str!("programs/null_rematerialization.keel");
    let src =
        &whole[whole.find("func @null_cycle").unwrap()..whole.find("func @null_branch").unwrap()];
    for arm in [false, true] {
        let regs = if arm { [9, 10] } else { [6, 7] };
        let pools = Pools {
            caller_saved: &regs,
            ..EMPTY
        };
        let text = compile(
            src,
            arm,
            SelectionTier::Baseline,
            &CycleAllocator(regs),
            &pools,
        );
        check_forced_edge(&text, arm);
    }
}
