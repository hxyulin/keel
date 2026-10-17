# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    lsl x1, x1, #56
    asr x1, x1, #56
    add x1, x1, x6
# LLVM-MCA-END
