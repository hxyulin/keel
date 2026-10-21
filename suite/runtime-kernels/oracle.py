"""Independent unbounded-integer reference; modulo 2**64 only at output."""
MASK = (1 << 64) - 1
CASES = ('positive32', 'balance32', 'positive64', 'balance64', 'sensor_mix',
         'clamped', 'unsigned_high', 'adjacent', 'buckets', 'run_balance',
         'widen32', 'widen_u32')
LENGTHS = (0, 1, 3, 4, 7, 8, 15, 16, 17, 31, 33, 257, 4096, 65536, 1048576)


def inputs(n, seed):
    if not 0 < seed <= MASK:
        raise ValueError('seed must be a nonzero uint64')
    state = seed

    def next_value():
        nonlocal state
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        return state

    a, b, u = [], [], []
    for i in range(n):
        x, y, z = next_value(), next_value(), next_value()
        x = -(x & ((1 << 31) - 1)) - 1 if x & (1 << 31) else x & ((1 << 31) - 1)
        y = -(y & ((1 << 63) - 1)) - 1 if y >> 63 else y
        if i % 3 == 0:
            x = (-(1 << 31), (1 << 31) - 1, -129, -128, -1, 0, 1, 127, 128)[i // 3 % 9]
            y = (-(1 << 63), (1 << 63) - 1, -1, 0, 1)[i // 3 % 5]
            z = (0, 1, (1 << 63) - 1, 1 << 63, MASK)[i // 3 % 5]
        a.append(x)
        b.append(y)
        u.append(z)
    return a, b, u


def reference(a, b, u):
    # Bucket updates are collapsed into weighted contributions; clamp uses
    # min/max; adjacent differences use arbitrary precision abs.
    balance, run_total = 0, 0
    for x in a:
        balance = balance + x if x > 0 else max(balance - 17, 0)
        run_total += balance
    values = (
        sum(max(x, 0) for x in a),
        sum(x if x < 0 else 7 for x in a),
        sum(max(x, 0) for x in b),
        sum(x if x < 0 else 13 for x in b),
        sum(x + (z % 65536) for x, z in zip(a, u)),
        sum(min(max(x + 128, 0), 255) for x in a),
        sum(z if z >= 1 << 63 else 3 for z in u),
        sum(abs(y - x) for x, y in zip(a, a[1:])),
        sum(((x % 8) + 1) * (z if y < 0 else 1) for x, y, z in zip(a, b, u)),
        run_total,
        sum(a),
        sum(x % (1 << 32) for x in a),
    )
    return dict(zip(CASES, (x & MASK for x in values)))
