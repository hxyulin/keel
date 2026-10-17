# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    movsbq %sil, %rsi
    addq %r8, %rsi
# LLVM-MCA-END
