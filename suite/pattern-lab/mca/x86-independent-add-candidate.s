# Straight-line loop-body extract; excludes counter and branch.
# LLVM-MCA-BEGIN body
    addq $2, %r8
    addq $2, %r9
    addq $2, %r10
    addq $2, %r11
# LLVM-MCA-END
