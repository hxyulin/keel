#include <stdint.h>
#include <stdlib.h>
uint64_t external_value=73;
uint64_t external_add(uint64_t x){return x+19;}
extern void *address_local(void),*address_external(void),*address_weak(void),*address_hidden(void);
extern uint64_t address_function(uint64_t),choose8(uint8_t,uint64_t,uint64_t),switch_loop(uint64_t,uint64_t,uint64_t),switch_cycle(uint64_t,uint64_t,uint64_t);
static int check_symbols_switches(void){
 if(*(uint64_t*)address_local()!=41||address_external()!=&external_value||*(uint64_t*)address_weak()!=55||*(uint64_t*)address_hidden()!=66)return 1;
 const uint64_t cases[]={0,1,126,127,128,129,254,255};
 for(unsigned n=0;n<sizeof(cases)/sizeof(cases[0]);n++){
  uint64_t i=cases[n];
  uint64_t expected=i==255?7-13:i==0?13:i==128?7:i==127?99:13-7;
  if(choose8(i,13,7)!=expected||address_function(i)!=i+19)return 2;
 }
 const uint64_t trips[]={0,1,2,3,8,9};
 for(unsigned n=0;n<sizeof(trips)/sizeof(trips[0]);n++){
  uint64_t i=trips[n];
  if(switch_loop(i,13,7)!=(i&1?7:13)||switch_cycle(i,13,7)!=(i&1?7:13))return 3;
 }
 if(switch_loop(UINT64_MAX,13,7)!=7)return 4;
 return 0;
}
