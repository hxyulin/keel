#include "pair.h"
#include <assert.h>
#include <stdio.h>
uint64_t helper0(Pair p, uint8_t tail, uint64_t last, Pair q) { return (p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback0(Pair p, uint8_t tail, uint64_t last, Pair q) { return (p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper1(uint8_t x0, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback1(uint8_t x0, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper2(uint64_t x0, uint8_t x1, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback2(uint64_t x0, uint8_t x1, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper3(uint64_t x0, uint64_t x1, uint8_t x2, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback3(uint64_t x0, uint64_t x1, uint8_t x2, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper4(uint64_t x0, uint64_t x1, uint64_t x2, uint8_t x3, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback4(uint64_t x0, uint64_t x1, uint64_t x2, uint8_t x3, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper5(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint8_t x4, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback5(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint8_t x4, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper6(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint8_t x5, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback6(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint8_t x5, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper7(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint8_t x6, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback7(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint8_t x6, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper8(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint8_t x7, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback8(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint8_t x7, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper9(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint64_t x7, uint8_t x8, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + (uint64_t)x8*11ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback9(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint64_t x7, uint8_t x8, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + (uint64_t)x8*11ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
uint64_t helper10(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint64_t x7, uint64_t x8, uint8_t x9, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + (uint64_t)x8*11ULL + (uint64_t)x9*12ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) + 101; }
static uint64_t callback10(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint64_t x7, uint64_t x8, uint8_t x9, Pair p, uint8_t tail, uint64_t last, Pair q) { return ((uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + (uint64_t)x8*11ULL + (uint64_t)x9*12ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17) ^ 0xfedcba9876543210ULL; }
int main(void) {
for (uint64_t k=0; k<128; ++k) {
Pair p={k*0x123456789abcdefULL, ~k}, q={k+17, k*31};
{
uint64_t original = p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test0(p, (uint8_t)k, k*19, q, callback0) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((uint8_t)(k+0))*3ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((uint8_t)(k+0))*3ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test1((uint8_t)(k+0), p, (uint8_t)k, k*19, q, callback1) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((uint8_t)(k+1))*4ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((uint8_t)(k+1))*4ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test2((k+0), (uint8_t)(k+1), p, (uint8_t)k, k*19, q, callback2) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((uint8_t)(k+2))*5ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((uint8_t)(k+2))*5ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test3((k+0), (k+1), (uint8_t)(k+2), p, (uint8_t)k, k*19, q, callback3) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((uint8_t)(k+3))*6ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((uint8_t)(k+3))*6ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test4((k+0), (k+1), (k+2), (uint8_t)(k+3), p, (uint8_t)k, k*19, q, callback4) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((uint8_t)(k+4))*7ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((uint8_t)(k+4))*7ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test5((k+0), (k+1), (k+2), (k+3), (uint8_t)(k+4), p, (uint8_t)k, k*19, q, callback5) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((uint8_t)(k+5))*8ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((uint8_t)(k+5))*8ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test6((k+0), (k+1), (k+2), (k+3), (k+4), (uint8_t)(k+5), p, (uint8_t)k, k*19, q, callback6) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((uint8_t)(k+6))*9ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((uint8_t)(k+6))*9ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test7((k+0), (k+1), (k+2), (k+3), (k+4), (k+5), (uint8_t)(k+6), p, (uint8_t)k, k*19, q, callback7) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((k+6))*9ULL + (uint64_t)((uint8_t)(k+7))*10ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((k+6))*9ULL + (uint64_t)((uint8_t)(k+7))*10ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test8((k+0), (k+1), (k+2), (k+3), (k+4), (k+5), (k+6), (uint8_t)(k+7), p, (uint8_t)k, k*19, q, callback8) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((k+6))*9ULL + (uint64_t)((k+7))*10ULL + (uint64_t)((uint8_t)(k+8))*11ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((k+6))*9ULL + (uint64_t)((k+7))*10ULL + (uint64_t)((uint8_t)(k+8))*11ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test9((k+0), (k+1), (k+2), (k+3), (k+4), (k+5), (k+6), (k+7), (uint8_t)(k+8), p, (uint8_t)k, k*19, q, callback9) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
{
uint64_t original = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((k+6))*9ULL + (uint64_t)((k+7))*10ULL + (uint64_t)((k+8))*11ULL + (uint64_t)((uint8_t)(k+9))*12ULL + p.a*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + q.b*17;
uint64_t changed = (uint64_t)((k+0))*3ULL + (uint64_t)((k+1))*4ULL + (uint64_t)((k+2))*5ULL + (uint64_t)((k+3))*6ULL + (uint64_t)((k+4))*7ULL + (uint64_t)((k+5))*8ULL + (uint64_t)((k+6))*9ULL + (uint64_t)((k+7))*10ULL + (uint64_t)((k+8))*11ULL + (uint64_t)((uint8_t)(k+9))*12ULL + (p.a ^ 0x87654321ULL)*3 + p.b*5 + (uint64_t)(uint8_t)k*7 + k*19*11 + q.a*13 + (q.b+9)*17;
assert(test10((k+0), (k+1), (k+2), (k+3), (k+4), (k+5), (k+6), (k+7), (k+8), (uint8_t)(k+9), p, (uint8_t)k, k*19, q, callback10) == (original ^ (changed+101) ^ (changed ^ 0xfedcba9876543210ULL)));
}
}
puts("1408 aggregate boundary patterns passed");
return 0;
}
