# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    rorq $13, %rsi
    xorq %r8, %rsi
# LLVM-MCA-END
