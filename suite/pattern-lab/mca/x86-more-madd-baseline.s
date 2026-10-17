# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    imulq $3, %rsi, %rsi
    addq $7, %rsi
# LLVM-MCA-END
