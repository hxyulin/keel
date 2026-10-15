/* Generic overflow: compare result bits AND flags with Clang, at -O0/-O2.
 * Unsigned construction keeps the test oracle free of signed overflow. */
#include <stdio.h>
#include <limits.h>
#if !__has_builtin(__builtin_add_overflow) || !__has_builtin(__builtin_sub_overflow) || !__has_builtin(__builtin_mul_overflow)
#error generic overflow not advertised
#endif
typedef unsigned __int128 U;
typedef __int128 S;
static U values[26];
static void initialize(void) {
    values[0] = 0;
    values[1] = 1;
    values[2] = 2;
    values[3] = 3;
    values[4] = 7;
    values[5] = 127;
    values[6] = 128;
    values[7] = 255;
    values[8] = 256;
    values[9] = 32767;
    values[10] = 32768;
    values[11] = 65535;
    values[12] = 2147483647U;
    values[13] = 2147483648U;
    values[14] = 4294967295U;
    values[15] = (U)1 << 63;
    values[16] = ((U)1 << 64) - 1;
    values[17] = (U)1 << 64;
    values[18] = ((U)1 << 64) + 1;
    values[19] = (U)1 << 65;
    values[20] = (U)1 << 126;
    values[21] = (U)1 << 127;
    values[22] = ~(U)0 >> 1;
    values[23] = ~(U)0;
    values[24] = ~(U)0 - 1;
    values[25] = ((U)0x123456789abcdef0ULL << 64) | 0xfedcba9876543210ULL;
}
static unsigned long long mix(unsigned long long h, U result, int flag) {
    h = (h ^ (unsigned long long)result) * 0x100000001b3ULL;
    h = (h ^ (unsigned long long)(result >> 64)) * 0x100000001b3ULL;
    return (h ^ (unsigned)flag) * 0x100000001b3ULL;
}
#define DEFINE(A,B,R,N) \
static void check_##N(void) { \
    unsigned long long h = 0xcbf29ce484222325ULL; \
    for (unsigned i = 0; i < sizeof values / sizeof values[0]; ++i) { \
        for (unsigned j = 0; j < sizeof values / sizeof values[0]; ++j) { \
            A a = (A)values[i]; B b = (B)values[j]; R r; int flag; \
            flag = __builtin_add_overflow(a,b,&r); h = mix(h,(U)r,flag); \
            flag = __builtin_sub_overflow(a,b,&r); h = mix(h,(U)r,flag); \
            flag = __builtin_mul_overflow(a,b,&r); h = mix(h,(U)r,flag); \
        } \
    } \
    printf(#N " %llx\n",h); \
}
DEFINE(signed char,signed char,signed char,m_0_0)
DEFINE(signed char,unsigned char,unsigned char,m_0_1)
DEFINE(signed char,short,short,m_0_2)
DEFINE(signed char,unsigned short,unsigned short,m_0_3)
DEFINE(signed char,int,int,m_0_4)
DEFINE(signed char,unsigned,unsigned,m_0_5)
DEFINE(signed char,long,long,m_0_6)
DEFINE(signed char,unsigned long,unsigned long,m_0_7)
DEFINE(signed char,long long,long long,m_0_8)
DEFINE(signed char,unsigned long long,unsigned long long,m_0_9)
DEFINE(signed char,S,S,m_0_10)
DEFINE(signed char,U,U,m_0_11)
DEFINE(unsigned char,signed char,unsigned char,m_1_0)
DEFINE(unsigned char,unsigned char,short,m_1_1)
DEFINE(unsigned char,short,unsigned short,m_1_2)
DEFINE(unsigned char,unsigned short,int,m_1_3)
DEFINE(unsigned char,int,unsigned,m_1_4)
DEFINE(unsigned char,unsigned,long,m_1_5)
DEFINE(unsigned char,long,unsigned long,m_1_6)
DEFINE(unsigned char,unsigned long,long long,m_1_7)
DEFINE(unsigned char,long long,unsigned long long,m_1_8)
DEFINE(unsigned char,unsigned long long,S,m_1_9)
DEFINE(unsigned char,S,U,m_1_10)
DEFINE(unsigned char,U,signed char,m_1_11)
DEFINE(short,signed char,short,m_2_0)
DEFINE(short,unsigned char,unsigned short,m_2_1)
DEFINE(short,short,int,m_2_2)
DEFINE(short,unsigned short,unsigned,m_2_3)
DEFINE(short,int,long,m_2_4)
DEFINE(short,unsigned,unsigned long,m_2_5)
DEFINE(short,long,long long,m_2_6)
DEFINE(short,unsigned long,unsigned long long,m_2_7)
DEFINE(short,long long,S,m_2_8)
DEFINE(short,unsigned long long,U,m_2_9)
DEFINE(short,S,signed char,m_2_10)
DEFINE(short,U,unsigned char,m_2_11)
DEFINE(unsigned short,signed char,unsigned short,m_3_0)
DEFINE(unsigned short,unsigned char,int,m_3_1)
DEFINE(unsigned short,short,unsigned,m_3_2)
DEFINE(unsigned short,unsigned short,long,m_3_3)
DEFINE(unsigned short,int,unsigned long,m_3_4)
DEFINE(unsigned short,unsigned,long long,m_3_5)
DEFINE(unsigned short,long,unsigned long long,m_3_6)
DEFINE(unsigned short,unsigned long,S,m_3_7)
DEFINE(unsigned short,long long,U,m_3_8)
DEFINE(unsigned short,unsigned long long,signed char,m_3_9)
DEFINE(unsigned short,S,unsigned char,m_3_10)
DEFINE(unsigned short,U,short,m_3_11)
DEFINE(int,signed char,int,m_4_0)
DEFINE(int,unsigned char,unsigned,m_4_1)
DEFINE(int,short,long,m_4_2)
DEFINE(int,unsigned short,unsigned long,m_4_3)
DEFINE(int,int,long long,m_4_4)
DEFINE(int,unsigned,unsigned long long,m_4_5)
DEFINE(int,long,S,m_4_6)
DEFINE(int,unsigned long,U,m_4_7)
DEFINE(int,long long,signed char,m_4_8)
DEFINE(int,unsigned long long,unsigned char,m_4_9)
DEFINE(int,S,short,m_4_10)
DEFINE(int,U,unsigned short,m_4_11)
DEFINE(unsigned,signed char,unsigned,m_5_0)
DEFINE(unsigned,unsigned char,long,m_5_1)
DEFINE(unsigned,short,unsigned long,m_5_2)
DEFINE(unsigned,unsigned short,long long,m_5_3)
DEFINE(unsigned,int,unsigned long long,m_5_4)
DEFINE(unsigned,unsigned,S,m_5_5)
DEFINE(unsigned,long,U,m_5_6)
DEFINE(unsigned,unsigned long,signed char,m_5_7)
DEFINE(unsigned,long long,unsigned char,m_5_8)
DEFINE(unsigned,unsigned long long,short,m_5_9)
DEFINE(unsigned,S,unsigned short,m_5_10)
DEFINE(unsigned,U,int,m_5_11)
DEFINE(long,signed char,long,m_6_0)
DEFINE(long,unsigned char,unsigned long,m_6_1)
DEFINE(long,short,long long,m_6_2)
DEFINE(long,unsigned short,unsigned long long,m_6_3)
DEFINE(long,int,S,m_6_4)
DEFINE(long,unsigned,U,m_6_5)
DEFINE(long,long,signed char,m_6_6)
DEFINE(long,unsigned long,unsigned char,m_6_7)
DEFINE(long,long long,short,m_6_8)
DEFINE(long,unsigned long long,unsigned short,m_6_9)
DEFINE(long,S,int,m_6_10)
DEFINE(long,U,unsigned,m_6_11)
DEFINE(unsigned long,signed char,unsigned long,m_7_0)
DEFINE(unsigned long,unsigned char,long long,m_7_1)
DEFINE(unsigned long,short,unsigned long long,m_7_2)
DEFINE(unsigned long,unsigned short,S,m_7_3)
DEFINE(unsigned long,int,U,m_7_4)
DEFINE(unsigned long,unsigned,signed char,m_7_5)
DEFINE(unsigned long,long,unsigned char,m_7_6)
DEFINE(unsigned long,unsigned long,short,m_7_7)
DEFINE(unsigned long,long long,unsigned short,m_7_8)
DEFINE(unsigned long,unsigned long long,int,m_7_9)
DEFINE(unsigned long,S,unsigned,m_7_10)
DEFINE(unsigned long,U,long,m_7_11)
DEFINE(long long,signed char,long long,m_8_0)
DEFINE(long long,unsigned char,unsigned long long,m_8_1)
DEFINE(long long,short,S,m_8_2)
DEFINE(long long,unsigned short,U,m_8_3)
DEFINE(long long,int,signed char,m_8_4)
DEFINE(long long,unsigned,unsigned char,m_8_5)
DEFINE(long long,long,short,m_8_6)
DEFINE(long long,unsigned long,unsigned short,m_8_7)
DEFINE(long long,long long,int,m_8_8)
DEFINE(long long,unsigned long long,unsigned,m_8_9)
DEFINE(long long,S,long,m_8_10)
DEFINE(long long,U,unsigned long,m_8_11)
DEFINE(unsigned long long,signed char,unsigned long long,m_9_0)
DEFINE(unsigned long long,unsigned char,S,m_9_1)
DEFINE(unsigned long long,short,U,m_9_2)
DEFINE(unsigned long long,unsigned short,signed char,m_9_3)
DEFINE(unsigned long long,int,unsigned char,m_9_4)
DEFINE(unsigned long long,unsigned,short,m_9_5)
DEFINE(unsigned long long,long,unsigned short,m_9_6)
DEFINE(unsigned long long,unsigned long,int,m_9_7)
DEFINE(unsigned long long,long long,unsigned,m_9_8)
DEFINE(unsigned long long,unsigned long long,long,m_9_9)
DEFINE(unsigned long long,S,unsigned long,m_9_10)
DEFINE(unsigned long long,U,long long,m_9_11)
DEFINE(S,signed char,S,m_10_0)
DEFINE(S,unsigned char,U,m_10_1)
DEFINE(S,short,signed char,m_10_2)
DEFINE(S,unsigned short,unsigned char,m_10_3)
DEFINE(S,int,short,m_10_4)
DEFINE(S,unsigned,unsigned short,m_10_5)
DEFINE(S,long,int,m_10_6)
DEFINE(S,unsigned long,unsigned,m_10_7)
DEFINE(S,long long,long,m_10_8)
DEFINE(S,unsigned long long,unsigned long,m_10_9)
DEFINE(S,S,long long,m_10_10)
DEFINE(S,U,unsigned long long,m_10_11)
DEFINE(U,signed char,U,m_11_0)
DEFINE(U,unsigned char,signed char,m_11_1)
DEFINE(U,short,unsigned char,m_11_2)
DEFINE(U,unsigned short,short,m_11_3)
DEFINE(U,int,unsigned short,m_11_4)
DEFINE(U,unsigned,int,m_11_5)
DEFINE(U,long,unsigned,m_11_6)
DEFINE(U,unsigned long,long,m_11_7)
DEFINE(U,long long,unsigned long,m_11_8)
DEFINE(U,unsigned long long,long long,m_11_9)
DEFINE(U,S,unsigned long long,m_11_10)
DEFINE(U,U,S,m_11_11)
DEFINE(S,S,signed char,w_10_10_0)
DEFINE(S,S,unsigned char,w_10_10_1)
DEFINE(S,S,short,w_10_10_2)
DEFINE(S,S,unsigned short,w_10_10_3)
DEFINE(S,S,int,w_10_10_4)
DEFINE(S,S,unsigned,w_10_10_5)
DEFINE(S,S,long,w_10_10_6)
DEFINE(S,S,unsigned long,w_10_10_7)
DEFINE(S,S,long long,w_10_10_8)
DEFINE(S,S,unsigned long long,w_10_10_9)
DEFINE(S,S,S,w_10_10_10)
DEFINE(S,S,U,w_10_10_11)
DEFINE(S,U,signed char,w_10_11_0)
DEFINE(S,U,unsigned char,w_10_11_1)
DEFINE(S,U,short,w_10_11_2)
DEFINE(S,U,unsigned short,w_10_11_3)
DEFINE(S,U,int,w_10_11_4)
DEFINE(S,U,unsigned,w_10_11_5)
DEFINE(S,U,long,w_10_11_6)
DEFINE(S,U,unsigned long,w_10_11_7)
DEFINE(S,U,long long,w_10_11_8)
DEFINE(S,U,unsigned long long,w_10_11_9)
DEFINE(S,U,S,w_10_11_10)
DEFINE(S,U,U,w_10_11_11)
DEFINE(U,S,signed char,w_11_10_0)
DEFINE(U,S,unsigned char,w_11_10_1)
DEFINE(U,S,short,w_11_10_2)
DEFINE(U,S,unsigned short,w_11_10_3)
DEFINE(U,S,int,w_11_10_4)
DEFINE(U,S,unsigned,w_11_10_5)
DEFINE(U,S,long,w_11_10_6)
DEFINE(U,S,unsigned long,w_11_10_7)
DEFINE(U,S,long long,w_11_10_8)
DEFINE(U,S,unsigned long long,w_11_10_9)
DEFINE(U,S,S,w_11_10_10)
DEFINE(U,S,U,w_11_10_11)
DEFINE(U,U,signed char,w_11_11_0)
DEFINE(U,U,unsigned char,w_11_11_1)
DEFINE(U,U,short,w_11_11_2)
DEFINE(U,U,unsigned short,w_11_11_3)
DEFINE(U,U,int,w_11_11_4)
DEFINE(U,U,unsigned,w_11_11_5)
DEFINE(U,U,long,w_11_11_6)
DEFINE(U,U,unsigned long,w_11_11_7)
DEFINE(U,U,long long,w_11_11_8)
DEFINE(U,U,unsigned long long,w_11_11_9)
DEFINE(U,U,S,w_11_11_10)
DEFINE(U,U,U,w_11_11_11)

static int lc, rc, pc;
static volatile int stored;
static int left(void) { ++lc; return INT_MAX; }
static unsigned right(void) { ++rc; return 1; }
static volatile int *destination(void) { ++pc; return &stored; }
static void effects(void) {
    int f = __builtin_add_overflow(left(),right(),destination());
    printf("once %d %d %d %d %d\n", f, stored, lc, rc, pc);
    __builtin_sub_overflow(left(),right(),destination());
    printf("unused %d %d %d %d\n", stored, lc, rc, pc);
    int alias = INT_MAX;
    f = __builtin_add_overflow(alias,1,&alias);
    printf("alias %d %d\n",f,alias);
    int a[] = {INT_MAX, 5}; int i = 0, j = 0, k = 0;
    f = __builtin_mul_overflow(a[i++],a[j++],&a[k++]);
    printf("indices %d %d %d %d %d\n", f,a[0],i,j,k);
    unsigned u;
    f = __builtin_add_overflow((_Bool)1,(_Bool)1,&u);
    printf("bool inputs %d %u\n",f,u);
    enum E { NEGATIVE = -1 };
    f = __builtin_add_overflow(NEGATIVE,1U,&u);
    printf("enum input %d %u\n",f,u);
    printf("boolean %d\n", _Generic(__builtin_add_overflow(1,2,&alias), _Bool:1, default:0));
}
int main(void) {
    initialize();
    U state = ((U)0x9876543210fedcbaULL << 64) | 0xabcdef0123456789ULL;
    for (unsigned pass = 0; pass < 2; ++pass) {
        check_m_0_0();
        check_m_0_1();
        check_m_0_2();
        check_m_0_3();
        check_m_0_4();
        check_m_0_5();
        check_m_0_6();
        check_m_0_7();
        check_m_0_8();
        check_m_0_9();
        check_m_0_10();
        check_m_0_11();
        check_m_1_0();
        check_m_1_1();
        check_m_1_2();
        check_m_1_3();
        check_m_1_4();
        check_m_1_5();
        check_m_1_6();
        check_m_1_7();
        check_m_1_8();
        check_m_1_9();
        check_m_1_10();
        check_m_1_11();
        check_m_2_0();
        check_m_2_1();
        check_m_2_2();
        check_m_2_3();
        check_m_2_4();
        check_m_2_5();
        check_m_2_6();
        check_m_2_7();
        check_m_2_8();
        check_m_2_9();
        check_m_2_10();
        check_m_2_11();
        check_m_3_0();
        check_m_3_1();
        check_m_3_2();
        check_m_3_3();
        check_m_3_4();
        check_m_3_5();
        check_m_3_6();
        check_m_3_7();
        check_m_3_8();
        check_m_3_9();
        check_m_3_10();
        check_m_3_11();
        check_m_4_0();
        check_m_4_1();
        check_m_4_2();
        check_m_4_3();
        check_m_4_4();
        check_m_4_5();
        check_m_4_6();
        check_m_4_7();
        check_m_4_8();
        check_m_4_9();
        check_m_4_10();
        check_m_4_11();
        check_m_5_0();
        check_m_5_1();
        check_m_5_2();
        check_m_5_3();
        check_m_5_4();
        check_m_5_5();
        check_m_5_6();
        check_m_5_7();
        check_m_5_8();
        check_m_5_9();
        check_m_5_10();
        check_m_5_11();
        check_m_6_0();
        check_m_6_1();
        check_m_6_2();
        check_m_6_3();
        check_m_6_4();
        check_m_6_5();
        check_m_6_6();
        check_m_6_7();
        check_m_6_8();
        check_m_6_9();
        check_m_6_10();
        check_m_6_11();
        check_m_7_0();
        check_m_7_1();
        check_m_7_2();
        check_m_7_3();
        check_m_7_4();
        check_m_7_5();
        check_m_7_6();
        check_m_7_7();
        check_m_7_8();
        check_m_7_9();
        check_m_7_10();
        check_m_7_11();
        check_m_8_0();
        check_m_8_1();
        check_m_8_2();
        check_m_8_3();
        check_m_8_4();
        check_m_8_5();
        check_m_8_6();
        check_m_8_7();
        check_m_8_8();
        check_m_8_9();
        check_m_8_10();
        check_m_8_11();
        check_m_9_0();
        check_m_9_1();
        check_m_9_2();
        check_m_9_3();
        check_m_9_4();
        check_m_9_5();
        check_m_9_6();
        check_m_9_7();
        check_m_9_8();
        check_m_9_9();
        check_m_9_10();
        check_m_9_11();
        check_m_10_0();
        check_m_10_1();
        check_m_10_2();
        check_m_10_3();
        check_m_10_4();
        check_m_10_5();
        check_m_10_6();
        check_m_10_7();
        check_m_10_8();
        check_m_10_9();
        check_m_10_10();
        check_m_10_11();
        check_m_11_0();
        check_m_11_1();
        check_m_11_2();
        check_m_11_3();
        check_m_11_4();
        check_m_11_5();
        check_m_11_6();
        check_m_11_7();
        check_m_11_8();
        check_m_11_9();
        check_m_11_10();
        check_m_11_11();
        check_w_10_10_0();
        check_w_10_10_1();
        check_w_10_10_2();
        check_w_10_10_3();
        check_w_10_10_4();
        check_w_10_10_5();
        check_w_10_10_6();
        check_w_10_10_7();
        check_w_10_10_8();
        check_w_10_10_9();
        check_w_10_10_10();
        check_w_10_10_11();
        check_w_10_11_0();
        check_w_10_11_1();
        check_w_10_11_2();
        check_w_10_11_3();
        check_w_10_11_4();
        check_w_10_11_5();
        check_w_10_11_6();
        check_w_10_11_7();
        check_w_10_11_8();
        check_w_10_11_9();
        check_w_10_11_10();
        check_w_10_11_11();
        check_w_11_10_0();
        check_w_11_10_1();
        check_w_11_10_2();
        check_w_11_10_3();
        check_w_11_10_4();
        check_w_11_10_5();
        check_w_11_10_6();
        check_w_11_10_7();
        check_w_11_10_8();
        check_w_11_10_9();
        check_w_11_10_10();
        check_w_11_10_11();
        check_w_11_11_0();
        check_w_11_11_1();
        check_w_11_11_2();
        check_w_11_11_3();
        check_w_11_11_4();
        check_w_11_11_5();
        check_w_11_11_6();
        check_w_11_11_7();
        check_w_11_11_8();
        check_w_11_11_9();
        check_w_11_11_10();
        check_w_11_11_11();
        for (unsigned i = 0; i < sizeof values / sizeof values[0]; ++i) {
            state ^= state << 23; state ^= state >> 17; state ^= state << 26;
            values[i] = state;
        }
    }
    effects();
    return 0;
}
