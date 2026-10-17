# Straight-line loop-body extract; excludes counter and branch.
# LLVM-MCA-BEGIN body
    add x4, x4, #1
    add x5, x5, #1
    add x6, x6, #1
    add x7, x7, #1
    add x4, x4, #1
    add x5, x5, #1
    add x6, x6, #1
    add x7, x7, #1
# LLVM-MCA-END
