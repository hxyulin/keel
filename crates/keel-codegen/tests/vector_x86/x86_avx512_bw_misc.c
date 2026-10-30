#include <immintrin.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
@DECLS@
@REFS@
typedef void (*fn)(const void*,const void*,const void*,void*,uint64_t);
struct test {const char*name;fn run,ref;unsigned bytes,op,alias,control;};
static const struct test tests[]={@TABLE@};
static unsigned difference(unsigned a,unsigned b){return a>b?a-b:b-a;}
static void model(const struct test*t,const unsigned char*s,const unsigned char*a,const unsigned char*b,unsigned char*out,uint64_t k){
 unsigned n=t->bytes,op=t->op,unit=(op==1||op==3||op==5||op==7)?2:op==6?8:1;
 const unsigned char*merge=t->alias==2?a:s;if(t->alias==1)b=a;
 memcpy(out,merge,n);
 if(op==6){for(unsigned i=0;i<n;i+=8){uint64_t sum=0;for(unsigned j=0;j<8;j++)sum+=difference(a[i+j],b[i+j]);memcpy(out+i,&sum,8);}return;}
 /* Intel SDM VDBPSADBW: shuffle B dwords within each 128-bit lane,
    then slide four-byte windows within each 64-bit superblock. */
 unsigned char shuffled[64];
 if(op==7)for(unsigned block=0;block<n;block+=16)for(unsigned d=0;d<4;d++)memcpy(shuffled+block+4*d,b+block+4*((t->control>>(2*d))&3),4);
 for(unsigned i=0;i<n/unit;i++){
  if(op==4||op==5){memcpy(out+i*unit,((k>>i)&1?b:a)+i*unit,unit);continue;}
  if(!((k>>i)&1))continue;
  if(op==0){unsigned v=a[i];out[i]=(unsigned char)(v>=128?256-v:v);}
  else if(op==1){uint16_t v;memcpy(&v,a+2*i,2);v=v>=32768?(uint16_t)(0u-v):v;memcpy(out+2*i,&v,2);}
  else if(op==2||op==3)memcpy(out+i*unit,a,unit);
  else if(op==7){unsigned block=(i/4)*8,q=i%4;uint16_t sum=0;for(unsigned j=0;j<4;j++)sum+=difference(a[block+4*(q/2)+j],shuffled[block+q+j]);memcpy(out+2*i,&sum,2);}
 }
}
int main(void){
 unsigned saved=_mm_getcsr();
 for(unsigned ti=0;ti<sizeof(tests)/sizeof(*tests);ti++){
  const struct test*t=tests+ti;unsigned n=t->bytes,op=t->op,lanes=n/((op==1||op==3||op==5||op==7)?2:op==6?8:1);
  for(unsigned sample=0;sample<8;sample++){
   unsigned char s[64],a[64],b[64];
   for(unsigned i=0;i<n;i++){s[i]=(unsigned char)(0x93+11*i);a[i]=(unsigned char)(sample*67+i*31+(i/16)*97);b[i]=(unsigned char)(sample*23+i*17+(i/8)*53);}
   if(sample==0){memset(a,0,n);a[0]=0x80;a[1]=0x80;a[n-1]=255;}
   if(sample==1){for(unsigned i=0;i<n;i+=2){a[i]=0;a[i+1]=0x80;}memset(b,255,n);}
   uint64_t active=lanes==64?UINT64_MAX:(UINT64_C(1)<<lanes)-1;
   uint64_t masks[]={0,active,UINT64_C(0xaaaaaaaaaaaaaaaa),UINT64_C(0x5555555555555555),1,UINT64_C(1)<<(lanes-1)};
   for(unsigned mi=0;mi<sizeof masks/sizeof(*masks);mi++){
    uint64_t k=masks[mi];unsigned char out[512],ref[512],expected[64];memset(out,0xa5,sizeof out);memset(ref,0xa5,sizeof ref);
    unsigned csr=(saved|63)&~0x80u;_mm_setcsr(csr);t->run(s,a,b,out,k);if(_mm_getcsr()!=csr)return 1;t->ref(s,a,b,ref,k);
    if(memcmp(out,ref,sizeof out)){fprintf(stderr,"%s vendor mismatch sample%u mask%llx\n",t->name,sample,(unsigned long long)k);return 2;}
    model(t,s,a,b,expected,k);if(memcmp(out,expected,n)){fprintf(stderr,"%s scalar mismatch sample%u mask%llx\n",t->name,sample,(unsigned long long)k);for(unsigned i=0;i<n;i++)fprintf(stderr,"%u:%u/%u ",i,out[i],expected[i]);return 3;}
    if(memcmp(out+64,s,n)||memcmp(out+128,a,n)||memcmp(out+192,b,n)||memcmp(out+256,&k,8))return 4;
   }
  }
 }
 _mm_setcsr(saved);return 0;
}
