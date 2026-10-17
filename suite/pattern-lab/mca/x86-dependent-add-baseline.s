# Straight-line loop-body extract; excludes counter and branch.
# LLVM-MCA-BEGIN body
    addq $1, %rsi
    addq $1, %rsi
# LLVM-MCA-END
