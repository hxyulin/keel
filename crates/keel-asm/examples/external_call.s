// Calls a function from another object, then calls it through its GOT address.
.text
.globl _main
.extern _answer
.p2align 2
_main:
    stp x29, x30, [sp, #-16]!
    bl _answer
    adrp x16, _answer@GOTPAGE
    ldr x16, [x16, _answer@GOTPAGEOFF]
    blr x16
    ldp x29, x30, [sp], #16
    ret
