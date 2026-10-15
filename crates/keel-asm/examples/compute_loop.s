// Sum 1..6, then double it. Returns 42 on Apple Silicon macOS.
.text
.globl _main
.p2align 2
_main:
    mov x0, #0
    mov x1, #6
Lsum:
    add x0, x0, x1
    subs x1, x1, #1
    b.ne Lsum
    lsl x0, x0, #1
    ret
