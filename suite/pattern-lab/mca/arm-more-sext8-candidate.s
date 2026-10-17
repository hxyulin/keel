# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    sxtb x1, w1
    add x1, x1, x6
# LLVM-MCA-END
