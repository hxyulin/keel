# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    lsr x4, x1, #13
    lsl x5, x1, #51
    orr x1, x4, x5
    eor x1, x1, x6
# LLVM-MCA-END
