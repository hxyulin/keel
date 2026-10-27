static int log_value;
static void record(int *p) { log_value = log_value * 10 + *p; }

static int backward(void) {
    int count = 0;
    int a __attribute__((cleanup(record))) = 1;
again:;
    int b __attribute__((cleanup(record))) = 2;
    int c __attribute__((cleanup(record))) = 3;
    if (++count < 3) goto again;
    return count;
}
static void forward(int n) {
    int a __attribute__((cleanup(record))) = 1;
    if (n) goto same;
    a = 2;
same:
    goto end;
    { int skipped __attribute__((cleanup(record))) = 3; }
end:;
}
static void siblings(void) {
    int a __attribute__((cleanup(record))) = 1;
    { int b __attribute__((cleanup(record))) = 2; goto other; }
    { other:; int c __attribute__((cleanup(record))) = 3; }
}
static void statement_expression(void) {
    int a __attribute__((cleanup(record))) = 1;
    (void) ({
        int b __attribute__((cleanup(record))) = 2;
        (void) ({ int c __attribute__((cleanup(record))) = 3; goto done; 0; });
        0;
    });
done:;
}
static void statement_expression_backward(void) {
    int n = 0;
    int result = ({
        int a __attribute__((cleanup(record))) = 1;
again:;
        int b __attribute__((cleanup(record))) = 2;
        if (++n < 2) goto again;
        n;
    });
    if (result != 2) log_value = 999;
}
static int loops(void) {
    for (int i = 0; i < 3; ++i) {
        int a __attribute__((cleanup(record))) = 1;
        { int b __attribute__((cleanup(record))) = 2; goto leave; }
leave:
        if (i == 0) continue;
        switch (i) {
            case 1: { int c __attribute__((cleanup(record))) = 3; goto next; }
            default: { int d __attribute__((cleanup(record))) = 4; break; }
        }
next:
        if (i == 2) break;
    }
    return log_value;
}
static int computed(int n) {
    int a __attribute__((cleanup(record))) = 1;
    void *p = n ? &&first : &&second;
    int visits = 0;
    goto *({ int b __attribute__((cleanup(record)))=2; ++visits; p; });
first:
    return visits == 1 ? 3 : 99;
second:
    return visits == 1 ? 4 : 99;
}
static void literal_computed(void) {
    int a __attribute__((cleanup(record))) = 1;
    { int b __attribute__((cleanup(record))) = 2; goto *&&done; }
done:;
}
static void for_initializer(void) {
    for (int a __attribute__((cleanup(record)))=1; a; ++a) {
        int b __attribute__((cleanup(record)))=2;
        goto done;
    }
done:;
}
static void uninitialized_declaration(void) {
again:;
    int a __attribute__((cleanup(record)));
    a = 2;
    if (!log_value) goto again;
}
int test(void) {
    log_value = 0;
    if (backward() != 3 || log_value != 3232321) return 1;
    log_value = 0; forward(1); if (log_value != 1) return 2;
    log_value = 0; forward(0); if (log_value != 2) return 3;
    log_value = 0; siblings(); if (log_value != 231) return 4;
    log_value = 0; statement_expression(); if (log_value != 321) return 5;
    log_value = 0; statement_expression_backward(); if (log_value != 221) return 6;
    log_value = 0; if (loops() != 21231241) return 7;
    log_value = 0; if (computed(1) != 3 || log_value != 21) return 8;
    log_value = 0; if (computed(0) != 4 || log_value != 21) return 9;
    log_value = 0; literal_computed(); if (log_value != 21) return 10;
    log_value = 0; uninitialized_declaration(); if (log_value != 22) return 11;
    log_value = 0; for_initializer(); if (log_value != 21) return 12;
    return 0;
}
