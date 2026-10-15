extern "C" int k_catch(int);
extern "C" int k_cleanup(int);
extern "C" int k_two(int, int, int);
extern "C" int cleanups;

extern "C" int c_may_throw(int x) {
    if (x < 0) throw -x;
    if (x == 1000) throw (long)x;
    return x * 2;
}

int main() {
    if (k_catch(5) != 11 || k_catch(-3) != 1007 ||
        k_catch(1000) != -1 || k_cleanup(4) != 8) return 1;
    try {
        k_cleanup(-9);
        return 2;
    } catch (int e) {
        if (e != 9) return 3;
    }
    if (cleanups != 91) return 4;
    if (k_two(1, 2, 3) != 12 || k_two(-1, 2, 3) != 100 ||
        k_two(1, 2, -3) != 200) return 5;
    try {
        k_two(1, -2, 3);
        return 6;
    } catch (int e) {
        if (e != 2) return 7;
    }
    return 0;
}
