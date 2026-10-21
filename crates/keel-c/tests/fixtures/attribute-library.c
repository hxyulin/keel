/* Representative library header: exported API, format checks, allocation
 * annotations, unused parameters, and capability-gated optional weak hooks.
 * Self contained; no system headers or downloaded project are required. */
#define API __attribute__((__visibility__("default")))
#define PRINTF(a,b) __attribute__((__format__(__printf__, a, b)))
#if defined(__KEEL__) && (__has_attribute(weak) || __has_attribute(alias) || __has_attribute(cleanup) || __has_attribute(constructor) || __has_attribute(visibility) || __has_attribute(used) || __has_attribute(retain) || __has_attribute(section) || __has_attribute(ifunc) || __has_attribute(destructor) || __has_attribute(weakref))
#error unsupported semantic attribute advertised
#endif
extern int log_message(const char *, ...) PRINTF(1, 2);
extern void *allocate(unsigned long) __attribute__((malloc, alloc_size(1), warn_unused_result));
API int library_sum(const int *, int) __attribute__((nonnull(1)));
API int library_sum(const int *values, int count) {
    int sum = 0;
    for (int i = 0; i < count; ++i) sum += values[i];
    return sum;
}
static int helper(int unused __attribute__((unused))) __attribute__((noinline));
static int helper(int unused __attribute__((unused))) { return 12; }
int old_api(void) __attribute__((deprecated("use library_sum")));
int old_api(void) { return helper(0); }
int main(void) {
    int values[] = { 2, 3, 7 };
    return library_sum(values, 3) == old_api() ? 0 : 1;
}
