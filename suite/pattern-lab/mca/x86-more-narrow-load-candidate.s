# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    movq %rsi, %rax
    shrq $8, %rax
    andq %rcx, %rax
    movzbl (%rdx,%rax,8), %esi
# LLVM-MCA-END
