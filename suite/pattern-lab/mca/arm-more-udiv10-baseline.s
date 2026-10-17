# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    udiv x1, x1, x5
    add x1, x1, x6
# LLVM-MCA-END
