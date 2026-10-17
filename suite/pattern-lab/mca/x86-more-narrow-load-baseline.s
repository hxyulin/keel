# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    movq %rsi, %rax
    shrq $8, %rax
    andq %rcx, %rax
    movq (%rdx,%rax,8), %rsi
    andq $255, %rsi
# LLVM-MCA-END
