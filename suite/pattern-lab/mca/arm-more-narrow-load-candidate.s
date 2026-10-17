# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    lsr x4, x1, #8
    and x4, x4, x3
    add x4, x2, x4, lsl #3
    ldrb w1, [x4]
# LLVM-MCA-END
