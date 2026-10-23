.intel_syntax noprefix
.text
xsave64 [rcx]
xrstor64 [rcx]
xsave64 [r12+64]
xrstor64 [r12+64]
