struct Entry { unsigned lo:3; unsigned hi:5; char text[4]; int values[2]; };
static struct Entry table[5] = {
    [1 ... 3] = {.hi = 17, .lo = 5, .text = "abc", .values = {[1] = 23}},
    [2].values[0] = 11,
    [4 ... 4] = {.text = "z", .values = {7, 8}}
};
static int inferred[] = {[2 ... 4] = 9, 12};
static char strings[3][4] = {[0 ... 1] = "xy", [2 ... 2] = "z"};
static int calls;
int tick(void) { return ++calls; }
int main(void) {
    struct Entry local[3] = {
        [0 ... 1] = {.lo = 3, .hi = 21, .text = "hi", .values = {4, 5}},
        [2 ... 2].values = {6, 7}
    };
    int single[3] = {[1 ... 1] = tick(), tick()};
    if (calls != 2 || single[0] || single[1] != 1 || single[2] != 2) return 1;
    // Preserve current Keel evaluation order for replicated expressions.
    // GNU's evaluate-once behavior would be a separate semantic change.
    int repeated[3] = {[0 ... 2] = tick()};
    if (calls != 5 || repeated[0] != 3 || repeated[1] != 4 || repeated[2] != 5) return 2;
    if (sizeof(inferred) != 6 * sizeof(int) || inferred[1] || inferred[2] != 9 || inferred[4] != 9 || inferred[5] != 12) return 3;
    if (table[0].lo || table[1].lo != 5 || table[3].hi != 17 || table[3].text[2] != 'c' || table[3].text[3]) return 4;
    if (table[1].values[0] || table[2].values[0] != 11 || table[3].values[1] != 23 || table[4].text[0] != 'z' || table[4].values[1] != 8) return 5;
    if (local[0].lo != 3 || local[1].hi != 21 || local[1].text[1] != 'i' || local[1].values[1] != 5 || local[2].values[0] != 6 || local[2].lo) return 6;
    if (strings[0][0] != 'x' || strings[1][1] != 'y' || strings[1][2] || strings[2][0] != 'z') return 7;
    int matrix[2][3] = {[0 ... 1] = {[1 ... 2] = 19}};
    if (matrix[0][0] || matrix[1][0] || matrix[0][2] != 19 || matrix[1][1] != 19) return 8;
    struct Entry empty[2] = {[0 ... 1] = {}};
    if (empty[0].hi || empty[1].values[1]) return 9;
    return 0;
}
