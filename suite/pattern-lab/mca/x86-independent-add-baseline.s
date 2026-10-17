# Straight-line loop-body extract; excludes counter and branch.
# LLVM-MCA-BEGIN body
    addq $1, %r8
    addq $1, %r9
    addq $1, %r10
    addq $1, %r11
    addq $1, %r8
    addq $1, %r9
    addq $1, %r10
    addq $1, %r11
# LLVM-MCA-END
