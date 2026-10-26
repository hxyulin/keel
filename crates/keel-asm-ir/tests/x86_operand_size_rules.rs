//! Prefix interpretation remains stable when imported size rules become typed.
use keel_asm_ir::x86::isa::{FORMS, OperandSizeRule};

#[test]
fn operand_size_rules_preserve_prefix_interpretation() {
    // Columns: no prefix, 66, REX.W, 66 + REX.W. A mandatory 66 is
    // an opcode selector rather than an operand-size override.
    let cases = [
        (OperandSizeRule::Default, [32, 16, 64, 64], [32, 32, 64, 64]),
        (OperandSizeRule::Byte, [8; 4], [8; 4]),
        (
            OperandSizeRule::Default64,
            [64, 16, 64, 16],
            [64, 16, 64, 16],
        ),
        (OperandSizeRule::Force64, [64; 4], [64; 4]),
        (OperandSizeRule::Force32Or64, [64; 4], [64; 4]),
        (
            OperandSizeRule::Force32OrRexW,
            [32, 32, 64, 64],
            [32, 32, 64, 64],
        ),
        (
            OperandSizeRule::Ignore66,
            [32, 32, 64, 64],
            [32, 32, 64, 64],
        ),
        (OperandSizeRule::RexW32, [32, 16, 64, 64], [32, 32, 64, 64]),
    ];
    for (rule, ordinary, mandatory) in cases {
        for (prefix, expected) in [(-1, ordinary), (0x66, mandatory)] {
            let form = keel_asm_ir::x86::isa::IsaForm {
                osz: rule,
                prefix,
                ..FORMS[0]
            };
            for ((p66, w), size) in [(false, false), (true, false), (false, true), (true, true)]
                .into_iter()
                .zip(expected)
            {
                assert_eq!(
                    form.operand_size(p66, w),
                    size,
                    "rule={rule:?}, mandatory_prefix={prefix}, 66={p66}, REX.W={w}"
                );
            }
        }
    }
}
