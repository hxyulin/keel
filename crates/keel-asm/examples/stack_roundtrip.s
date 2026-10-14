// Exercise stack saves, integer/FP memory, writeback, register offsets,
// sign extension, and acquire/release. Returns 42 on Apple Silicon macOS.
.text
.globl _main
.p2align 2
_main:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #64
    movz x9, #42
    str x9, [sp, #16]
    ldr d0, [sp, #16]
    str d0, [sp, #24]
    ldr w0, [sp, #24]
    mov x10, sp
    strb w0, [x10], #1
    ldursb x11, [x10, #-1]
    strh w11, [x10, #1]!
    ldrh w12, [x10]
    movz w13, #2
    str w12, [sp, w13, uxtw #2]
    ldr w0, [sp, x13, lsl #2]
    stlr w0, [sp]
    ldar w0, [sp]
    ldrsw x0, [sp]
    add sp, sp, #64
    ldp x29, x30, [sp], #16
    ret
