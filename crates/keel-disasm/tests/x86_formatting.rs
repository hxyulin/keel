use keel_asm_ir::x86::{EvexOptions, Rounding, isa};
use keel_disasm::x86::{Instruction, Operand};

fn instruction(operands: Vec<Operand>) -> Instruction {
    Instruction {
        address: 0,
        bytes: Vec::new(),
        form: &isa::forms("mov")[0],
        operand_size: 64,
        operands,
        lock: false,
        repeat: 0,
        evex: None,
    }
}

#[test]
fn memory_formatting_preserves_widths_segments_and_displacement_boundaries() {
    for (base, index, scale, displacement, expected) in [
        (None, None, 1, 0, "qword ptr fs:[0]"),
        (
            None,
            None,
            1,
            i64::MIN,
            "qword ptr fs:[-9223372036854775808]",
        ),
        (
            Some("rax"),
            None,
            1,
            i64::MIN,
            "qword ptr fs:[rax - 9223372036854775808]",
        ),
        (
            Some("rax"),
            Some("rcx"),
            4,
            16,
            "qword ptr fs:[rax + rcx*4 + 16]",
        ),
        (None, Some("rcx"), 1, -16, "qword ptr fs:[rcx - 16]"),
        (Some(""), Some(""), 1, 0, "qword ptr fs:[0]"),
        (Some("rax"), Some(""), 1, 0, "qword ptr fs:[rax + ]"),
    ] {
        let operand = Operand::Memory {
            bits: 64,
            address_bits: 64,
            base: base.map(str::to_owned),
            index: index.map(str::to_owned),
            scale,
            displacement,
            segment: Some("fs"),
        };
        assert_eq!(
            instruction(vec![operand]).assembly(),
            format!("mov {expected}")
        );
    }
    for (bits, width) in [
        (8, "byte"),
        (16, "word"),
        (32, "dword"),
        (48, "fword"),
        (64, "qword"),
        (80, "tbyte"),
        (128, "xmmword"),
        (256, "ymmword"),
        (512, "zmmword"),
        (0, ""),
    ] {
        let operand = Operand::Memory {
            bits,
            address_bits: 64,
            base: None,
            index: None,
            scale: 1,
            displacement: 0,
            segment: None,
        };
        let expected = if width.is_empty() {
            "mov [0]".to_owned()
        } else {
            format!("mov {width} ptr [0]")
        };
        assert_eq!(
            instruction(vec![operand]).assembly(),
            expected,
            "width {bits}"
        );
    }
}

#[test]
fn evex_decorators_keep_their_operand_positions() {
    let mut i = instruction(vec![
        Operand::Register("zmm0".into()),
        Operand::Register("zmm1".into()),
        Operand::Immediate(7),
    ]);
    i.evex = Some(EvexOptions {
        address32: true,
        mask: Some(3),
        zero: true,
        rounding: Some(Rounding::Nearest),
        ..EvexOptions::default()
    });
    assert_eq!(
        i.assembly(),
        "addr32 {evex} mov zmm0{k3}{z}, zmm1, {rn-sae}, 7"
    );
    i.operands.pop();
    assert_eq!(
        i.assembly(),
        "addr32 {evex} mov zmm0{k3}{z}, zmm1, {rn-sae}"
    );
    i.operands.push(Operand::Register("eax".into()));
    assert_eq!(
        i.assembly(),
        "addr32 {evex} mov zmm0{k3}{z}, zmm1, {rn-sae}, eax"
    );
    i.operands.clear();
    assert_eq!(i.assembly(), "addr32 {evex} mov {rn-sae}");
}

#[test]
fn display_streams_to_a_fallible_writer() {
    use std::fmt::{self, Write};

    struct LimitedWriter {
        remaining: usize,
    }
    impl Write for LimitedWriter {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            self.remaining = self.remaining.checked_sub(text.len()).ok_or(fmt::Error)?;
            Ok(())
        }
    }

    let memory = Operand::Memory {
        bits: 64,
        address_bits: 64,
        base: Some("rax".into()),
        index: Some("rcx".into()),
        scale: 8,
        displacement: -16,
        segment: Some("fs"),
    };
    assert_eq!(memory.to_string(), "qword ptr fs:[rax + rcx*8 - 16]");
    let mut i = instruction(vec![Operand::Register("zmm0".into()), memory]);
    i.evex = Some(EvexOptions {
        mask: Some(3),
        zero: true,
        broadcast: 8,
        ..EvexOptions::default()
    });
    let expected = "{evex} mov zmm0{k3}{z}, qword ptr fs:[rax + rcx*8 - 16]{1to8}";
    assert_eq!(i.to_string(), expected);
    assert_eq!(i.assembly(), expected);
    for remaining in 0..expected.len() {
        assert!(
            write!(LimitedWriter { remaining }, "{i}").is_err(),
            "capacity {remaining}"
        );
    }
    assert!(
        write!(
            LimitedWriter {
                remaining: expected.len()
            },
            "{i}"
        )
        .is_ok()
    );
}
