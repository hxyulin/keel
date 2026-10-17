# Straight-line loop-body extract; excludes setup, counter and branch.
# LLVM-MCA-BEGIN body
    lsr x4, x1, #8
    and x4, x4, x3
    ldr x1, [x2, x4, lsl #3]
    and x1, x1, #255
# LLVM-MCA-END
