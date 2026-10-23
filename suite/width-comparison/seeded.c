/* Dedicated initial-sum/range control; unsigned addition wraps, widening is signed. */
typedef unsigned long long U;
typedef long long I;
U seeded(const int *p, I start, I end, U sum) {
    for (I i = start; i < end; ++i) sum += (U)(I)p[i];
    return sum;
}
