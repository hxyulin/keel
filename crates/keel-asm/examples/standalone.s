/* A hand-written leaf function using named macros and repetition. */
.text
.p2align 2
.macro increment reg
    add \reg, \reg, #1
.endm

.globl _answer
_answer:
.cfi_startproc
    mov w0, #0b101000
    .rept 2
        increment w0
    .endr
    ret
.cfi_endproc
.subsections_via_symbols
