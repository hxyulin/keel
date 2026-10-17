# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    ror x1, x1, #13
    eor x1, x1, x6
# LLVM-MCA-END
