mod support;
use support::execute;

fn optimize(src: &str) -> keel_ir::Module {
    let mut module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["simplify-cfg"]).unwrap();
    module
}

#[test]
fn forwarding_substitutes_permutations_and_repeated_arguments() {
    let src = "func @f(i64, i64, i8) -> i64 {
block0(v0: i64, v1: i64, v2: i8):
    brif v2, block1(v0, v1), block2(v0, v1)
block1(v3: i64, v4: i64):
    jump block3(v4, v3)
block2(v5: i64, v6: i64):
    jump block3(v5, v5)
block3(v7: i64, v8: i64):
    v9 = imul v7, v7
    v10 = iadd v9, v8
    return v10
}";
    let original = keel_ir::parse(src).unwrap();
    let optimized = optimize(src);
    assert_eq!(optimized.functions[0].blocks().count(), 2);
    for a in [0, 1, 7, u64::MAX] {
        for b in [0, 2, 9, u64::MAX] {
            for condition in [0, 1] {
                assert_eq!(
                    execute(&original.functions[0], &[a, b, condition]),
                    execute(&optimized.functions[0], &[a, b, condition])
                );
            }
        }
    }
    let mut second = optimized;
    assert!(
        !keel_opt::find_pass("simplify-cfg")
            .unwrap()
            .run(&mut second.functions[0])
    );
}

#[test]
fn externally_used_parameter_keeps_its_definition() {
    let src = "func @f(i64, i64, i8) -> i64 {
block0(v0: i64, v1: i64, v2: i8):
    brif v2, block1(v0), block1(v1)
block1(v3: i64):
    jump block2
block2:
    return v3
}";
    let optimized = optimize(src);
    assert_eq!(optimized.functions[0].blocks().count(), 2);
    assert_eq!(execute(&optimized.functions[0], &[40, 99, 0]), 99);
    assert_eq!(execute(&optimized.functions[0], &[40, 99, 1]), 40);
}

#[test]
fn forwarding_cycle_terminates_and_retains_nontermination() {
    let src = "func @f(i64) {
block0(v0: i64):
    jump block1(v0)
block1(v1: i64):
    jump block2(v1)
block2(v2: i64):
    jump block1(v2)
}";
    let mut module = optimize(src);
    assert_eq!(module.functions[0].blocks().count(), 2);
    assert!(
        !keel_opt::find_pass("simplify-cfg")
            .unwrap()
            .run(&mut module.functions[0])
    );
    assert!(module.functions[0].layout_insts().all(|(_, i)| matches!(
        module.functions[0].inst_data(i),
        keel_ir::InstData::Jump { .. }
    )));
}

#[test]
fn forwarding_preserves_parameterized_loop_backedges() {
    let src = "func @f(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i64 0
    jump block1(v1)
block1(v2: i64):
    v3 = iconst.i64 1
    v4 = iadd v2, v3
    v5 = icmp.ult v4, v0
    brif v5, block2(v4), block3(v4)
block2(v6: i64):
    jump block1(v6)
block3(v7: i64):
    return v7
}";
    let original = keel_ir::parse(src).unwrap();
    for subset in 0..32 {
        let other = ["const-fold", "sccp", "gvn", "dce", "gcm"];
        let mut names = vec!["simplify-cfg"];
        names.extend(
            other
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &name)| name),
        );
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, &names).unwrap();
        keel_opt::run_passes(&mut module, &names).unwrap();
        for input in 0..32 {
            assert_eq!(
                execute(&module.functions[0], &[input]),
                execute(&original.functions[0], &[input]),
                "passes {names:?}, input {input}"
            );
        }
    }
}

#[test]
fn merging_long_chain_preserves_instruction_order_and_external_parameter_uses() {
    // Arithmetic prevents forwarding, so merging must consume the chain.
    // Descending control flow also differs from the block layout order.
    let mut src = String::from("func @f(i64) -> i64 {\nblock0(v0: i64):\n jump block128(v0)\n");
    for block in 1..=128 {
        let param = block * 3;
        src.push_str(&format!(
            "block{block}(v{param}: i64):\n v{} = iconst.i64 {block}\n v{} = isub v{param}, v{}\n",
            param + 1,
            param + 2,
            param + 1,
        ));
        if block == 1 {
            // v0 is defined outside every merged block.
            src.push_str(&format!(" v999 = iadd v{}, v0\n return v999\n", param + 2));
        } else {
            src.push_str(&format!(" jump block{}(v{})\n", block - 1, param + 2));
        }
    }
    src.push('}');
    let optimized = optimize(&src);
    let func = &optimized.functions[0];
    assert_eq!(func.blocks().count(), 1);
    let constants: Vec<_> = func
        .layout_insts()
        .filter_map(|(_, inst)| match func.inst_data(inst) {
            keel_ir::InstData::Iconst { imm, .. } => Some(*imm),
            _ => None,
        })
        .collect();
    assert_eq!(constants, (1..=128).rev().collect::<Vec<_>>());
    for input in [0u64, 1, 8192, u64::MAX] {
        assert_eq!(
            execute(func, &[input]),
            input.wrapping_sub(128 * 129 / 2).wrapping_add(input)
        );
    }
    assert_eq!(optimized.to_string(), optimize(&src).to_string());
    let mut again = optimized.clone();
    assert!(
        !keel_opt::find_pass("simplify-cfg")
            .unwrap()
            .run(&mut again.functions[0])
    );
}

#[test]
fn merging_preserves_duplicate_branch_edges_and_diamond_join() {
    let src = "func @f(i64, i64, i8) -> i64 {
block0(v0: i64, v1: i64, v2: i8):
    jump block1
block1:
    v3 = iadd v0, v1
    brif v2, block2(v3), block2(v0)
block2(v4: i64):
    v5 = isub v4, v1
    brif v2, block3(v5), block4(v5)
block3(v6: i64):
    v7 = iadd v6, v0
    jump block5(v7)
block4(v8: i64):
    v9 = isub v8, v0
    jump block5(v9)
block5(v10: i64):
    jump block6(v10)
block6(v11: i64):
    return v11
}";
    let original = keel_ir::parse(src).unwrap();
    let optimized = optimize(src);
    assert_eq!(optimized.functions[0].blocks().count(), 5);
    for a in [0, 7, u64::MAX] {
        for b in [0, 11, u64::MAX] {
            for cond in [0, 1] {
                assert_eq!(
                    execute(&original.functions[0], &[a, b, cond]),
                    execute(&optimized.functions[0], &[a, b, cond]),
                );
            }
        }
    }
}

#[test]
fn merging_switch_dispatch_preserves_duplicate_label_edges_and_layout_holes() {
    // Computed gotos lower to a switch over label numbers. Both case edges
    // to block2 must survive even when the switch moves into block0.
    let src = "func @f(i32, i64) -> i64 {
block0(v0: i32, v1: i64):
    jump block1
block99:
    trap
block1:
    switch v0, block3(v1), [1: block2(v1), 2: block2(v1)]
block2(v2: i64):
    v3 = iadd v2, v1
    jump block4(v3)
block3(v4: i64):
    v5 = isub v4, v1
    jump block4(v5)
block4(v6: i64):
    return v6
}";
    let optimized = optimize(src);
    assert_eq!(optimized.functions[0].blocks().count(), 4);
    let switch = optimized.functions[0]
        .layout_insts()
        .find_map(|(_, inst)| match optimized.functions[0].inst_data(inst) {
            keel_ir::InstData::Switch { dests, .. } => Some(dests),
            _ => None,
        })
        .unwrap();
    assert_eq!(switch.len(), 3);
    assert_eq!(switch[1].block, switch[2].block);
    assert_ne!(switch[0].block, switch[1].block);
    assert_eq!(optimized.to_string(), optimize(src).to_string());
}

#[test]
fn merging_cycle_preserves_entry_and_creates_self_loop() {
    for two_block_cycle in [false, true] {
        // Both fixtures keep entry outside the cycle. Constants prevent
        // forwarding, so the two-block case must merge into a self-loop.
        let tail = if two_block_cycle {
            "jump block2\nblock2:\n    v1 = iconst.i64 2\n    jump block1"
        } else {
            "jump block1"
        };
        let src = format!(
            "func @f() {{
block0:
    jump block1
block1:
    v0 = iconst.i64 1
    {tail}
}}"
        );
        let optimized = optimize(&src);
        let func = &optimized.functions[0];
        let entry = func.entry_block().unwrap();
        assert_eq!(func.blocks().count(), 2);
        let loop_block = func.blocks().last().unwrap();
        assert_ne!(loop_block, entry);
        let entry_term = *func.block_insts(entry).last().unwrap();
        assert_eq!(
            func.inst_data(entry_term).branch_dests()[0].block,
            loop_block
        );
        let term = *func.block_insts(loop_block).last().unwrap();
        assert_eq!(func.inst_data(term).branch_dests()[0].block, loop_block);
        let constants: Vec<_> = func
            .layout_insts()
            .filter_map(|(_, inst)| match func.inst_data(inst) {
                keel_ir::InstData::Iconst { imm, .. } => Some(*imm),
                _ => None,
            })
            .collect();
        assert_eq!(
            constants,
            if two_block_cycle { vec![1, 2] } else { vec![1] }
        );
        keel_ir::verify_module(&optimized).unwrap();
        assert_eq!(optimized.to_string(), optimize(&src).to_string());
        let mut again = optimized.clone();
        assert!(
            !keel_opt::find_pass("simplify-cfg")
                .unwrap()
                .run(&mut again.functions[0])
        );
    }
}

#[test]
fn merging_moves_invoke_without_changing_normal_or_unwind_edges() {
    let src = "decl @may_throw(i32) -> i32
decl @personality()
func @f(i32) -> i32 personality @personality {
block0(v0: i32):
    jump block1
block1:
    v1 = invoke @may_throw(v0), block2, block3
block2:
    jump block4(v1)
block3:
    v2, v3 = landingpad cleanup
    return v0
block4(v4: i32):
    v5 = iadd v4, v0
    return v5
}";
    let optimized = optimize(src);
    let func = &optimized.functions[0];
    assert_eq!(func.blocks().count(), 3);
    let invoke = *func
        .block_insts(func.entry_block().unwrap())
        .last()
        .unwrap();
    let keel_ir::InstData::Invoke { dests, .. } = func.inst_data(invoke) else {
        panic!("entry must retain invoke");
    };
    assert_ne!(dests[0].block, dests[1].block);
    assert!(dests.iter().all(|dest| dest.args.is_empty()));
    assert!(matches!(
        func.inst_data(func.block_insts(dests[1].block)[0]),
        keel_ir::InstData::LandingPad { .. }
    ));
    assert_eq!(optimized.to_string(), optimize(src).to_string());
}
