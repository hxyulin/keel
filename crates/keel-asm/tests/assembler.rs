use keel_asm::{assemble, assemble_object};
use object::{Object, ObjectSection, ObjectSymbol};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;
mod common;
use common::compare_clang;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use common::{Temp, execute, run};

#[test]
fn all_supported_forms_match_clang() {
    let mut source = String::from(".text\n.globl _main\n_main:\nLstart:\n");
    for (bits, prefix) in [(32, "w"), (64, "x")] {
        for op in ["movz", "movk", "movn"] {
            for imm in [0, 42, 65535] {
                for shift in (0..bits).step_by(16) {
                    source.push_str(&format!("{op} {prefix}30, #{imm}, lsl #{shift}\n"));
                }
            }
        }
        for op in ["add", "sub", "adds", "subs"] {
            for imm in [0, 42, 4095] {
                for shift in [0, 12] {
                    source.push_str(&format!(
                        "{op} {prefix}0, {prefix}30, #{imm}, lsl #{shift}\n"
                    ));
                }
            }
            for shift in ["lsl", "lsr", "asr"] {
                for amount in [0, bits - 1] {
                    source.push_str(&format!(
                        "{op} {prefix}0, {prefix}30, {prefix}zr, {shift} #{amount}\n"
                    ));
                }
            }
        }
        source.push_str(&format!("mov {prefix}0, {prefix}30\nmov {prefix}zr, {prefix}0\nmov {prefix}0, {prefix}zr\nmov {prefix}0, #42\ncmp {prefix}0, #4095\ncmn {prefix}30, {prefix}0, asr #1\n"));
    }
    source.push_str("mov x29, sp\nmov sp, x29\nmov w0, wsp\nadd sp, sp, #16\nsubs xzr, sp, #16\ncmp sp, #16\nret\nret x0\nret xzr\nnop\nb Lforward\nbl Lstart\nb #-4\nbl #4\nLforward:\n.p2align 4\nret\n");
    compare_clang(&source);
}

#[test]
fn registers_and_immediates_match_clang() {
    let mut source = String::from(".text\n");
    for n in 0..=30 {
        source.push_str(&format!(
            "movz x{n}, #{}\nadd w{n}, w{n}, w{n}\nsub x{n}, x{n}, #{}\nret x{n}\n",
            n * 2113,
            n * 131
        ));
    }
    compare_clang(&source);
}

#[test]
fn local_forward_and_backward_branches_use_instruction_pc() {
    let a = assemble("Lback: nop\nb Lforward\nbl Lback\nLforward: ret").unwrap();
    let words: Vec<_> = a
        .text
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    assert_eq!(words, [0xd503201f, 0x14000002, 0x97fffffe, 0xd65f03c0]);
    compare_clang("Lback: nop\nb Lforward\nbl Lback\nLforward: ret");
}

#[test]
fn branch_range_and_alignment_are_checked() {
    assert!(assemble("b #-134217728\nb #134217724").is_ok());
    for bad in ["b #-134217732", "b #134217728", "b #2", "bl #-2"] {
        assert!(assemble(bad).unwrap_err().message.contains("displacement"));
    }
}

#[test]
fn invalid_operands_and_unsupported_syntax_are_diagnostics() {
    for bad in [
        "movz w0, #65536",
        "movk w0, #0, lsl #32",
        "movn x0, #0, lsl #17",
        "movz sp, #42",
        "add x0, w1, #0",
        "add x0, x1, w2",
        "add x0, x1, #4096",
        "add x0, x1, #-1",
        "add x0, x1, #1, lsl #1",
        "add x0, x1, x2, ror #1",
        "sub w0, w1, w2, lsl #32",
        "add xzr, x0, #0",
        "adds sp, x0, #0",
        "sub x0, sp, x1, lsl #5",
        "mov x0, x31",
        "mov x0, x32",
        "mov x0, sp, #0",
        "ret w0",
        "ret sp",
        "mov x0, #0x123456789abcdef0",
        "mov sp, xzr",
        "nop #0",
        "movz x0, #0,",
        ".cfi_startproc",
        ".section __DATA,__unsupported",
        ".p2align 17",
        ".p2align 2, 0",
        ".byte 256",
        "L: nop\nL: ret",
        "b 1f",
        "1: ret",
        "ld1 {v0.4s}, [x1]",
    ] {
        let error = assemble(&format!("\n{bad}")).expect_err(bad);
        assert!(error.line >= 2, "{bad}: {error}");
        assert!(!error.message.is_empty());
    }
}

#[test]
fn comments_labels_and_case() {
    let a = assemble(
        "// comment\n.text\n.globl _main\n_main: MOVZ W0, #0x2a ; comment\nRET // comment\n",
    )
    .unwrap();
    assert_eq!(a.text, [0x40, 0x05, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6]);
    assert!(a.symbols[0].global);
    assert_eq!(a.symbols[0].name, "_main");
}

#[test]
fn macho_is_readable_and_preserves_symbols_and_alignment() {
    let src = ".text\n.globl _main\n_main: movz w0, #42\n.p2align 5\nLreturn: ret\n";
    let a = assemble(src).unwrap();
    let bytes = a.to_macho().unwrap();
    assert_eq!(bytes, assemble_object(src).unwrap());
    let object = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(object.format(), object::BinaryFormat::MachO);
    assert_eq!(object.architecture(), object::Architecture::Aarch64);
    assert_eq!(object.kind(), object::ObjectKind::Relocatable);
    assert!(object.is_little_endian());
    let section = object.section_by_name("__text").unwrap();
    assert_eq!(section.segment_name().unwrap(), Some("__TEXT"));
    assert_eq!(section.data().unwrap(), a.text);
    assert_eq!(section.align(), 32);
    assert_eq!(section.relocations().count(), 0);
    let main = object
        .symbols()
        .find(|s| s.name().unwrap() == "_main")
        .unwrap();
    assert!(main.is_global());
    assert_eq!(main.address(), 0);
    // Temporary labels remain available in the assembly model, but need no
    // nlist entry unless a relocation refers to them.
    assert_eq!(
        a.symbols
            .iter()
            .find(|s| s.name == "Lreturn")
            .unwrap()
            .offset,
        Some(32)
    );
    assert!(object.symbols().all(|s| s.name().unwrap() != "Lreturn"));
}

#[test]
fn output_is_deterministic() {
    let src = include_str!("../examples/return42.s");
    assert_eq!(assemble_object(src).unwrap(), assemble_object(src).unwrap());
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn apple_linker_runs_return42() {
    let temp = Temp::new();
    let object = temp.0.join("return42.o");
    let executable = temp.0.join("return42");
    std::fs::write(
        &object,
        assemble_object(include_str!("../examples/return42.s")).unwrap(),
    )
    .unwrap();
    run(Command::new("clang")
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(42));
}

/// Cover all scalar transfer sizes, immediate forms, and register extensions.
#[test]
fn single_load_store_forms_match_clang() {
    let mut forms = Vec::new();
    for op in ["ldr", "str"] {
        for (register, bytes) in [
            ("w0", 4),
            ("x0", 8),
            ("b0", 1),
            ("h0", 2),
            ("s0", 4),
            ("d0", 8),
            ("q0", 16),
        ] {
            forms.push((op, register, bytes));
        }
    }
    forms.extend([
        ("ldrb", "w0", 1),
        ("strb", "w0", 1),
        ("ldrh", "w0", 2),
        ("strh", "w0", 2),
        ("ldrsb", "w0", 1),
        ("ldrsb", "x0", 1),
        ("ldrsh", "w0", 2),
        ("ldrsh", "x0", 2),
        ("ldrsw", "x0", 4),
    ]);
    let mut source = String::from(".text\n");
    for (op, register, bytes) in forms {
        source.push_str(&format!("{op} {register}, [sp]\n"));
        for offset in [0, bytes, 4095 * bytes] {
            source.push_str(&format!("{op} {register}, [x30, #{offset}]\n"));
        }
        // LLVM selects the unscaled form for small negative/unaligned offsets.
        let unscaled = op.replace("ldr", "ldur").replace("str", "stur");
        for offset in [-256, -1, 0, 1, 255] {
            source.push_str(&format!(
                "{op} {register}, [x30, #{offset}]\n{unscaled} {register}, [sp, #{offset}]\n"
            ));
            source.push_str(&format!(
                "{op} {register}, [x30, #{offset}]!\n{op} {register}, [sp], #{offset}\n"
            ));
        }
        let scale = (bytes as u32).trailing_zeros();
        source.push_str(&format!("{op} {register}, [x30, xzr]\n"));
        for (index, extend) in [
            ("wzr", "uxtw"),
            ("w2", "sxtw"),
            ("xzr", "lsl"),
            ("x2", "sxtx"),
        ] {
            for shift in [0, scale] {
                source.push_str(&format!(
                    "{op} {register}, [sp, {index}, {extend} #{shift}]\n"
                ));
            }
            if extend != "lsl" {
                source.push_str(&format!("{op} {register}, [x30, {index}, {extend}]\n"));
            }
        }
    }
    compare_clang(&source);
}

#[test]
fn pair_and_ordered_load_store_forms_match_clang() {
    let mut source = String::from(".text\n");
    for op in ["ldp", "stp"] {
        for (a, b, bytes) in [
            ("w0", "wzr", 4),
            ("x0", "xzr", 8),
            ("s0", "s31", 4),
            ("d0", "d31", 8),
            ("q0", "q31", 16),
        ] {
            source.push_str(&format!("{op} {a}, {b}, [sp]\n"));
            for offset in [-64 * bytes, -bytes, 0, bytes, 63 * bytes] {
                source.push_str(&format!("{op} {a}, {b}, [x30, #{offset}]\n{op} {a}, {b}, [sp, #{offset}]!\n{op} {a}, {b}, [x30], #{offset}\n"));
            }
        }
    }
    for op in ["ldar", "stlr"] {
        for register in ["w0", "x0", "wzr", "xzr"] {
            source.push_str(&format!(
                "{op} {register}, [sp]\n{op} {register}, [x30, #0]\n"
            ));
        }
    }
    for op in ["ldarb", "ldarh", "stlrb", "stlrh"] {
        for register in ["w0", "wzr"] {
            source.push_str(&format!(
                "{op} {register}, [sp]\n{op} {register}, [x30, #0]\n"
            ));
        }
    }
    compare_clang(&source);
}

#[test]
fn transfer_register_numbers_and_writeback_roles_match_clang() {
    let mut source = String::from(".text\n");
    for n in 0..=30 {
        source.push_str(&format!("str x{n}, [x{n}]\nldr w{n}, [x{n}]\nstrb w{n}, [sp, #{n}]\nldr d{n}, [x{n}], #8\nstp d{n}, d{n}, [x{n}, #-16]!\n"));
    }
    // FP register 31 exists; it does not share a register with SP or XZR.
    source.push_str(
        "ldr q31, [sp]\nstp xzr, xzr, [sp]\nldr xzr, [sp], #8\nLDR Q31, [SP, XZR, LSL #4]\n",
    );
    compare_clang(&source);
}

#[test]
fn invalid_memory_operands_have_line_diagnostics() {
    for bad in [
        "ldr x0, x1",
        "ldr x0, Ldata",
        "ldr x0, =42",
        "ldr x0, [x1",
        "ldr x0, x1]",
        "ldr x0, [[x1]]",
        "ldr x0, [x1]]",
        "ldr x0, []",
        "ldr x0, [x1,]",
        "ldr x0, [x1] trailing",
        "ldr x0 [x1]",
        "ldr x0, [w1]",
        "ldr x0, [wsp]",
        "ldr x0, [xzr]",
        "str sp, [x1]",
        "ldr x0, [x1, #32768]",
        "ldr x0, [x1, #-257]",
        "ldr x0, [x1, #257]",
        "ldr x0, [x1, #256]!",
        "str w0, [x1], #-257",
        "ldr x0, [x1]!",
        "str x0, [x1, #0]!, #8",
        "str x0, [x1, #8], #8",
        "ldr x0, [x1, x2]!",
        "ldur x0, [x1, x2]",
        "stur x0, [x1], #8",
        "ldur x0, [x1, #0]!",
        "ldur x0, [x1, #256]",
        "ldr x0, [x1, w2]",
        "ldr x0, [x1, sp]",
        "ldr x0, [x1, x31]",
        "ldr x0, [x1, x2, uxtw]",
        "ldr x0, [x1, w2, sxtx]",
        "ldr x0, [x1, x2, lsr #3]",
        "ldr x0, [x1, x2, lsl]",
        "ldr x0, [x1, x2, lsl #2]",
        "ldrb w0, [x1, x2, lsl #1]",
        "ldr q0, [x1, x2, lsl #5]",
        "ldr x0, [x1, x2, sxtx #3 extra]",
        "ldr x0, [x1, #0, lsl #3]",
        "ldr x0, [x1, x2, lsl #3, #0]",
        "ldr x1, [x1, #8]!",
        "str w1, [x1], #4",
        "ldrsb w1, [x1], #1",
        "ldrb x0, [x1]",
        "strh x0, [x1]",
        "ldrsh d0, [x1]",
        "ldrsw w0, [x1]",
        "ldr d32, [x1]",
        "ldr q32, [x1]",
        "str v0, [x1]",
        "ldp b0, b1, [x1]",
        "ldp w0, x2, [x1]",
        "stp x0, d2, [x1]",
        "ldp x0, x0, [x1]",
        "ldp d0, d0, [sp]",
        "ldp xzr, xzr, [sp]",
        "ldp x0, x2, [x1, #512]",
        "stp x0, x2, [x1, #-520]",
        "ldp x0, x2, [x1, #4]",
        "stp x0, x2, [x1, x3]",
        "ldp x0, x1, [x1], #16",
        "stp w0, w1, [x1, #-8]!",
        "ldar x0, [x1, #8]",
        "stlr x0, [x1], #8",
        "stlrb x0, [x1]",
        "ldar d0, [x1]",
        "ldar x0, [x1, #0]!",
        "ldar x0, [x1, x2]",
        "ldxp x0, x2, [x1]",
        "ldnp x0, x2, [x1]",
        "prfm pldl1keep, [x1]",
    ] {
        let error = assemble(&format!("nop\n{bad}")).expect_err(bad);
        assert_eq!(error.line, 2, "{bad}: {error}");
        assert!(!error.message.is_empty());
    }
}

#[test]
fn memory_scaling_and_byte_index_shift_are_encoded() {
    let pairs = [
        ("ldr x0, [x1, #8]", "ldr x0, [x1, #1]"),
        ("ldrb w0, [x1, x2]", "ldrb w0, [x1, x2, lsl #0]"),
        ("ldrb w0, [x1, w2, uxtw]", "ldrb w0, [x1, w2, uxtw #0]"),
        ("ldr b0, [x1, x2]", "ldr b0, [x1, x2, lsl #0]"),
    ];
    for (a, b) in pairs {
        assert_ne!(assemble(a).unwrap().text, assemble(b).unwrap().text);
        compare_clang(&format!("{a}\n{b}\n"));
    }
    assert_eq!(
        assemble("ldr x0, [x1, #1]").unwrap().text,
        assemble("ldur x0, [x1, #1]").unwrap().text
    );
    assert_ne!(
        assemble("ldr x0, [x1]").unwrap().text,
        assemble("ldur x0, [x1]").unwrap().text
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn apple_linker_runs_stack_roundtrip() {
    let source = include_str!("../examples/stack_roundtrip.s");
    compare_clang(source);
    let temp = Temp::new();
    let object = temp.0.join("stack.o");
    let executable = temp.0.join("stack");
    std::fs::write(&object, assemble_object(source).unwrap()).unwrap();
    run(Command::new("clang")
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(42));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn signed_loads_and_simd_copies_execute_correctly() {
    let source = "
.text
.globl _signed_byte32, _signed_byte64, _signed_half32, _signed_half64
.globl _signed_word64, _unscaled_word64, _word_index32, _word_index64
.globl _copy_vector, _copy_vector_pair
_signed_byte32: ldrsb w0, [x0]
    ret
_signed_byte64: ldrsb x0, [x0]
    ret
_signed_half32: ldrsh w0, [x0]
    ret
_signed_half64: ldrsh x0, [x0]
    ret
_signed_word64: ldrsw x0, [x0]
    ret
_unscaled_word64: ldursw x0, [x0, #-4]
    ret
_word_index32: movn w1, #0
    ldrsw x0, [x0, w1, sxtw #2]
    ret
_word_index64: movn x1, #0
    ldrsw x0, [x0, x1, sxtx #2]
    ret
_copy_vector: ldr q31, [x1]
    str q31, [x0]
    ret
_copy_vector_pair: ldp q30, q31, [x1]
    stp q30, q31, [x0]
    ret
";
    let driver = "
#include <stdint.h>
extern int signed_byte32(const int8_t *);
extern long signed_byte64(const int8_t *);
extern int signed_half32(const int16_t *);
extern long signed_half64(const int16_t *);
extern long signed_word64(const int32_t *);
extern long unscaled_word64(const int32_t *);
extern long word_index32(const int32_t *);
extern long word_index64(const int32_t *);
extern void copy_vector(void *, const void *);
extern void copy_vector_pair(void *, const void *);
int main(void) {
    int8_t byte = -83;
    int16_t half = -12345;
    int32_t words[2] = {-1234567, 42};
    if (signed_byte32(&byte) != byte || signed_byte64(&byte) != byte) return 1;
    if (signed_half32(&half) != half || signed_half64(&half) != half) return 2;
    if (signed_word64(words) != words[0] || unscaled_word64(words + 1) != words[0]) return 3;
    if (word_index32(words + 1) != words[0] || word_index64(words + 1) != words[0]) return 4;
    _Alignas(16) unsigned char src[32], dst[32] = {0};
    for (int i = 0; i < 32; ++i) src[i] = (unsigned char)(i * 7 + 3);
    copy_vector(dst, src);
    for (int i = 0; i < 32; ++i) if (dst[i] != (i < 16 ? src[i] : 0)) return 5;
    copy_vector_pair(dst, src);
    for (int i = 0; i < 32; ++i) if (dst[i] != src[i]) return 6;
    return 0;
}
";
    compare_clang(source);
    let temp = Temp::new();
    let object = temp.0.join("memory.o");
    let c = temp.0.join("driver.c");
    let executable = temp.0.join("memory");
    std::fs::write(&object, assemble_object(source).unwrap()).unwrap();
    std::fs::write(&c, driver).unwrap();
    run(Command::new("clang")
        .args(["-std=c11", "-O2"])
        .arg(&c)
        .arg(&object)
        .arg("-o")
        .arg(&executable));
    assert_eq!(execute(&executable).code(), Some(0));
}
