.subsections_via_symbols
.globl _main
_main:
stp x29,x30,[sp,#-16]!
mov x29,sp
adrp x9,_data@PAGE+8
ldr w10,[x9,_data@PAGEOFF+8]
cmp w10,#3
b.ne Lfail
adrp x9,_data@PAGE-4
ldr w10,[x9,_data@PAGEOFF-4]
cbnz w10,Lfail
adrp x9,_diff@PAGE
ldrsw x10,[x9,_diff@PAGEOFF]
mov x11,#4088
cmp x10,x11
b.ne Lfail
bl _choose+4
b Ldone
Lfail:
mov w0,#1
Ldone:
ldp x29,x30,[sp],#16
ret
.globl _choose
_choose:
mov w0,#1
mov w0,#0
ret
.data
.p2align 12
.globl _block
_block:
.space 4088
.globl _data
_data:
.long 1,2,3,4
_diff:
.long _data-_block
