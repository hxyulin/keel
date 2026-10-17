# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    movq %rsi, %rax
    xorq %rdx, %rdx
    divq %r9
    movq %rax, %rsi
    addq %r8, %rsi
# LLVM-MCA-END
