extern long edge_locations(long, long, long);
extern long empty_branch(long, long, long);
extern long empty_switch(long, long, long);
int main(void) {
    if (edge_locations(1, 7, 11) != 14 || edge_locations(0, 7, 11) != 18) return 1;
    if (empty_branch(1, 7, 11) != -4 || empty_branch(0, 7, 11) != 18) return 2;
    for (long i = 0; i < 6; i++) {
        if (empty_switch(i, 7, 11) != (i >= 1 && i <= 4 ? 18 : -4)) return 3;
    }
    return 0;
}
