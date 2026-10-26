use keel_disasm::{Recognition, decode_word};

#[test]
fn formatted_compiler_instructions_reassemble_to_original_bytes() {
    // No branch relocation or symbolic expression is involved in this corpus.
    let source = "
mov x0, x1
mov x29, sp
mov sp, x29
add x0, x1, #123
add x0, x0, #4095, lsl #12
sub wsp, wsp, #1, lsl #12
add sp, sp, #0, lsl #12
cmp x0, #4095, lsl #12
cmn w0, #1, lsl #12
adds w0, w1, #3
cmp x0, #7
sub sp, sp, #32
add x0, x1, x2, lsl #3
subs w0, w1, w2, asr #2
cmp x1, x2
orr x0, x1, x2, ror #5
and x0, x1, #0xff00ff00ff00ff00
orr w0, w1, #0xff
and sp, x1, #0xff
orr wsp, w1, #0xff
eor sp, x1, #0xff
tst x1, #0xff
movz x0, #123, lsl #32
movk w0, #42, lsl #16
movn x0, #12
sdiv x0, x1, x2
udiv w0, w1, w2
lsl x0, x1, x2
adc x0, x1, x2
clz x0, x1
rev w0, w1
madd x0, x1, x2, x3
msub w0, w1, w2, w3
mul x0, x1, x2
csel x0, x1, x2, eq
csinc w0, w1, w2, ne
ubfm x0, x1, #3, #7
sbfm w0, w1, #0, #7
extr x0, x1, x2, #4
ldr x0, [sp, #16]
str w0, [x1, #4]
ldrb w0, [x1, #3]
strh w0, [x1, #6]
ldrsb x0, [x1, #7]
ldrsh w0, [x1, #8]
ldrsw x0, [x1, #12]
ldur x0, [x1, #-3]
ldr x0, [x1, #8]!
str w0, [x1], #4
ldr x0, [x1, x2, lsl #3]
ldr w0, [x1, w2, sxtw #2]
stp x29, x30, [sp, #-16]!
ldp x29, x30, [sp], #16
stp d0, d1, [sp, #-16]!
ldr q0, [sp, #16]
fadd d0, d1, d2
fmul s0, s1, s2
fneg d0, d1
fmadd d0, d1, d2, d3
fcsel s0, s1, s2, eq
br x1
blr x2
ret x3
nop
svc #0
brk #123
";
    let input = keel_asm::assemble(source).unwrap();
    for &bytes in input.text.as_chunks::<4>().0 {
        let instruction = decode_word(bytes, 0);
        assert!(
            matches!(instruction.recognition, Recognition::Unique(_)),
            "{instruction:?}"
        );
        let text = instruction.assembly();
        assert!(!text.starts_with(".inst"), "{instruction:?}");
        let output = keel_asm::assemble(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(output.text, bytes, "{text}: {instruction:?}");
    }
}
