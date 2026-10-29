#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
typedef void (*fma_fn)(const void*,const void*,const void*,void*);
typedef void (*half_fn)(const void*,void*);
static unsigned csr(void){unsigned v;__asm__ volatile("stmxcsr %0":"=m"(v));return v;}
static void setcsr(unsigned v){__asm__ volatile("ldmxcsr %0"::"m"(v));}
extern void fmadd_ps128(const void*,const void*,const void*,void*),reference_fmadd_ps128(const void*,const void*,const void*,void*);
extern void fmadd_pd128(const void*,const void*,const void*,void*),reference_fmadd_pd128(const void*,const void*,const void*,void*);
extern void fmadd_ps256(const void*,const void*,const void*,void*),reference_fmadd_ps256(const void*,const void*,const void*,void*);
extern void fmadd_pd256(const void*,const void*,const void*,void*),reference_fmadd_pd256(const void*,const void*,const void*,void*);
extern void fmadd_ss128(const void*,const void*,const void*,void*),reference_fmadd_ss128(const void*,const void*,const void*,void*);
extern void fmadd_sd128(const void*,const void*,const void*,void*),reference_fmadd_sd128(const void*,const void*,const void*,void*);
extern void fmsub_ps128(const void*,const void*,const void*,void*),reference_fmsub_ps128(const void*,const void*,const void*,void*);
extern void fmsub_pd128(const void*,const void*,const void*,void*),reference_fmsub_pd128(const void*,const void*,const void*,void*);
extern void fmsub_ps256(const void*,const void*,const void*,void*),reference_fmsub_ps256(const void*,const void*,const void*,void*);
extern void fmsub_pd256(const void*,const void*,const void*,void*),reference_fmsub_pd256(const void*,const void*,const void*,void*);
extern void fmsub_ss128(const void*,const void*,const void*,void*),reference_fmsub_ss128(const void*,const void*,const void*,void*);
extern void fmsub_sd128(const void*,const void*,const void*,void*),reference_fmsub_sd128(const void*,const void*,const void*,void*);
extern void fnmadd_ps128(const void*,const void*,const void*,void*),reference_fnmadd_ps128(const void*,const void*,const void*,void*);
extern void fnmadd_pd128(const void*,const void*,const void*,void*),reference_fnmadd_pd128(const void*,const void*,const void*,void*);
extern void fnmadd_ps256(const void*,const void*,const void*,void*),reference_fnmadd_ps256(const void*,const void*,const void*,void*);
extern void fnmadd_pd256(const void*,const void*,const void*,void*),reference_fnmadd_pd256(const void*,const void*,const void*,void*);
extern void fnmadd_ss128(const void*,const void*,const void*,void*),reference_fnmadd_ss128(const void*,const void*,const void*,void*);
extern void fnmadd_sd128(const void*,const void*,const void*,void*),reference_fnmadd_sd128(const void*,const void*,const void*,void*);
extern void fnmsub_ps128(const void*,const void*,const void*,void*),reference_fnmsub_ps128(const void*,const void*,const void*,void*);
extern void fnmsub_pd128(const void*,const void*,const void*,void*),reference_fnmsub_pd128(const void*,const void*,const void*,void*);
extern void fnmsub_ps256(const void*,const void*,const void*,void*),reference_fnmsub_ps256(const void*,const void*,const void*,void*);
extern void fnmsub_pd256(const void*,const void*,const void*,void*),reference_fnmsub_pd256(const void*,const void*,const void*,void*);
extern void fnmsub_ss128(const void*,const void*,const void*,void*),reference_fnmsub_ss128(const void*,const void*,const void*,void*);
extern void fnmsub_sd128(const void*,const void*,const void*,void*),reference_fnmsub_sd128(const void*,const void*,const void*,void*);
extern void fmaddsub_ps128(const void*,const void*,const void*,void*),reference_fmaddsub_ps128(const void*,const void*,const void*,void*);
extern void fmaddsub_pd128(const void*,const void*,const void*,void*),reference_fmaddsub_pd128(const void*,const void*,const void*,void*);
extern void fmaddsub_ps256(const void*,const void*,const void*,void*),reference_fmaddsub_ps256(const void*,const void*,const void*,void*);
extern void fmaddsub_pd256(const void*,const void*,const void*,void*),reference_fmaddsub_pd256(const void*,const void*,const void*,void*);
extern void fmsubadd_ps128(const void*,const void*,const void*,void*),reference_fmsubadd_ps128(const void*,const void*,const void*,void*);
extern void fmsubadd_pd128(const void*,const void*,const void*,void*),reference_fmsubadd_pd128(const void*,const void*,const void*,void*);
extern void fmsubadd_ps256(const void*,const void*,const void*,void*),reference_fmsubadd_ps256(const void*,const void*,const void*,void*);
extern void fmsubadd_pd256(const void*,const void*,const void*,void*),reference_fmsubadd_pd256(const void*,const void*,const void*,void*);
extern void ph128_0(const void*,void*),reference_ph128_0(const void*,void*);
extern void ph256_0(const void*,void*),reference_ph256_0(const void*,void*);
extern void ph128_1(const void*,void*),reference_ph128_1(const void*,void*);
extern void ph256_1(const void*,void*),reference_ph256_1(const void*,void*);
extern void ph128_2(const void*,void*),reference_ph128_2(const void*,void*);
extern void ph256_2(const void*,void*),reference_ph256_2(const void*,void*);
extern void ph128_3(const void*,void*),reference_ph128_3(const void*,void*);
extern void ph256_3(const void*,void*),reference_ph256_3(const void*,void*);
extern void ph128_4(const void*,void*),reference_ph128_4(const void*,void*);
extern void ph256_4(const void*,void*),reference_ph256_4(const void*,void*);
extern void ph128_5(const void*,void*),reference_ph128_5(const void*,void*);
extern void ph256_5(const void*,void*),reference_ph256_5(const void*,void*);
extern void ph128_6(const void*,void*),reference_ph128_6(const void*,void*);
extern void ph256_6(const void*,void*),reference_ph256_6(const void*,void*);
extern void ph128_7(const void*,void*),reference_ph128_7(const void*,void*);
extern void ph256_7(const void*,void*),reference_ph256_7(const void*,void*);
extern void ph128_255(const void*,void*),reference_ph128_255(const void*,void*);
extern void ph256_255(const void*,void*),reference_ph256_255(const void*,void*);
extern void ps128(const void*,void*),reference_ps128(const void*,void*);
extern void ps256(const void*,void*),reference_ps256(const void*,void*);
extern float scalar_ps(unsigned short),reference_scalar_ps(unsigned short);
extern unsigned short scalar_ph(float),reference_scalar_ph(float);
int main(void){unsigned original=csr();int status=0;
fma_fn actual[]={fmadd_ps128,fmadd_pd128,fmadd_ps256,fmadd_pd256,fmadd_ss128,fmadd_sd128,fmsub_ps128,fmsub_pd128,fmsub_ps256,fmsub_pd256,fmsub_ss128,fmsub_sd128,fnmadd_ps128,fnmadd_pd128,fnmadd_ps256,fnmadd_pd256,fnmadd_ss128,fnmadd_sd128,fnmsub_ps128,fnmsub_pd128,fnmsub_ps256,fnmsub_pd256,fnmsub_ss128,fnmsub_sd128,fmaddsub_ps128,fmaddsub_pd128,fmaddsub_ps256,fmaddsub_pd256,fmsubadd_ps128,fmsubadd_pd128,fmsubadd_ps256,fmsubadd_pd256};
fma_fn expected[]={reference_fmadd_ps128,reference_fmadd_pd128,reference_fmadd_ps256,reference_fmadd_pd256,reference_fmadd_ss128,reference_fmadd_sd128,reference_fmsub_ps128,reference_fmsub_pd128,reference_fmsub_ps256,reference_fmsub_pd256,reference_fmsub_ss128,reference_fmsub_sd128,reference_fnmadd_ps128,reference_fnmadd_pd128,reference_fnmadd_ps256,reference_fnmadd_pd256,reference_fnmadd_ss128,reference_fnmadd_sd128,reference_fnmsub_ps128,reference_fnmsub_pd128,reference_fnmsub_ps256,reference_fnmsub_pd256,reference_fnmsub_ss128,reference_fnmsub_sd128,reference_fmaddsub_ps128,reference_fmaddsub_pd128,reference_fmaddsub_ps256,reference_fmaddsub_pd256,reference_fmsubadd_ps128,reference_fmsubadd_pd128,reference_fmsubadd_ps256,reference_fmsubadd_pd256};
unsigned widths[]={16,16,32,32,16,16,16,16,32,32,16,16,16,16,32,32,16,16,16,16,32,32,16,16,16,16,32,32,16,16,32,32};
unsigned doubles[]={0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1};

for(unsigned rc=0;rc<4;rc++) for(unsigned ftz=0;ftz<2;ftz++) for(unsigned f=0;f<32;f++) for(unsigned pattern=0;pattern<8;pattern++) {
 unsigned char a[32],b[32],c[32],out[48],ref[48];
 if(doubles[f]) {
  const double edges[]={1.0,-0.0,0x1.0000000000001p0,0x1.fffffffffffffp0,0x1p-1022,0x1p-1074,0x1p1023,-0x1p1023};
  for(unsigned lane=0;lane<4;lane++){double av=edges[(pattern+lane)%8],bv=edges[(pattern*3+lane+1)%8],cv=edges[(pattern+lane+2)%8];memcpy(a+lane*8,&av,8);memcpy(b+lane*8,&bv,8);memcpy(c+lane*8,&cv,8);}
 }else {
  const float edges[]={1.0f,-0.0f,0x1.000002p0f,0x1.fffffep0f,0x1p-126f,0x1p-149f,0x1p127f,-0x1p127f};
  for(unsigned lane=0;lane<8;lane++){float av=edges[(pattern+lane)%8],bv=edges[(pattern*3+lane+1)%8],cv=edges[(pattern+lane+2)%8];memcpy(a+lane*4,&av,4);memcpy(b+lane*4,&bv,4);memcpy(c+lane*4,&cv,4);}
 }
 /* Scalar FMA must copy, not evaluate, the non-low lanes, even SNaNs. */
 if(f<24 && f%6>=4) {
  if(doubles[f]) { uint64_t snan=UINT64_C(0x7ff0000000000001);memcpy(a+8,&snan,8);memcpy(b+8,&snan,8);memcpy(c+8,&snan,8); }
  else { uint32_t snan=UINT32_C(0x7f800001);for(unsigned lane=1;lane<4;lane++){memcpy(a+4*lane,&snan,4);memcpy(b+4*lane,&snan,4);memcpy(c+4*lane,&snan,4);} }
 }
 unsigned mode=0x1f80|(rc<<13)|(ftz<<15);memset(out,0xa5,48);memset(ref,0xa5,48);
 setcsr(mode);actual[f](a,b,c,out+1);unsigned flags=csr()&63;
 setcsr(mode);expected[f](a,b,c,ref+1);unsigned reference_flags=csr()&63;
 if(memcmp(out,ref,48)||flags!=reference_flags){printf("FMA f=%u rc=%u ftz=%u pattern=%u flags=%u/%u\n",f,rc,ftz,pattern,flags,reference_flags);status=1;goto done;}
}

half_fn ph_actual[]={ph128_0,ph256_0,ph128_1,ph256_1,ph128_2,ph256_2,ph128_3,ph256_3,ph128_4,ph256_4,ph128_5,ph256_5,ph128_6,ph256_6,ph128_7,ph256_7,ph128_255,ph256_255};
half_fn ph_expected[]={reference_ph128_0,reference_ph256_0,reference_ph128_1,reference_ph256_1,reference_ph128_2,reference_ph256_2,reference_ph128_3,reference_ph256_3,reference_ph128_4,reference_ph256_4,reference_ph128_5,reference_ph256_5,reference_ph128_6,reference_ph256_6,reference_ph128_7,reference_ph256_7,reference_ph128_255,reference_ph256_255};

for(unsigned rc=0;rc<4;rc++)for(unsigned f=0;f<18;f++) {
 float input[]={0.0f,-0.0f,1.00048828125f,-1.00048828125f,65520.0f,-65520.0f,0x1p-25f,0x1p-149f};
 unsigned char out[32],ref[32];memset(out,0xa5,32);memset(ref,0xa5,32);
 setcsr(0x1f80|(rc<<13));ph_actual[f](input,out+1);unsigned flags=csr()&63;
 setcsr(0x1f80|(rc<<13));ph_expected[f](input,ref+1);unsigned reference_flags=csr()&63;
 if(memcmp(out,ref,32)||flags!=reference_flags){printf("F16C ph=%u rc=%u flags=%u/%u\n",f,rc,flags,reference_flags);status=2;goto done;}
}
for(unsigned base=0;base<65536;base+=8){
 uint16_t input[8];for(unsigned lane=0;lane<8;lane++)input[lane]=(uint16_t)(base+lane);
 unsigned char out[48],ref[48];
 for(unsigned width=0;width<2;width++){
  memset(out,0xa5,48);memset(ref,0xa5,48);setcsr(0x1f80);
  if(width)ps256(input,out+1);else ps128(input,out+1);unsigned flags=csr()&63;
  setcsr(0x1f80);if(width)reference_ps256(input,ref+1);else reference_ps128(input,ref+1);unsigned reference_flags=csr()&63;
  if(memcmp(out,ref,48)||flags!=reference_flags){printf("F16C ps width=%u base=%u\n",width,base);status=3;goto done;}
 }
 unsigned char av[4],bv[4];setcsr(0x1f80);float a=scalar_ps(input[0]);memcpy(av,&a,4);setcsr(0x1f80);float b=reference_scalar_ps(input[0]);memcpy(bv,&b,4);if(memcmp(av,bv,4)){status=4;goto done;}
}
for(unsigned rc=0;rc<4;rc++){setcsr(0x1f80|(rc<<13));unsigned short a=scalar_ph(1.00048828125f);setcsr(0x1f80|(rc<<13));unsigned short b=reference_scalar_ph(1.00048828125f);if(a!=b){status=5;goto done;}}
done:setcsr(original);if(!status)puts("FMA/F16C vendor oracle PASS");return status;}
