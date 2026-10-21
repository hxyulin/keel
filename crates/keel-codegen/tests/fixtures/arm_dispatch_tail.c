#include <stdint.h>
#include <signal.h>
#include <sys/wait.h>
#include <sys/resource.h>
#include <unistd.h>
extern uint64_t f32(uint32_t, uint32_t, uint64_t, uint64_t);
extern uint64_t f64(uint64_t, uint32_t, uint64_t, uint64_t);
extern uint64_t mixed32(uint32_t, uint32_t, uint64_t, uint64_t);
extern uint64_t mixed64(uint64_t, uint32_t, uint64_t, uint64_t);
static uint64_t oracle(uint64_t k, unsigned route, uint64_t a, uint64_t b) {
    uint64_t x = route ? a : b, y = route ? b : a;
    switch(k) {
      case 10: return x - y + x;
      case 11: return x + y;
      case 12: return y - x + x;
      case 14: return x - y;
      default: __builtin_trap();
    }
}
int main(void) {
    struct rlimit limit = {0,0}; setrlimit(RLIMIT_CORE, &limit);
    const uint64_t values[] = {0, 1, 91, UINT64_MAX, UINT64_C(0x8000000000000000)};
    const uint64_t keys[] = {10,11,12,14};
    for (unsigned r=0;r<2;r++) for(unsigned k=0;k<4;k++)
      for(unsigned a=0;a<5;a++) for(unsigned b=0;b<5;b++) {
        uint64_t expected=oracle(keys[k],r,values[a],values[b]);
        if(f32(keys[k],r,values[a],values[b])!=expected) return 1;
        if(f64(keys[k],r,values[a],values[b])!=expected) return 2;
        if(mixed32(keys[k],r,values[a],values[b])!=expected) return 7;
        if(mixed64(keys[k],r,values[a],values[b])!=expected) return 8;
      }
    const uint64_t bad[] = {0,9,13,15,UINT32_MAX,UINT64_MAX,UINT64_C(0x10000000a)};
    for(unsigned mixed=0;mixed<2;mixed++) for(unsigned bits=32;bits<=64;bits+=32) for(unsigned r=0;r<2;r++)
      for(unsigned k=0;k<7;k++) {
        if(bits==32 && (uint32_t)bad[k]==10) continue;
        pid_t pid=fork(); if(pid<0) return 3;
        if(pid==0) { if(bits==32) { if(mixed) mixed32(bad[k],r,37,91); else f32(bad[k],r,37,91); }
          else { if(mixed) mixed64(bad[k],r,37,91); else f64(bad[k],r,37,91); } _exit(99); }
        int status; if(waitpid(pid,&status,0)!=pid) return 4;
        if(!WIFSIGNALED(status) || WTERMSIG(status)!=SIGTRAP) return 5;
      }
    // The full-width bound must reject 2^32+10 while i32 consumes its low bits.
    if(f32(UINT64_C(0x10000000a),1,37,91)!=oracle(10,1,37,91)) return 6;
    return 0;
}
