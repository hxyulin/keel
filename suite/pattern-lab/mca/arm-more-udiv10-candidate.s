# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    umulh x1, x1, x7
    lsr x1, x1, #3
    add x1, x1, x6
# LLVM-MCA-END
