// Pinned Clang/GNU headers map their 64-bit wrappers to these builtins.
// No headers/sysroot needed for the cross-compile proof.
void clang_save(void *p,__UINT64_TYPE__ mask){__builtin_ia32_xsave64(p,mask);}
void clang_restore(void *p,__UINT64_TYPE__ mask){__builtin_ia32_xrstor64(p,mask);}
