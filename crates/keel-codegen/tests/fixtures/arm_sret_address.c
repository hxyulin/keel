#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef struct { uint64_t a,b; } Pair;
typedef struct { double a,b; } Hfa;
typedef struct { unsigned char bytes[9]; } Nine;
Pair make_pair(uint64_t x) { return (Pair){x^UINT64_C(0xa53c1e729bd8046f),x+UINT64_C(0x8472f9bace135760)}; }
Pair increment(Pair x) { return (Pair){x.a+1,x.b+2}; }
Hfa make_hfa(double x) { return (Hfa){x+1.25,x-3.5}; }
Nine make_nine(uint64_t x) { Nine n; for(unsigned i=0;i<9;i++) n.bytes[i]=(unsigned char)((x>>(8*(i%8)))+i); return n; }
extern uint64_t fixed_pair(uint64_t),external(Pair*,uint64_t),indirect(Pair(*)(uint64_t),uint64_t),dynamic(uint64_t),overlap(uint64_t),fixed_nine(uint64_t);
extern double fixed_hfa(double);
#define CHECK(c) do { if(!(c)) { fprintf(stderr,"oracle line %d\n",__LINE__);return 1; } } while(0)
int main(void) {
 uint64_t state=1;
 for(unsigned j=0;j<20000;j++) {
  state^=state<<13;state^=state>>7;state^=state<<17;
  uint64_t x=state;if(j<5) { const uint64_t e[]={0,1,UINT64_MAX,UINT64_C(1)<<63,(UINT64_C(1)<<63)-1};x=e[j]; }
  Pair p=make_pair(x),out={0,0};
  CHECK(fixed_pair(x)==(p.a^p.b));
  CHECK(external(&out,x)==p.a && out.a==p.a && out.b==p.b);
  CHECK(indirect(make_pair,x)==p.a);
  CHECK(dynamic(x)==(p.a^x));
  CHECK(overlap(x)==((x+1)^(x+19)));
  Nine n=make_nine(x);uint64_t word;memcpy(&word,n.bytes,8);
  CHECK(fixed_nine(x)==(word^n.bytes[8]^UINT64_C(81985529216486895)));
 }
 for(int j=-100;j<100;j++) { double x=j*0.125;Hfa h=make_hfa(x);CHECK(fixed_hfa(x)==h.a+h.b); }
 return 0;
}
