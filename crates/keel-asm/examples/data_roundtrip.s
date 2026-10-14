// Update mutable data and call through a constant function-pointer table.
.text
.globl _main
.p2align 2
_main:
    stp x29, x30, [sp, #-16]!
    adrp x9, _counter@PAGE
    ldr w0, [x9, _counter@PAGEOFF]
    add w0, w0, #2
    str w0, [x9, _counter@PAGEOFF]
    adrp x10, _table@PAGE
    ldr x10, [x10, _table@PAGEOFF]
    blr x10
    ldp x29, x30, [sp], #16
    ret
_answer:
    adrp x9, _counter@PAGE
    ldr w0, [x9, _counter@PAGEOFF]
    ret
.data
.p2align 2
_counter: .long 40
.section __DATA,__const
.p2align 3
_table: .quad _answer
.zerofill __DATA,__bss,_scratch,64,4
