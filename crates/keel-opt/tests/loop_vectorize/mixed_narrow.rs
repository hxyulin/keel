use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

const SOURCE: &str = include_str!("mixed_narrow.keel");
fn optimize(source: &str, target: VectorTarget) -> keel_ir::Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes_for_target(&mut module, &["loop-reduce-multi"], target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
fn vectorized(module: &keel_ir::Module) -> bool {
    module.functions[0]
        .layout_insts()
        .any(|(_, i)| module.functions[0].inst_data(i).is_vector_instruction())
}
fn execute(module: &keel_ir::Module, input: &[i16], n: u64) -> Outcome {
    let mut interp = Interpreter::new(module).with_step_limit(10_000_000);
    let bytes: Vec<_> = input.iter().flat_map(|x| x.to_le_bytes()).collect();
    let data = interp.memory_mut().alloc_bytes(&bytes, 16);
    interp.call("k_narrow", &[Val::Ptr(data), Val::I64(n)])
}
fn reference(input: &[i16]) -> i64 {
    input
        .iter()
        .map(|&v| {
            let c = v.clamp(-128, 127) as i8;
            let m = ((v as u8 as i32) * 3 + ((v >> 8) as i8 as i32) * 5 + ((v as u16) >> 2) as i32)
                as u16;
            (c as i32 * 7 + m as i32 - v.wrapping_mul(3) as i32 / 7 + ((v >> 3) as u8) as i32)
                as i64
        })
        .sum()
}
#[test]
fn narrow_mixed_rings_constant_shifts_and_division_vectorize_exactly() {
    let after = optimize(SOURCE, VectorTarget::Aarch64Neon);
    assert!(vectorized(&after), "{after}");
    assert!(
        after.to_string().contains("swiden_high"),
        "widen both halves before carrying the i64 sum: {after}"
    );
    let text = after.to_string();
    assert!(
        text.contains("spairwise_add_long"),
        "eight-lane signed contributions: {text}"
    );
    assert!(
        text.contains("upairwise_add_long"),
        "eight-lane unsigned contributions: {text}"
    );
    assert!(text.contains("vsmin"), "{text}");
    assert!(
        !text.contains("vxor"),
        "signed casts and bounded division: {text}"
    );
    let mut again = after.clone();
    keel_opt::run_passes_for_target(
        &mut again,
        &["loop-reduce-multi"],
        VectorTarget::Aarch64Neon,
    )
    .unwrap();
    assert_eq!(again.to_string(), after.to_string());
    // Cover every narrow signed input, including division remainders and wrap
    // boundaries, while retaining exact-size reads and scalar tails.
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for n in [0, 1, 3, 7, 8, 9, 31, 65, 65536] {
        assert_eq!(
            execute(&after, &input[..n], n as u64),
            Outcome::Return(vec![Val::I64(reference(&input[..n]) as u64)]),
            "n={n}"
        );
    }
}
#[test]
fn narrow_recipe_rejects_effects_unbounded_division_and_unsupported_targets() {
    for target in [VectorTarget::Portable, VectorTarget::X86Sse2] {
        let after = optimize(SOURCE, target);
        assert!(!vectorized(&after));
    }
    for source in [
        SOURCE.replace("load.i16 trap_ub", "load.i16 volatile"),
        SOURCE.replace("sdiv v29, v21", "sdiv v24, v21"),
        SOURCE.replace("iconst.i32 7", "iconst.i32 0"),
        SOURCE.replace("iconst.i32 7", "iconst.i32 -1"),
    ] {
        let after = optimize(&source, VectorTarget::Aarch64Neon);
        assert!(!vectorized(&after), "{after}");
    }
}
#[test]
fn narrow_division_handles_negative_divisors_and_large_quotient_boundaries() {
    let input: Vec<_> = [
        i16::MIN,
        -32767,
        -22938,
        -128,
        -8,
        -7,
        -6,
        -1,
        0,
        1,
        6,
        7,
        8,
        127,
        22937,
        32767,
    ]
    .into_iter()
    .cycle()
    .take(65)
    .collect();
    for narrow in ["i8", "i16"] {
        for divisor in [-32768, -127, -7, -2, 2, 3, 7, 127, 32768] {
            let source = SOURCE
                .replace("v28 = trunc.i16", &format!("v28 = trunc.{narrow}"))
                .replace(
                    "v31 = sdiv v29, v21",
                    &format!("v90 = iconst.i32 {divisor}\n    v31 = sdiv v29, v90"),
                );
            let before = keel_ir::parse(&source).unwrap();
            let after = optimize(&source, VectorTarget::Aarch64Neon);
            assert!(vectorized(&after), "d={divisor}: {after}");
            let exhaustive: Vec<_>;
            let values = if narrow == "i16" && matches!(divisor, -7 | 3 | 7 | 127) {
                exhaustive = (i16::MIN..=i16::MAX).collect();
                exhaustive.as_slice()
            } else {
                input.as_slice()
            };
            assert_eq!(
                execute(&before, values, values.len() as u64),
                execute(&after, values, values.len() as u64),
                "d={divisor}"
            );
        }
    }
}

#[test]
fn small_narrow_dag_keeps_exact_threshold_and_i64_accumulator_ring() {
    let source = "func @k_narrow(ptr,i64)->i64 {
block0(v0:ptr,v1:i64):
 v2=iconst.i64 0
 v3=iconst.i64 9223372036854775800
 jump block1(v2,v3)
block1(v4:i64,v5:i64):
 v6=icmp.ult v4,v1
 brif v6,block2,block3
block2:
 v7=iconst.i64 1
 v8=shl v4,v7
 v9=ptradd v0,v8
 v10=load.i16 trap_ub v9
 v11=sext.i32 v10
 v12=iconst.i32 7
 v13=sdiv v11,v12
 v14=sext.i64 v13
 v15=iadd v5,v14
 v16=iadd v4,v7
 jump block1(v16,v15)
block3:
 return v5
}";
    let after = optimize(source, VectorTarget::Aarch64Neon);
    assert!(after.to_string().contains("swiden_high"), "{after}");
    let input: Vec<i16> = [i16::MIN, i16::MAX, 21, -28, 35, 42, 49, 56]
        .into_iter()
        .cycle()
        .take(65536)
        .collect();
    for n in [0, 1, 7, 8, 9, 15, 16, 17, 65536] {
        let expected = input[..n]
            .iter()
            .fold(i64::MAX - 7, |s, &x| s.wrapping_add(i64::from(x) / 7));
        assert_eq!(
            execute(&after, &input[..n], n as u64),
            Outcome::Return(vec![Val::I64(expected as u64)]),
            "n={n}"
        );
    }
}

#[test]
fn eight_lane_recipe_preserves_casts_and_rejects_unproved_demotions() {
    let cases = [
        // The intermediate byte cast must wrap outside the usual clamp range.
        (
            SOURCE.replace(" -128", " -200").replace(" 127", " 200"),
            true,
        ),
        // The mixed unsigned contribution wraps independently at 16 bits.
        (SOURCE.replace("iconst.i32 5", "iconst.i32 512"), true),
        // Shifting a promoted value outside the signed i16 range needs i32 lanes.
        (SOURCE.replace("sshr v47, v62", "sshr v27, v62"), false),
        // Splitting an overflowing i32 add tree across the i64 extension is wrong.
        (
            SOURCE.replace(
                "v38 = iadd v32, v37",
                "v900 = iconst.i32 2147483647\n    v901 = iadd v32, v900\n    v38 = iadd v901, v37",
            ),
            false,
        ),
        // Negative unsigned contributions are outside this bounded recipe.
        (
            SOURCE.replace("v24 = iadd v22, v23", "v24 = isub v22, v23"),
            false,
        ),
    ];
    let input: Vec<i16> = [
        i16::MIN,
        -32767,
        -21846,
        -10923,
        -201,
        -200,
        -129,
        -128,
        -127,
        -7,
        -1,
        0,
        1,
        7,
        126,
        127,
        128,
        199,
        200,
        201,
        10922,
        21845,
        32766,
        i16::MAX,
    ]
    .into_iter()
    .cycle()
    .take(257)
    .collect();
    for (source, narrow) in cases {
        let before = keel_ir::parse(&source).unwrap();
        let after = optimize(&source, VectorTarget::Aarch64Neon);
        let text = after.to_string();
        assert_eq!(text.contains("spairwise_add_long"), narrow, "{text}");
        for n in [0, 1, 7, 8, 9, 15, 16, 17, 31, 257] {
            assert_eq!(
                execute(&after, &input[..n], n as u64),
                execute(&before, &input[..n], n as u64),
                "n={n}: {source}"
            );
        }
    }
}

#[test]
fn packed_quotients_keep_the_scalar_division_source_location() {
    let mut m = keel_ir::parse(SOURCE).unwrap();
    let file = m.add_file("narrow.c");
    let loc = Some(keel_ir::SourceLoc {
        file,
        line: 17,
        col: 9,
        scope: None,
    });
    let f = &mut m.functions[0];
    for i in f.layout_insts().map(|(_, i)| i).collect::<Vec<_>>() {
        if matches!(
            f.inst_data(i),
            keel_ir::InstData::Binary {
                op: keel_ir::BinaryOp::Sdiv,
                ..
            }
        ) {
            f.set_inst_loc(i, loc);
        }
    }
    keel_opt::run_passes_for_target(&mut m, &["loop-reduce-multi"], VectorTarget::Aarch64Neon)
        .unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let shuffles: Vec<_> = f
        .layout_insts()
        .map(|(_, i)| i)
        .filter(|&i| matches!(f.inst_data(i), keel_ir::InstData::VectorShuffle { .. }))
        .collect();
    assert!(!shuffles.is_empty(), "{}", m);
    for i in shuffles {
        assert_eq!(f.inst_loc(i), loc);
    }
}
