/* Objects with static storage: initializers, tentative definitions,
 * static locals, thread-locals, and arrays sized by their initializer. */
#include <stdio.h>

int counter;
int counter;
extern int later;
static int hidden = 5;
const double scale = 2.5;
char message[] = "msg";
long longs[] = { [3] = 9, 1, [1] = -2 };
unsigned char bytes[6] = { 1, 2 };
_Thread_local int per_thread = 11;
struct config { const char *name; int flags : 4; int level; double weight; } cfg = {
    .name = "cfg", .flags = -3, .level = 2, .weight = 1.0 / 8,
};
struct config *cfg_ptr = &cfg;
int *later_ptr = &later;
int later = 99;
float fl = 3.0f / 4;
_Bool flag = 7;
enum color { RED, GREEN = 5, BLUE } color = BLUE;
int sizes[] = { sizeof(struct config), sizeof message, BLUE * 2 };
/* An array of mutable pointers to const: writable, unlike the strings. */
static const char *dirs[] = { 0, "/tmp" };
static const char *const fixed[] = { "a", "b" };

static int next_id(void) {
    static int id = 100;
    return id++;
}

static int other_next_id(void) {
    static int id;
    return id++;
}

int main(void) {
    counter += hidden;
    per_thread++;
    printf("%d %g %s %zu\n", counter, scale, message, sizeof longs / sizeof longs[0]);
    printf("%ld %ld %ld %ld\n", longs[0], longs[1], longs[3], longs[4]);
    printf("%d %d %d %d\n", bytes[0], bytes[1], bytes[2], (int)sizeof bytes);
    printf("%d %s %d %d %g\n", per_thread, cfg_ptr->name, cfg.flags, cfg.level, cfg.weight);
    printf("%d %g %d %d\n", *later_ptr, fl, flag, color);
    printf("%d %d %d\n", sizes[0], sizes[1], sizes[2]);
    dirs[0] = message;
    printf("%s %s %s\n", dirs[0], dirs[1], fixed[1]);
    next_id();
    printf("%d %d %d\n", next_id(), other_next_id(), other_next_id());
    return 0;
}
