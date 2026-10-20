#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
extern uint8_t diamond8(uint8_t, uint8_t, uint8_t);
extern uint16_t diamond16(uint8_t, uint16_t, uint16_t);
extern uint32_t diamond32(uint8_t, uint32_t, uint32_t);
extern uint64_t diamond64(uint8_t, uint64_t, uint64_t);
extern uint64_t guarded_div(uint8_t, uint64_t, uint64_t);
extern uint64_t guarded_load(uint8_t, const uint64_t *, uint64_t);
extern const void *edge_pointer(uint8_t, const void *, const void *);
static uint64_t expected(unsigned c, uint64_t a, uint64_t b) {
    return c ? (a+b)^a : (b-a)^b;
}
static void trap_case(unsigned which) {
    pid_t child = fork();
    if (child < 0) abort();
    if (child == 0) {
        switch (which) {
        case 0: guarded_div(1, 5, 0); break;
        case 1: guarded_div(1, UINT64_C(1)<<63, UINT64_MAX); break;
        case 2: guarded_load(1, NULL, 5); break;
        }
        _exit(0);
    }
    int status;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status)) abort();
}
int main(void) {
    uint64_t state = 1;
    unsigned cases = 0;
    for (unsigned i = 0; i < 10000; ++i) {
        state ^= state << 13; state ^= state >> 7; state ^= state << 17;
        uint64_t a = state, b = ~state * 0x9e3779b97f4a7c15ULL;
        unsigned c = (i % 3 == 0) ? 0 : (i % 3 == 1 ? 1 : 255);
        if (diamond8(c,a,b) != (uint8_t)expected(c,(uint8_t)a,(uint8_t)b)) abort();
        if (diamond16(c,a,b) != (uint16_t)expected(c,(uint16_t)a,(uint16_t)b)) abort();
        if (diamond32(c,a,b) != (uint32_t)expected(c,(uint32_t)a,(uint32_t)b)) abort();
        if (diamond64(c,a,b) != expected(c,a,b)) abort();
        cases += 4;
        if (guarded_div(0,a,0) != a || guarded_load(0,NULL,a) != a) abort();
        if (guarded_load(1,&a,0) != a || guarded_div(1,17,3) != 5) abort();
        if (edge_pointer(c,&a,&b) != (c ? &a : &b)) abort();
        cases += 5;
    }
    for (unsigned i = 0; i < 3; ++i) trap_case(i);
    printf("%u independent native diamond/guard/edge oracle cases; 3 taken traps passed\n", cases);
}
