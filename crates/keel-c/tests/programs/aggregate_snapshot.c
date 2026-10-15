#include <stdio.h>
struct S { long a, b; };
static struct S source = {3,4}, destination;
static int mutate_destination(void) { destination.a = 9; return 0; }
static int mutate_source(void) { source.a = 9; return 0; }
static int consume(struct S value, int ignored) { (void)ignored; return value.a; }
static int (*choose_consumer(void))(struct S, int) {
    source.a = 17;
    return consume;
}
int main(void) {
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
