/* Ordinary C, no restrict/alignment/overflow contracts. All totals wrap unsigned. */
typedef unsigned long long U;
typedef long long I;
typedef unsigned int W;
typedef int S;

U rk_positive32(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += a[i] > 0 ? (W)a[i] : 0;
    return sum;
}
U rk_balance32(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += a[i] < 0 ? (U)(I)a[i] : 7;
    return sum;
}
U rk_positive64(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += b[i] > 0 ? (U)b[i] : 0;
    return sum;
}
U rk_balance64(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += b[i] < 0 ? (U)b[i] : 13;
    return sum;
}
/* Sensor values stored in low 16 bits; signed offsets widen before adding. */
U rk_sensor_mix(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += (U)((I)(unsigned short)u[i] + (I)a[i]);
    return sum;
}
U rk_clamped(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) {
        S x = a[i];
        sum += x < -128 ? 0 : x > 127 ? 255 : (U)(x + 128);
    }
    return sum;
}
U rk_unsigned_high(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += u[i] > 9223372036854775807ULL ? u[i] : 3;
    return sum;
}
U rk_adjacent(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 1; i < n; ++i) {
        I d = (I)a[i] - (I)a[i - 1];
        sum += (U)(d < 0 ? -d : d);
    }
    return sum;
}
U rk_buckets(const S *a, const I *b, const U *u, U n) {
    U bins[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    for (U i = 0; i < n; ++i) {
        W slot = (W)a[i] & 7;
        bins[slot] += b[i] < 0 ? u[i] : 1;
    }
    U sum = 0;
    for (U j = 0; j < 8; ++j) sum += bins[j] * (j + 1);
    return sum;
}
U rk_run_balance(const S *a, const I *b, const U *u, U n) {
    U sum = 0, balance = 0;
    for (U i = 0; i < n; ++i) {
        if (a[i] > 0) balance += (W)a[i];
        else balance = balance > 17 ? balance - 17 : 0;
        sum += balance;
    }
    return sum;
}

/* Direct signed/unsigned widening isolates mixed-width reduction coverage. */
U rk_widen32(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += (U)(I)a[i];
    return sum;
}
U rk_widen_u32(const S *a, const I *b, const U *u, U n) {
    U sum = 0;
    for (U i = 0; i < n; ++i) sum += (W)a[i];
    return sum;
}
