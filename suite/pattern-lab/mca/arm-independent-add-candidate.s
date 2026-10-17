# Straight-line loop-body extract; excludes counter and branch.
# LLVM-MCA-BEGIN body
    add x4, x4, #2
    add x5, x5, #2
    add x6, x6, #2
    add x7, x7, #2
# LLVM-MCA-END
