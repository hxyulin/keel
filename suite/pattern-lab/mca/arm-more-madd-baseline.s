# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    mul x4, x1, x5
    add x1, x4, x6
# LLVM-MCA-END
