/* A 4 KiB test mapping at VA 4 MiB, outside the low boot/payload image. */
.text
.code64
.global guest_main
.extern invalidate
guest_main:
    pushq $2
    popfq
    push %rbx
    push %r12
    sub $8, %rsp
    lea bss_end(%rip), %rax
    cmp $0x400000, %rax
    jae setup_error
    /* Walk the trusted boot identity map to its page directory. */
    mov %cr3, %rax
    and $-4096, %rax
    mov (%rax), %rax
    and $-4096, %rax
    mov (%rax), %rax
    and $-4096, %rax
    lea 16(%rax), %rbx
    mov (%rbx), %r12
    lea page_a(%rip), %rax
    or $3, %rax
    mov %rax, test_pt(%rip)
    lea test_pt(%rip), %rax
    or $3, %rax
    mov %rax, (%rbx)
    mov %cr3, %rax
    mov %rax, %cr3
    mov $0x400000, %eax
    mov (%rax), %rax
    movabs $0x123456789abcdef0, %rcx
    cmp %rcx, %rax
    jne mismatch
    /* Fill the translation, then change only the PTE. The generated payload
     * must invalidate the containing page for its final-byte address. */
    lea page_b(%rip), %rax
    or $3, %rax
    mov %rax, test_pt(%rip)
    mov $0x400fff, %edi
    lea output(%rip), %rsi
    movabs $0xfedcba9876543210, %rdx
    call invalidate
    mov $0x400000, %eax
    mov (%rax), %rax
    movabs $0x0fedcba987654321, %rcx
    cmp %rcx, %rax
    jne mismatch
    lea output(%rip), %rsi
    lea expected(%rip), %rdi
    mov $32, %ecx
    cld
    repe cmpsb
    jne mismatch
    /* Address-only operands need no present mapping and must not be read. */
.macro address_only address, image
    lea output(%rip), %rdi
    mov $32, %ecx
    mov $0xa5, %al
    cld
    rep stosb
    movabs $\address, %rdi
    lea output(%rip), %rsi
    movabs $0xfedcba9876543210, %rdx
    call invalidate
    lea output(%rip), %rsi
    lea \image(%rip), %rdi
    mov $32, %ecx
    cld
    repe cmpsb
    jne mismatch
.endm
    address_only 0x401fff, expected_unmapped
    address_only 0x0000800000000000, expected_noncanonical
    xor %edx, %edx
    jmp restore
mismatch:
    mov $1, %edx
restore:
    mov %r12, (%rbx)
    mov %cr3, %rax
    mov %rax, %cr3
    mov %edx, %eax
    jmp cleanup
setup_error:
    mov $1, %eax
cleanup:
    add $8, %rsp
    pop %r12
    pop %rbx
    ret
.data
.balign 16
output:
    .fill 32, 1, 0xa5
.balign 4096
page_a:
    .quad 0x123456789abcdef0
    .zero 4088
.balign 4096
page_b:
    .quad 0x0fedcba987654321
    .zero 4088
.section .rodata,"a"
expected:
    .quad 0xfedcba9876543210, 0x400fff
    .fill 16, 1, 0xa5
expected_unmapped:
    .quad 0xfedcba9876543210, 0x401fff
    .fill 16, 1, 0xa5
expected_noncanonical:
    .quad 0xfedcba9876543210, 0x0000800000000000
    .fill 16, 1, 0xa5
.bss
.balign 4096
test_pt: .skip 4096
.section .note.GNU-stack,"",@progbits
