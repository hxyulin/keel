#include <stdint.h>
#include <stddef.h>
extern uint64_t copy_bytes(void*,const void*,uint64_t,uint64_t);
extern void *move_bytes(void*,const void*,uint64_t);
extern uint64_t fill_bytes(void*,int8_t,uint64_t,uint64_t);
extern void *sequence_bytes(void*,const void*,int8_t,uint64_t);
static void seed(unsigned char *p){for(unsigned j=0;j<96;j++)p[j]=(unsigned char)(j*17+11);}
static int same(const unsigned char *a,const unsigned char *b){for(unsigned j=0;j<96;j++)if(a[j]!=b[j])return 0;return 1;}
int main(void){
 const unsigned counts[]={0,1,17,31};
 for(unsigned k=0;k<4;k++){
  unsigned n=counts[k];unsigned char src[96],dst[96],expected[96],snapshot[96];
  seed(src);seed(dst);seed(expected);
  for(unsigned j=0;j<n;j++)expected[7+j]=src[3+j];
  if(copy_bytes(dst+7,src+3,n,UINT64_C(0x9e3779b97f4a7c15))!=((uintptr_t)(dst+7)^UINT64_C(0x9e3779b97f4a7c15)))return 1;
  if(!same(dst,expected))return 2;
  for(unsigned direction=0;direction<2;direction++){
   seed(dst);seed(expected);seed(snapshot);
   unsigned to=direction?3:7,from=direction?7:3;
   for(unsigned j=0;j<n;j++)expected[to+j]=snapshot[from+j];
   if(move_bytes(dst+to,dst+from,n)!=dst+to)return 3;
   if(!same(dst,expected))return 4;
  }
  seed(dst);seed(expected);for(unsigned j=0;j<n;j++)expected[5+j]=(unsigned char)-117;
  if(fill_bytes(dst+5,-117,n,UINT64_C(0x123456789abcdef))!=n+UINT64_C(0x123456789abcdef))return 5;
  if(!same(dst,expected))return 6;
  seed(dst);seed(expected);
  for(unsigned j=0;j<n;j++)expected[9+j]=src[2+j];
  for(unsigned j=0;j<96;j++)snapshot[j]=expected[j];
  for(unsigned j=0;j<n;j++)expected[12+j]=snapshot[9+j];
  if(sequence_bytes(dst+9,src+2,-1,n)!=dst+9)return 7;
  if(!same(dst,expected))return 8;
 }
 return 0;
}
