# Straight-line loop-body extract; constants ready; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    shlq $56, %rsi
    sarq $56, %rsi
    addq %r8, %rsi
# LLVM-MCA-END
