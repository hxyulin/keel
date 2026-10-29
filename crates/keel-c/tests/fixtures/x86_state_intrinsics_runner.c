#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern void state_maskmove(const void*,const void*,void*);
extern unsigned state_csr(unsigned),state_fields(unsigned,unsigned*);
extern void state_zero(const void*,void*),state_stream(const void*,void*,void*),state_prefetch(const void*,unsigned*);
static unsigned csr(void){unsigned value;__asm__ volatile("stmxcsr %0":"=m"(value));return value;}
static void setcsr(unsigned value){__asm__ volatile("ldmxcsr %0"::"m"(value));}
int main(void) {
  unsigned saved=csr();
  for(unsigned rc=0;rc<4;rc++)for(unsigned daz=0;daz<2;daz++)for(unsigned ftz=0;ftz<2;ftz++)for(unsigned flags=0;flags<64;flags+=7) {
    unsigned value=0x1f80|(rc<<13)|(daz<<6)|(ftz<<15)|flags;
    if(state_csr(value)!=value||csr()!=saved){puts("raw MXCSR round trip");return 1;}
    unsigned count=0;setcsr(value);
    unsigned next=state_fields((rc^3)<<13,&count);
    if(next!=((value&~0x6000u)|((rc^3)<<13))||csr()!=value||count!=1){puts("MXCSR field preservation");return 2;}
    setcsr(saved);
  }
  _Alignas(32) unsigned char input[96],out[192],expected[192],loaded[48],zero[96];
  for(unsigned i=0;i<96;i++)input[i]=(unsigned char)(i*71+131);
  memset(out,0xa5,sizeof(out));memset(expected,0xa5,sizeof(expected));
  for(unsigned i=0;i<3;i++)memcpy(expected+i*16,input,16);
  for(unsigned i=0;i<3;i++)memcpy(expected+64+i*32,input,32);
  memcpy(expected+160,input,4);memcpy(expected+168,input,8);
  state_stream(input,out,loaded);
  if(memcmp(out,expected,sizeof(out))||memcmp(loaded,input,16)||memcmp(loaded+16,input,32)){puts("non-temporal transfers");return 3;}
  for(unsigned pattern=0;pattern<256;pattern++) {
    unsigned char mask[16],expected[16];
    for(unsigned i=0;i<16;i++){mask[i]=(unsigned char)(pattern+i*79);expected[i]=(mask[i]&128)?input[i]:0xa5;}
    memset(out,0xa5,32);state_maskmove(input,mask,out+1);
    if(out[0]!=0xa5||out[17]!=0xa5||memcmp(out+1,expected,16)){puts("byte maskmove");return 7;}
  }
  state_zero(input,zero);
  if(memcmp(input,zero,96)){puts("SIMD clear destroyed live values");return 4;}
  long page=sysconf(_SC_PAGESIZE);void*guard=mmap(0,page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(guard==MAP_FAILED)return 5;
  unsigned count=0;state_prefetch(guard,&count);munmap(guard,page);
  if(count!=6||csr()!=saved){puts("prefetch effects");return 6;}
  puts("PASS MXCSR, fields, non-temporal transfers, fences and live SIMD preservation");return 0;
}
