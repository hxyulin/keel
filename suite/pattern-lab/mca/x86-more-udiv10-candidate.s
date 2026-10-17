# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    movq %rsi, %rax
    mulq %r9
    shrq $3, %rdx
    movq %rdx, %rsi
    addq %r8, %rsi
# LLVM-MCA-END
