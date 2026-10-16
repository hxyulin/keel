extern signed char ext_s8(signed char);
extern unsigned char ext_u8(unsigned char);
extern short ext_s16(short);
extern unsigned short ext_u16(unsigned short);
extern unsigned long ext_stack(unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,void*,double,double,double,double,double,double,double,double,double,double);
__attribute__((noinline)) signed char own_s8(signed char x) { return ext_s8(x); }
__attribute__((noinline)) unsigned char own_u8(unsigned char x) { return ext_u8(x); }
__attribute__((noinline)) short own_s16(short x) { return ext_s16(x); }
__attribute__((noinline)) unsigned short own_u16(unsigned short x) { return ext_u16(x); }
long abi_small(long x) {
    return own_s8((signed char)x)+own_u8((unsigned char)x)+own_s16((short)x)+own_u16((unsigned short)x);
}
unsigned long abi_stack(unsigned long x, void *p) {
    return ext_stack(x,2,3,4,5,6,7,8,9,10,p,1.25,2.5,3.75,4.,5.,6.,7.,8.,9.,10.);
}
