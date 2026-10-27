# Literal reproduction of /tmp/keel-linux64-keel-aggregate.o's .note.Linux.
.section .note.Linux,"a",@note
.balign 4
.long 6, 1, 0x100
.asciz "Linux"
.balign 4
.byte 0
.balign 4
.long 6, 4, 0x101
.asciz "Linux"
.balign 4
.long 0
.balign 4
.section .text
.globl _start
_start: ret
