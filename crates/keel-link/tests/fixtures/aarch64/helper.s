.text
.global answer
.type answer, %function
answer:
    adrp x1, value
    add x1, x1, :lo12:value
    ldr w0, [x1]
    adrp x2, :got:value
    ldr x2, [x2, :got_lo12:value]
    ldr w3, [x2]
    cmp w0, w3
    b.ne failure
    cbz w0, failure
    tbz w0, #1, failure
    adr x4, answer
    ldr x5, pointer
    cmp x1, x5
    b.ne failure
    adrp x6, vector
    ldr q0, [x6, :lo12:vector]
    ldrb w7, [x6, :lo12:vector]
    ldrh w7, [x6, :lo12:vector]
    ldr w7, [x6, :lo12:vector]
    ldr x7, [x6, :lo12:vector]
    ret
failure:
    mov w0, #99
    ret
.size answer, .-answer
.data
.p2align 3
pointer: .xword value
value: .word 42
.p2align 4
vector: .xword 1, 2
.bss
.p2align 3
scratch: .zero 32
.comm shared, 16, 16
