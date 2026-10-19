//! SSA construction tests: build small programs the way a frontend would,
//! with mutable variables, and check the IR that comes out.

use keel_frontend::{FunctionBuilder, FunctionBuilderContext};
use keel_ir::{Function, IntCC, Signature, Type, verify};

fn sig(params: &[Type], returns: &[Type]) -> Signature {
    Signature::new(params, returns)
}

/// ```c
/// i64 sum(i64 n) { i64 i = 0, acc = 0; while (i < n) { acc += i; i += 1; } return acc; }
/// ```
#[test]
fn while_loop() {
    let mut func = Function::new("sum", sig(&[Type::I64], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let (entry, header, body, exit) = (
        b.create_block(),
        b.create_block(),
        b.create_block(),
        b.create_block(),
    );
    let (n, i, acc) = (
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
    );

    b.switch_to_block(entry);
    b.seal_block(entry);
    let arg = b.append_block_param(entry, Type::I64);
    b.def_var(n, arg);
    let zero = b.ins().iconst(Type::I64, 0);
    b.def_var(i, zero);
    b.def_var(acc, zero);
    b.ins().jump(header, &[]);

    // The back edge is not built yet, so the header stays unsealed.
    b.switch_to_block(header);
    let (iv, nv) = (b.use_var(i), b.use_var(n));
    let cond = b.ins().icmp(IntCC::Slt, iv, nv);
    b.ins().brif(cond, body, &[], exit, &[]);

    b.switch_to_block(body);
    b.seal_block(body);
    let (accv, iv) = (b.use_var(acc), b.use_var(i));
    let acc2 = b.ins().iadd(accv, iv);
    b.def_var(acc, acc2);
    let one = b.ins().iconst(Type::I64, 1);
    let i2 = b.ins().iadd(iv, one);
    b.def_var(i, i2);
    b.ins().jump(header, &[]);
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);
    let result = b.use_var(acc);
    b.ins().return_(&[result]);
    b.finalize();

    verify(&func).unwrap();
    // `n` got a header parameter while the header was unsealed; both
    // edges pass the same `v0`, so it was trivial and removed. `acc`'s
    // header parameter (v5) is created when the body reads it.
    assert_eq!(
        func.to_string(),
        "\
func @sum(i64) -> i64 cc(c) {
block0(v0: i64):
    v1 = iconst.i64 0
    jump block1(v1, v1)
block1(v2: i64, v5: i64):
    v4 = icmp.slt v2, v0
    brif v4, block2, block3
block2:
    v6 = iadd v5, v2
    v7 = iconst.i64 1
    v8 = iadd v2, v7
    jump block1(v8, v6)
block3:
    return v5
}
"
    );
}

/// ```c
/// i64 f(i64 x) { i64 y = 1; if (x) y = 2; else x = x + 1; return x + y; }
/// ```
#[test]
fn if_else_merges_both_variables() {
    let mut func = Function::new("f", sig(&[Type::I64], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let (entry, then_b, else_b, merge) = (
        b.create_block(),
        b.create_block(),
        b.create_block(),
        b.create_block(),
    );
    let (x, y) = (b.declare_var(Type::I64), b.declare_var(Type::I64));

    b.switch_to_block(entry);
    b.seal_block(entry);
    let arg = b.append_block_param(entry, Type::I64);
    b.def_var(x, arg);
    let one = b.ins().iconst(Type::I64, 1);
    b.def_var(y, one);
    let xv = b.use_var(x);
    b.ins().brif(xv, then_b, &[], else_b, &[]);

    b.switch_to_block(then_b);
    b.seal_block(then_b);
    let two = b.ins().iconst(Type::I64, 2);
    b.def_var(y, two);
    b.ins().jump(merge, &[]);

    b.switch_to_block(else_b);
    b.seal_block(else_b);
    let xv = b.use_var(x);
    let x1 = b.ins().iadd(xv, one);
    b.def_var(x, x1);
    b.ins().jump(merge, &[]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    let (xv, yv) = (b.use_var(x), b.use_var(y));
    let sum = b.ins().iadd(xv, yv);
    b.ins().return_(&[sum]);
    b.finalize();

    verify(&func).unwrap();
    assert_eq!(
        func.to_string(),
        "\
func @f(i64) -> i64 cc(c) {
block0(v0: i64):
    v1 = iconst.i64 1
    brif v0, block1, block2
block1:
    v2 = iconst.i64 2
    jump block3(v0, v2)
block2:
    v3 = iadd v0, v1
    jump block3(v3, v1)
block3(v4: i64, v5: i64):
    v6 = iadd v4, v5
    return v6
}
"
    );
}

/// Nested loops that only read `n`. Sealing each block as soon as its
/// predecessors are known removes every trivial parameter on the spot.
#[test]
fn nested_loops_sealed_early() {
    nested_loops(false);
}

/// The same, sealing everything at the end. Sealing the outer header gives
/// `n` a parameter on the still-unsealed latch, so the outer header's
/// parameter (args `v0` and the latch's) is not trivial yet. The latch's
/// becomes trivial later, which makes the outer header's trivial too; only
/// `finalize` sees that.
#[test]
fn nested_loops_sealed_at_end() {
    nested_loops(true);
}

fn nested_loops(seal_at_end: bool) {
    let seal = |b: &mut FunctionBuilder, block| {
        if !seal_at_end {
            b.seal_block(block);
        }
    };
    let mut func = Function::new("f", sig(&[Type::I64], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let entry = b.create_block();
    let outer = b.create_block();
    let inner = b.create_block();
    let inner_body = b.create_block();
    let outer_latch = b.create_block();
    let exit = b.create_block();
    let (n, i, j) = (
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
        b.declare_var(Type::I64),
    );

    b.switch_to_block(entry);
    seal(&mut b, entry);
    let arg = b.append_block_param(entry, Type::I64);
    b.def_var(n, arg);
    let zero = b.ins().iconst(Type::I64, 0);
    b.def_var(i, zero);
    b.ins().jump(outer, &[]);

    // for (i = 0; i < n; i++) for (j = 0; j < n; j++) {}
    b.switch_to_block(outer);
    let (iv, nv) = (b.use_var(i), b.use_var(n));
    let c = b.ins().icmp(IntCC::Slt, iv, nv);
    let zero = b.ins().iconst(Type::I64, 0);
    b.def_var(j, zero);
    b.ins().brif(c, inner, &[], exit, &[]);

    b.switch_to_block(inner);
    let (jv, nv) = (b.use_var(j), b.use_var(n));
    let c = b.ins().icmp(IntCC::Slt, jv, nv);
    b.ins().brif(c, inner_body, &[], outer_latch, &[]);

    b.switch_to_block(inner_body);
    seal(&mut b, inner_body);
    let jv = b.use_var(j);
    let one = b.ins().iconst(Type::I64, 1);
    let j2 = b.ins().iadd(jv, one);
    b.def_var(j, j2);
    b.ins().jump(inner, &[]);
    seal(&mut b, inner);

    b.switch_to_block(outer_latch);
    seal(&mut b, outer_latch);
    let iv = b.use_var(i);
    let one = b.ins().iconst(Type::I64, 1);
    let i2 = b.ins().iadd(iv, one);
    b.def_var(i, i2);
    b.ins().jump(outer, &[]);
    seal(&mut b, outer);

    b.switch_to_block(exit);
    seal(&mut b, exit);
    let result = b.use_var(i);
    b.ins().return_(&[result]);
    if seal_at_end {
        b.seal_all_blocks();
    }
    b.finalize();

    verify(&func).unwrap();
    let text = func.to_string();
    // Only i (outer) and j (inner) are loop-carried; n is v0 everywhere.
    assert!(text.contains("block1(v2: i64):"), "{text}");
    assert!(text.contains("block2(v6: i64):"), "{text}");
    assert!(text.contains("icmp.slt v2, v0"), "{text}");
    assert!(text.contains("icmp.slt v6, v0"), "{text}");
}

/// C reads an uninitialized local as some value; the builder gives zero,
/// also through a block parameter when only some paths define it.
#[test]
fn use_before_definition_reads_zero() {
    let mut func = Function::new("f", sig(&[Type::I8], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let (entry, then, join) = (b.create_block(), b.create_block(), b.create_block());
    let x = b.declare_var(Type::I64);

    b.switch_to_block(entry);
    b.seal_block(entry);
    let c = b.append_block_param(entry, Type::I8);
    b.ins().brif(c, then, &[], join, &[]);
    b.seal_block(then);

    b.switch_to_block(then);
    let one = b.ins().iconst(Type::I64, 1);
    b.def_var(x, one);
    b.ins().jump(join, &[]);
    b.seal_block(join);

    b.switch_to_block(join);
    let value = b.use_var(x);
    b.ins().return_(&[value]);
    b.finalize();

    verify(&func).unwrap();
    let text = func.to_string();
    assert!(text.contains("iconst.i64 0"), "{text}");
    assert!(text.contains("block2(v"), "{text}");
}

/// A conditional can have two edges to the same block. Each destination
/// needs an argument, including when removing a trivial SSA parameter.
#[test]
fn duplicate_destinations_preserve_explicit_arguments() {
    let mut func = Function::new("duplicate", sig(&[Type::I8], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let entry = b.create_block();
    let join = b.create_block();
    let x = b.declare_var(Type::I64);
    let cond = b.append_block_param(entry, Type::I8);
    let explicit = b.append_block_param(join, Type::I64);
    b.switch_to_block(entry);
    b.seal_block(entry);
    let one = b.ins().iconst(Type::I64, 1);
    let two = b.ins().iconst(Type::I64, 2);
    b.def_var(x, one);
    b.ins().brif(cond, join, &[one], join, &[two]);
    b.switch_to_block(join);
    let local = b.use_var(x);
    b.seal_block(join);
    let sum = b.ins().iadd(explicit, local);
    b.ins().return_(&[sum]);
    b.finalize();
    verify(&func).unwrap();
    assert_eq!(func.block_params(join), &[explicit]);
    let branch = *func.block_insts(entry).last().unwrap();
    let dests = func.inst_data(branch).branch_dests();
    assert_eq!(dests[0].args, [one]);
    assert_eq!(dests[1].args, [two]);
    assert_eq!(func.resolve(local), one);
}

/// A destination rewritten through raw Function access must invalidate an
/// edge that was indexed earlier, before either destination is sealed.
#[test]
fn raw_destination_rewrite_before_sealing() {
    let mut func = Function::new("redirect", sig(&[Type::I8], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let entry = b.create_block();
    let left = b.create_block();
    let right = b.create_block();
    let join = b.create_block();
    let unreachable = b.create_block();
    let x = b.declare_var(Type::I64);
    let cond = b.append_block_param(entry, Type::I8);
    b.switch_to_block(entry);
    b.seal_block(entry);
    b.ins().brif(cond, left, &[], right, &[]);
    b.switch_to_block(left);
    b.seal_block(left);
    let one = b.ins().iconst(Type::I64, 1);
    b.def_var(x, one);
    let branch = b.ins().jump(unreachable, &[]);
    b.switch_to_block(right);
    b.seal_block(right); // indexes left's original destination
    let two = b.ins().iconst(Type::I64, 2);
    b.def_var(x, two);
    b.ins().jump(join, &[]);
    b.function_mut().inst_data_mut(branch).branch_dests_mut()[0].block = join;
    b.switch_to_block(join);
    let result = b.use_var(x);
    b.seal_block(join);
    b.ins().return_(&[result]);
    b.switch_to_block(unreachable);
    b.seal_block(unreachable);
    let zero = b.use_var(x);
    b.ins().return_(&[zero]);
    b.finalize();
    verify(&func).unwrap();
    assert_eq!(func.block_params(join), &[result]);
    assert_eq!(func.inst_data(branch).branch_dests()[0].args, [one]);
    assert!(func.to_string().contains("iconst.i64 0"));
}

/// A read after the loop is complete, so every block is sealed: the join
/// `j` is reached again through the loop while its predecessors are read.
/// ```c
/// i64 f(i8 c, i8 d) { i64 x = 1; while (c) { if (d) x = 2; } return x; }
/// ```
/// `x` needs a parameter at the header (1 or the latch's) and at the latch
/// (2 or the header's); both are created only once the read finds that.
#[test]
fn join_reached_again_around_a_loop_gets_its_parameter() {
    let mut func = Function::new("f", sig(&[Type::I8, Type::I8], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let [entry, header, body, set, latch, exit] = [(); 6].map(|_| b.create_block());
    let x = b.declare_var(Type::I64);

    b.switch_to_block(entry);
    b.seal_block(entry);
    let c = b.append_block_param(entry, Type::I8);
    let d = b.append_block_param(entry, Type::I8);
    let one = b.ins().iconst(Type::I64, 1);
    b.def_var(x, one);
    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    b.ins().brif(c, body, &[], exit, &[]);
    b.switch_to_block(body);
    b.ins().brif(d, set, &[], latch, &[]);
    b.switch_to_block(set);
    let two = b.ins().iconst(Type::I64, 2);
    b.def_var(x, two);
    b.ins().jump(latch, &[]);
    b.switch_to_block(latch);
    b.ins().jump(header, &[]);
    b.seal_all_blocks();

    b.switch_to_block(exit);
    let result = b.use_var(x);
    b.ins().return_(&[result]);
    b.finalize();

    verify(&func).unwrap();
    assert_eq!(
        func.to_string(),
        "\
func @f(i8, i8) -> i64 cc(c) {
block0(v0: i8, v1: i8):
    v2 = iconst.i64 1
    jump block1(v2)
block1(v4: i64):
    brif v0, block2, block5
block2:
    brif v1, block3, block4(v4)
block3:
    v3 = iconst.i64 2
    jump block4(v3)
block4(v5: i64):
    jump block1(v5)
block5:
    return v4
}
"
    );
}

/// The same loop without the assignment: the read comes back around the
/// loop, so the header's parameter is made, then removed as trivial.
#[test]
fn trivial_join_around_a_loop_reads_the_entry_value() {
    let mut func = Function::new("f", sig(&[Type::I8], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let [entry, header, body, exit] = [(); 4].map(|_| b.create_block());
    let x = b.declare_var(Type::I64);

    b.switch_to_block(entry);
    b.seal_block(entry);
    let c = b.append_block_param(entry, Type::I8);
    let one = b.ins().iconst(Type::I64, 1);
    b.def_var(x, one);
    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    b.ins().brif(c, body, &[], exit, &[]);
    b.switch_to_block(body);
    b.ins().jump(header, &[]);
    b.seal_all_blocks();

    b.switch_to_block(exit);
    let result = b.use_var(x);
    assert_eq!(result, one);
    b.ins().return_(&[result]);
    b.finalize();

    verify(&func).unwrap();
    assert!(func.to_string().contains("block1:\n"), "{func}");
}

/// A join whose predecessors agree, with no loop through it, never gets
/// a parameter value at all, not even a removed one.
#[test]
fn trivial_acyclic_join_spends_no_value() {
    let mut func = Function::new("f", sig(&[Type::I8], &[Type::I64]));
    let mut b = FunctionBuilder::new(&mut func);
    let [entry, then, other, join] = [(); 4].map(|_| b.create_block());
    let x = b.declare_var(Type::I64);

    b.switch_to_block(entry);
    b.seal_block(entry);
    let c = b.append_block_param(entry, Type::I8);
    let one = b.ins().iconst(Type::I64, 1);
    b.def_var(x, one);
    b.ins().brif(c, then, &[], other, &[]);
    for block in [then, other] {
        b.seal_block(block);
        b.switch_to_block(block);
        b.ins().jump(join, &[]);
    }
    b.seal_block(join);
    b.switch_to_block(join);
    let result = b.use_var(x);
    assert_eq!(result, one);
    b.ins().return_(&[result]);
    b.finalize();

    verify(&func).unwrap();
    // v0 and v1 only: no parameter value was made and then removed.
    assert_eq!(func.num_values(), 2);
    assert!(func.to_string().contains("block3:\n"), "{func}");
}

/// `n` sequential diamonds, each assigning one of three variables on one
/// side, inside a counted loop; every variable is read after each join.
fn diamonds(b: &mut FunctionBuilder, n: usize) {
    let entry = b.create_block();
    b.switch_to_block(entry);
    b.seal_block(entry);
    let arg = b.append_block_param(entry, Type::I64);
    let vars = [(); 3].map(|_| b.declare_var(Type::I64));
    let zero = b.ins().iconst(Type::I64, 0);
    for &v in &vars {
        b.def_var(v, zero);
    }
    let header = b.create_block();
    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    for k in 0..n {
        let (then, other, join) = (b.create_block(), b.create_block(), b.create_block());
        let c = b.ins().icmp(IntCC::Slt, arg, zero);
        b.ins().brif(c, then, &[], other, &[]);
        b.seal_block(then);
        b.seal_block(other);
        b.switch_to_block(then);
        let old = b.use_var(vars[k % 3]);
        let new = b.ins().iadd(old, arg);
        b.def_var(vars[k % 3], new);
        b.ins().jump(join, &[]);
        b.switch_to_block(other);
        b.ins().jump(join, &[]);
        b.seal_block(join);
        b.switch_to_block(join);
        let sum = vars.map(|v| b.use_var(v));
        let s = b.ins().iadd(sum[0], sum[1]);
        let s = b.ins().iadd(s, sum[2]);
        b.def_var(vars[(k + 1) % 3], s);
    }
    let exit = b.create_block();
    let c = b.ins().icmp(IntCC::Slt, arg, zero);
    b.ins().brif(c, header, &[], exit, &[]);
    b.seal_block(header);
    b.seal_block(exit);
    b.switch_to_block(exit);
    let result = b.use_var(vars[0]);
    b.ins().return_(&[result]);
}

/// Handing the builder's tables from one function to the next, from big to
/// small and back, builds exactly what fresh builders do.
#[test]
fn reused_context_builds_the_same_ir() {
    let sizes = [40, 1, 0, 25, 3, 60, 2];
    let fresh: Vec<String> = sizes
        .iter()
        .map(|&n| {
            let mut func = Function::new("f", sig(&[Type::I64], &[Type::I64]));
            let mut b = FunctionBuilder::new(&mut func);
            diamonds(&mut b, n);
            let _: () = b.finalize();
            verify(&func).unwrap();
            format!("{func:?}")
        })
        .collect();
    let mut context = FunctionBuilderContext::new();
    for (&n, want) in sizes.iter().zip(&fresh) {
        let mut func = Function::new("f", sig(&[Type::I64], &[Type::I64]));
        let mut b = FunctionBuilder::with_context(&mut func, context);
        diamonds(&mut b, n);
        context = b.finalize_with_context();
        assert_eq!(&format!("{func:?}"), want, "diamonds({n})");
    }
}
