#include <stdio.h>
struct S { long a, b; };
static struct S source = {3,4}, destination;
static int mutate_destination(void) { destination.a = 9; return 0; }
static int mutate_source(void) { source.a = 9; return 0; }
__attribute__((noinline)) static int consume(struct S value, int ignored) { (void)ignored; return value.a; }
static int (*choose_consumer(void))(struct S, int) {
    source.a = 17;
    return consume;
}
/* The two calls' snapshots have disjoint lifetimes and may share bytes. */
__attribute__((noinline)) static int consume_twice(void) {
    int first = consume((destination = source), mutate_destination());
    source.a = 5;
    return first * 10 + consume((destination = source), mutate_destination());
}
int main(void) {
    source.a = 3;
    if (consume_twice() != 35) return 4;
    source.a = 3;
    int result = consume((destination = source), mutate_destination());
    if (result != 3) return 1;
    source.a = 3;
    result = consume((destination = source), mutate_source());
    if (result != destination.a) return 2;
    source.a = 3;
    result = choose_consumer()((destination = source), 0);
    if (result != destination.a) return 3;
    puts("aggregate snapshots preserved");
    return 0;
}
