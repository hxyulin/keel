# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    madd x1, x1, x5, x6
# LLVM-MCA-END
