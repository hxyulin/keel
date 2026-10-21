#include "pair.h"
uint64_t test0(Pair p, uint8_t tail, uint64_t last, Pair q, Callback0 cb) {
uint64_t own = p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper0(p, tail, last, q) ^ cb(p, tail, last, q);
}
uint64_t test1(uint8_t x0, Pair p, uint8_t tail, uint64_t last, Pair q, Callback1 cb) {
uint64_t own = (uint64_t)x0*3ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper1(x0, p, tail, last, q) ^ cb(x0, p, tail, last, q);
}
uint64_t test2(uint64_t x0, uint8_t x1, Pair p, uint8_t tail, uint64_t last, Pair q, Callback2 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper2(x0, x1, p, tail, last, q) ^ cb(x0, x1, p, tail, last, q);
}
uint64_t test3(uint64_t x0, uint64_t x1, uint8_t x2, Pair p, uint8_t tail, uint64_t last, Pair q, Callback3 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper3(x0, x1, x2, p, tail, last, q) ^ cb(x0, x1, x2, p, tail, last, q);
}
uint64_t test4(uint64_t x0, uint64_t x1, uint64_t x2, uint8_t x3, Pair p, uint8_t tail, uint64_t last, Pair q, Callback4 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper4(x0, x1, x2, x3, p, tail, last, q) ^ cb(x0, x1, x2, x3, p, tail, last, q);
}
uint64_t test5(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint8_t x4, Pair p, uint8_t tail, uint64_t last, Pair q, Callback5 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper5(x0, x1, x2, x3, x4, p, tail, last, q) ^ cb(x0, x1, x2, x3, x4, p, tail, last, q);
}
uint64_t test6(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint8_t x5, Pair p, uint8_t tail, uint64_t last, Pair q, Callback6 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper6(x0, x1, x2, x3, x4, x5, p, tail, last, q) ^ cb(x0, x1, x2, x3, x4, x5, p, tail, last, q);
}
uint64_t test7(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint8_t x6, Pair p, uint8_t tail, uint64_t last, Pair q, Callback7 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper7(x0, x1, x2, x3, x4, x5, x6, p, tail, last, q) ^ cb(x0, x1, x2, x3, x4, x5, x6, p, tail, last, q);
}
uint64_t test8(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint8_t x7, Pair p, uint8_t tail, uint64_t last, Pair q, Callback8 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper8(x0, x1, x2, x3, x4, x5, x6, x7, p, tail, last, q) ^ cb(x0, x1, x2, x3, x4, x5, x6, x7, p, tail, last, q);
}
uint64_t test9(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint64_t x7, uint8_t x8, Pair p, uint8_t tail, uint64_t last, Pair q, Callback9 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + (uint64_t)x8*11ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper9(x0, x1, x2, x3, x4, x5, x6, x7, x8, p, tail, last, q) ^ cb(x0, x1, x2, x3, x4, x5, x6, x7, x8, p, tail, last, q);
}
uint64_t test10(uint64_t x0, uint64_t x1, uint64_t x2, uint64_t x3, uint64_t x4, uint64_t x5, uint64_t x6, uint64_t x7, uint64_t x8, uint8_t x9, Pair p, uint8_t tail, uint64_t last, Pair q, Callback10 cb) {
uint64_t own = (uint64_t)x0*3ULL + (uint64_t)x1*4ULL + (uint64_t)x2*5ULL + (uint64_t)x3*6ULL + (uint64_t)x4*7ULL + (uint64_t)x5*8ULL + (uint64_t)x6*9ULL + (uint64_t)x7*10ULL + (uint64_t)x8*11ULL + (uint64_t)x9*12ULL + p.a*3 + p.b*5 + (uint64_t)tail*7 + last*11 + q.a*13 + q.b*17;
p.a ^= 0x87654321ULL; q.b += 9;
return own ^ helper10(x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, p, tail, last, q) ^ cb(x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, p, tail, last, q);
}
