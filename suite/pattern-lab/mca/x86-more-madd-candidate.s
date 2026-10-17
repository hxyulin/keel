# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    leaq 7(%rsi,%rsi,2), %rsi
# LLVM-MCA-END
