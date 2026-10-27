unsigned long goto_rw_output(int branch, unsigned long initial,
    unsigned long a, unsigned long b, unsigned long c, unsigned long d,
    unsigned long e, unsigned long f, unsigned long g, unsigned long h) {
    unsigned long value=initial;
    asm goto("addq $7, %0; testl %k1, %k1; jnz %l[taken]"
             : "+&r"(value) : "r"(branch) : "cc" : taken);
    return value+a+3*b+5*c+7*d+11*e+13*f+17*g+19*h+101;
taken:
    return value+a+3*b+5*c+7*d+11*e+13*f+17*g+19*h+103;
}
int goto_tied_output(int branch,int value) {
    int result;
    asm goto("addl $11, %0; testl %k2, %k2; jnz %l3"
             : "=&r"(result) : "0"(value), "r"(branch) : "cc" : taken);
    return result+13;
taken: return result+17;
}
int goto_numeric_rw_output(int branch,int value) {
    asm goto("addl $19, %0; testl %k1, %k1; jnz %l3"
             : "+&r"(value) : "r"(branch) : "cc" : taken);
    return value+23;
taken: return value+29;
}
int goto_memory_outputs(int branch,int *p,int value) {
    asm goto("addl $31, %0; addl $37, %1; testl %k2, %k2; jnz %l5"
             : "+m"(*p), "+&r"(value) : "r"(branch) : "cc" : taken);
    return *p+value+41;
taken: return *p+value+43;
}
int goto_flag_output(int branch) {
    int zero,result;
    asm goto("movl $47, %1; testl %k2, %k2; jnz %l[taken]"
             : "=@ccz"(zero), "=&r"(result) : "r"(branch) : : taken);
    return result+zero+53;
taken: return result+zero+59;
}
int goto_backward_output(int n) {
again:
    asm goto("subl $1, %0; jnz %l[again]" : "+r"(n) : : "cc" : again);
    return n;
}

int goto_output_address(int branch,int *p) {
    int *cursor=p;
    asm goto("movl $61, %0; testl %k1, %k1; jnz %l[taken]"
             : "=&r"(*cursor++) : "r"(branch) : "cc" : taken);
    return *p+(cursor-p)+67;
taken: return *p+(cursor-p)+71;
}
