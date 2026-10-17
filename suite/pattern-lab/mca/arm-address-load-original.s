# Straight-line loop-body extract; excludes counter and branch.
# LLVM-MCA-BEGIN body
    ubfx w4, w1, #16, #8
    and w4, w4, w3
    sxtw x4, w4
    lsl x4, x4, #4
    add x4, x2, x4
    ldr x1, [x4]
# LLVM-MCA-END
