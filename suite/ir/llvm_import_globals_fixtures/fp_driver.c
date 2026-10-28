#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
static uint64_t bits64(double x) { uint64_t b; memcpy(&b,&x,8); return b; }
static uint32_t bits32(float x) { uint32_t b; memcpy(&b,&x,4); return b; }
extern double fp_negative_zero;
extern const double fp_nan;
extern const float fp_subnormal;
extern struct { unsigned char tag; double value; float values[2]; } fp_nested;
extern struct { double value; float values[2]; } fp_zero;
extern float *const fp_field;
extern double (*fp_callback)(double);
extern double fp_apply_global(double);
extern void *fp_field_address(void);
double fp_external = 3.5;
double fp_external_callback(double x) { return x * 2.0; }
static double alternate(double x) { return x - 1.0; }
int main(void) {
 assert(bits64(fp_negative_zero)==UINT64_C(0x8000000000000000));
 assert(bits64(fp_nan)==UINT64_C(0x7ff0000000000001));
 assert(bits32(fp_subnormal)==1);
 assert(fp_nested.tag==7 && offsetof(__typeof__(fp_nested),value)==8);
 assert(bits64(fp_nested.value)==UINT64_C(0x8000000000000000));
 assert(bits32(fp_nested.values[0])==UINT32_C(0x7f800001));
 assert(fp_nested.values[1]==1.5f && fp_field==&fp_nested.values[1]);
 assert(fp_field_address()==fp_field);
 assert(bits64(fp_zero.value)==0 && bits32(fp_zero.values[0])==0 && bits32(fp_zero.values[1])==0);
 assert(fp_callback==fp_external_callback && fp_apply_global(2.0)==7.5);
 fp_callback=alternate;
 assert(fp_apply_global(2.0)==4.5);
 fp_external=-4.0;
 assert(fp_apply_global(2.0)==-3.0);
 fp_negative_zero=9.0;
 assert(fp_negative_zero==9.0);
 puts("floating global assertions passed");
}
