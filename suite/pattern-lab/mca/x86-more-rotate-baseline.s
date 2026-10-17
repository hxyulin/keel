# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    movq %rsi, %rax
    shrq $13, %rsi
    shlq $51, %rax
    orq %rax, %rsi
    xorq %r8, %rsi
# LLVM-MCA-END
