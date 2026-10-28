.text
.code64
.global invalidate
invalidate:
.ifndef KEEL_OMIT_INVALIDATE
    invlpg (%rdi)
.endif
.ifdef KEEL_BAD_MARKER
    xor %edx, %edx
.endif
.ifdef KEEL_BAD_ADDRESS
    xor %edi, %edi
.endif
    mov %rdx, (%rsi)
    mov %rdi, 8(%rsi)
    ret
.section .note.GNU-stack,"",@progbits
