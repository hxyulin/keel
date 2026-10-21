//! Exact signed low-i32 absolute differences accumulated modulo 2^64.
use super::*;
use keel_ir::{VectorBinaryOp as V, VectorType};

#[derive(Clone, Copy, Debug)]
pub(super) struct ExactSabal {
    pub accumulator: Value,
    pub left: Value,
    pub right: Value,
}

// Check the declared result, not merely the result reached through aliases.
fn producer(f: &Function, i: Inst, ty: Type) -> Option<Value> {
    let [r] = *f.inst_results(i) else { return None };
    (f.resolve(r) == r && f.value_def(r) == ValueDef::Result(i, 0) && f.value_type(r) == ty)
        .then_some(r)
}
fn zero(f: &Function, v: Value) -> bool {
    let v = f.resolve(v);
    let ValueDef::Result(s, 0) = f.value_def(v) else {
        return false;
    };
    if producer(f, s, Type::Vector(VectorType::I64x2)) != Some(v) {
        return false;
    }
    let InstData::VectorSplat {
        ty: Type::Vector(VectorType::I64x2),
        arg,
    } = *f.inst_data(s)
    else {
        return false;
    };
    let arg = f.resolve(arg);
    let ValueDef::Result(c, 0) = f.value_def(arg) else {
        return false;
    };
    producer(f, c, Type::I64) == Some(arg)
        && matches!(
            f.inst_data(c),
            InstData::Iconst {
                ty: Type::I64,
                imm: 0
            }
        )
}

pub(super) fn select(
    f: &Function,
    plan: &mut crate::select::Selection,
) -> HashMap<Inst, ExactSabal> {
    let mut selected = HashMap::default();
    // Every chain starts with a low I32x4 -> I64x2 widening; without one (or
    // with inline assembly) there is nothing to build use lists for.
    let mut candidate = false;
    for (_, i) in f.layout_insts() {
        match f.inst_data(i) {
            InstData::InlineAsm { .. } => return selected,
            InstData::Convert {
                op: ConvertOp::SwidenLow,
                ty: Type::Vector(VectorType::I64x2),
                ..
            } => candidate = true,
            _ => {}
        }
    }
    if !candidate {
        return selected;
    }
    // Operand positions are occurrence identities. In particular, duplicate
    // successor arguments and repeated operands must never collapse to a set.
    let mut uses = vec![Vec::new(); f.num_values()];
    for (_, i) in f.layout_insts() {
        let mut n = 0;
        f.inst_data(i).for_each_operand(|v| {
            uses[f.resolve(v).index()].push((i, n));
            n += 1;
        });
    }
    for b in f.blocks() {
        for chain in f.block_insts(b).windows(7) {
            if !matches!(
                f.inst_data(chain[0]),
                InstData::Convert {
                    op: ConvertOp::SwidenLow,
                    ..
                }
            ) {
                continue;
            }
            let loc = f.inst_loc(chain[0]).map(|l| (l.file, l.line));
            if chain.iter().any(|&i| {
                let mut raw = Vec::new();
                f.inst_data(i).for_each_operand(|v| raw.push(f.resolve(v)));
                plan.covered(i)
                    || plan.matches.contains_key(&i)
                    || plan.conditions.contains_key(&i)
                    || plan.compare_memory.contains_key(&i)
                    || plan.binary_memory.contains_key(&i)
                    || plan.division_remainders.contains_key(&i)
                    || plan.select_compares.contains_key(&i)
                    || plan.has_constraint(i)
                    || plan.operands(f, i) != raw
                    || f.inst_loc(i).map(|l| (l.file, l.line)) != loc
            }) {
                continue;
            }
            let [cw, pw, sub, cmp, neg, mag, root] = <[Inst; 7]>::try_from(chain).unwrap();
            let Some(values) = chain
                .iter()
                .map(|&i| producer(f, i, Type::Vector(VectorType::I64x2)))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let [c, p, d, m, n, a, _r] = <[Value; 7]>::try_from(values).unwrap();
            // Canonical result ownership also makes all seven producers distinct.
            let InstData::Convert {
                op: ConvertOp::SwidenLow,
                ty: Type::Vector(VectorType::I64x2),
                arg: left,
            } = *f.inst_data(cw)
            else {
                continue;
            };
            let InstData::Convert {
                op: ConvertOp::SwidenLow,
                ty: Type::Vector(VectorType::I64x2),
                arg: right,
            } = *f.inst_data(pw)
            else {
                continue;
            };
            let left = f.resolve(left);
            let right = f.resolve(right);
            if [left, right]
                .iter()
                .any(|&v| f.value_type(v) != Type::Vector(VectorType::I32x4))
            {
                continue;
            }
            let InstData::VectorBinary { op: V::Isub, args } = *f.inst_data(sub) else {
                continue;
            };
            if args.map(|v| f.resolve(v)) != [c, p] {
                continue;
            }
            let InstData::VectorIcmp {
                cond: IntCC::Slt,
                args,
            } = *f.inst_data(cmp)
            else {
                continue;
            };
            if f.resolve(args[0]) != d || !zero(f, args[1]) {
                continue;
            }
            let InstData::VectorUnary {
                op: UnaryOp::Ineg,
                arg,
            } = *f.inst_data(neg)
            else {
                continue;
            };
            if f.resolve(arg) != d {
                continue;
            }
            let InstData::VectorSelect { args } = *f.inst_data(mag) else {
                continue;
            };
            if args.map(|v| f.resolve(v)) != [m, n, d] {
                continue;
            }
            let InstData::VectorBinary { op: V::Iadd, args } = *f.inst_data(root) else {
                continue;
            };
            let args = args.map(|v| f.resolve(v));
            let (accumulator, abs_operand) = match args {
                [x, y] if y == a && x != a => (x, 1),
                [x, y] if x == a && y != a => (y, 0),
                _ => continue,
            };
            if f.value_type(accumulator) != Type::Vector(VectorType::I64x2) {
                continue;
            }
            let expected = [
                (c, vec![(sub, 0)]),
                (p, vec![(sub, 1)]),
                (d, vec![(cmp, 0), (neg, 0), (mag, 2)]),
                (m, vec![(mag, 0)]),
                (n, vec![(mag, 1)]),
                (a, vec![(root, abs_operand)]),
            ];
            if expected.into_iter().any(|(v, mut wanted)| {
                let mut actual = uses[v.index()].clone();
                actual.sort_by_key(|&(i, n)| (i.index(), n));
                wanted.sort_by_key(|&(i, n)| (i.index(), n));
                actual != wanted
            }) {
                continue;
            }
            for &i in &chain[..6] {
                plan.cover(i);
            }
            plan.set_operands(root, vec![accumulator, left, right]);
            selected.insert(
                root,
                ExactSabal {
                    accumulator,
                    left,
                    right,
                },
            );
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        regalloc::{Constraint, OperandKind, ValuePart},
        select::Selection,
    };
    const SOURCE: &str = "func @test(i64x2,i32x4,i32x4) -> i64x2 {
block0(v0:i64x2,v1:i32x4,v2:i32x4):
 v3 = iconst.i64 0
 v4 = vsplat.i64x2 v3
 v5 = swiden_low.i64x2 v1
 v6 = swiden_low.i64x2 v2
 v7 = visub v5,v6
 v8 = vicmp.slt v7,v4
 v9 = vineg v7
 v10 = vselect v8,v9,v7
 v11 = viadd v0,v10
 return v11
}";
    fn function(source: &str) -> Function {
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        m.functions.into_iter().next().unwrap()
    }
    fn chain(f: &Function) -> Vec<Inst> {
        f.block_insts(f.entry_block().unwrap())[2..9].to_vec()
    }
    fn reject(f: &Function) {
        let mut s = Selection::default();
        assert!(select(f, &mut s).is_empty());
        assert!(f.layout_insts().all(|(_, i)| !s.covered(i)));
    }
    #[test]
    fn exact_root_operands_multiplicity_and_covered_operands() {
        for same in [false, true] {
            let f = function(&if same {
                SOURCE.replace("swiden_low.i64x2 v2", "swiden_low.i64x2 v1")
            } else {
                SOURCE.to_owned()
            });
            let mut s = Selection::default();
            let found = select(&f, &mut s);
            assert_eq!(found.len(), 1);
            let (&root, cover) = found.iter().next().unwrap();
            assert_eq!(
                s.operands(&f, root),
                vec![cover.accumulator, cover.left, cover.right]
            );
            assert_eq!(
                s.operands(&f, root),
                vec![
                    Value::new(0),
                    Value::new(1),
                    Value::new(if same { 1 } else { 2 })
                ]
            );
            for &i in &chain(&f)[..6] {
                assert!(s.covered(i));
                assert!(s.operands(&f, i).is_empty());
            }
            assert!(!s.covered(root));
        }
    }
    #[test]
    fn every_producer_root_splat_and_literal_requires_fresh_canonical_result() {
        let f = function(SOURCE);
        for i in chain(&f)
            .into_iter()
            .chain([f.block_insts(f.entry_block().unwrap())[1]])
        {
            let mut aliased = f.clone();
            aliased.replace_all_uses(f.first_result(i), Value::new(0));
            reject(&aliased);
        }
        // A zero operand through a scalar parameter is not a literal proof.
        let mut f = function(
            &SOURCE
                .replace("i32x4) ->", "i32x4,i64) ->")
                .replace("v2:i32x4):", "v2:i32x4,v12:i64):"),
        );
        let literal = f.first_result(f.block_insts(f.entry_block().unwrap())[0]);
        let param = *f.block_params(f.entry_block().unwrap()).last().unwrap();
        f.replace_all_uses(literal, param);
        reject(&f);
    }
    #[test]
    fn resolved_operand_aliases_are_accepted_but_external_alias_uses_reject() {
        for v in 5..=11 {
            let src = SOURCE.replace(" v5 =", " v12 = viadd v0,v0\n v5 =");
            let mut f = function(&src);
            let insts = f.block_insts(f.entry_block().unwrap());
            let target = f.first_result(insts[v - 2]);
            let alias = f.first_result(insts[2]);
            // Replace only operand occurrences with the alias placeholder.
            let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
            for i in insts {
                f.inst_data_mut(i).for_each_operand_mut(|arg| {
                    if *arg == target {
                        *arg = alias
                    }
                });
            }
            f.replace_all_uses(alias, target);
            keel_ir::verify(&f).unwrap();
            assert_eq!(
                select(&f, &mut Selection::default()).len(),
                1,
                "operand alias v{v}"
            );
        }
        for v in 5..=10 {
            let src=SOURCE.replace("return v11", &format!("v12 = viadd v0,v0\n jump block1(v11,v12,v12)\nblock1(v13:i64x2,v14:i64x2,v15:i64x2):\n return v13"));
            let mut f = function(&src);
            f.replace_all_uses(Value::new(12), Value::new(v));
            reject(&f);
        }
    }
    #[test]
    fn every_constraint_debug_boundary_or_prior_claim_rejects_atomically() {
        let f = function(SOURCE);
        for i in chain(&f) {
            for kind in [OperandKind::Use, OperandKind::Def] {
                let mut s = Selection::default();
                s.set_constraint(i, ValuePart::low(Value::new(0)), kind, Constraint::Any);
                assert!(select(&f, &mut s).is_empty());
                assert!(chain(&f).iter().all(|&at| !s.covered(at)));
            }
            let mut debug = f.clone();
            debug.set_inst_loc(
                i,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line: 2,
                    col: 1,
                    scope: None,
                }),
            );
            reject(&debug);
            let mut s = Selection::default();
            s.set_operands(i, vec![]);
            assert!(select(&f, &mut s).is_empty());
            let mut s = Selection::default();
            s.cover(i);
            assert!(select(&f, &mut s).is_empty());
        }
        let mut debug = f.clone();
        for i in chain(&f) {
            debug.set_inst_loc(
                i,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line: 2,
                    col: 1,
                    scope: None,
                }),
            );
        }
        assert_eq!(select(&debug, &mut Selection::default()).len(), 1);
        let i = chain(&f)[6];
        debug.set_inst_loc(
            i,
            Some(keel_ir::SourceLoc {
                file: keel_ir::SourceFile::new(0),
                line: 3,
                col: 1,
                scope: None,
            }),
        );
        reject(&debug);
    }
}
