asm(".macro answer\nmovq %rdi,%rax\naddq %rsi,%rax\nxorq $37,%rax\n.endm\n");
unsigned long datum __attribute__((section(".init.data"))) = 19;
unsigned long zeros[4] __attribute__((section(".init.data"), aligned(32)));
static const unsigned long table[2] __attribute__((section(".init.rodata"), used)) = {23,29};
static unsigned long kept __attribute__((used)) = 41;
static unsigned long kept_fn(void) __attribute__((used));
static unsigned long kept_fn(void) { return kept; }
unsigned long target(unsigned long x) __attribute__((section(".init.text")));
unsigned long target(unsigned long x) { return x + datum + table[0]; }
asm(".text\n.globl asm_leaf\n.type asm_leaf,@function\nasm_leaf:\nmovl $3,%ecx\n1:\ndecl %ecx\njnz 1b\nanswer\nret\n.size asm_leaf,.-asm_leaf\n.balign 16,0\n"
    ".weak alias\n.set alias,target\n"
    ".pushsection .keel.exports,\"aw\",@progbits\n.balign 8\n.globl records\nrecords:\n.quad target,datum,kept_fn\n"
    ".pushsection .init.rodata,\"a\",@progbits\n.globl asm_bytes\nasm_bytes:\n.byte 3,5,7,11\n.popsection\n.popsection\n"
    ".pushsection .keel.relative,\"a\",@progbits\n.globl relative_record\nrelative_record:\n.long target-.\n.popsection\n");
extern unsigned long asm_leaf(unsigned long, unsigned long);
extern unsigned long alias(unsigned long);
unsigned long call_assembly(unsigned long a, unsigned long b) { return asm_leaf(a,b) + alias(a); }
